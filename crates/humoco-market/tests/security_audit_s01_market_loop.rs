use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use human_money_core::crypto::{create_user_id, ed25519_sk_to_x25519_sk};
use human_money_core::services::mnemonic::MnemonicLanguage;
use humoco_market::{
    build_recovery_request_uri, canonicalize_write_url, compute_drop_members_with_stats,
    compute_rehearsal_verifiers, compute_share_commitment, create_social_recovery_vault_bundle,
    derive_delta_prefix, derive_recovery_key, encode_post_slot, guardian_entry_signing_bytes,
    open_and_verify_guardian_release_reply, open_multi_class_slot_blob, open_slot_payload,
    pack_post_slot, parse_ed25519_pubkey_hex64, parse_emergency_kit_uri,
    parse_recovery_request_uri, prepare_post_senders, recover_master_payload_from_shares,
    seal_guardian_release_reply, seal_multi_class_slot_blob, seal_post_box,
    seal_recovery_master_payload, seal_slot_payload, sign_member_request_direct, split_secret_32,
    try_open_post_box_prepared, unseal_recovery_master_payload, validate_kdf_params,
    validate_market_inquiry, validate_market_inquiry_message, validate_post_delivery_receipt,
    validate_sync_prefix, validate_wot_handshake_assertion, verify_guardian_vault_entry,
    verify_rehearsal_answers, DropDescriptor, GuardianLocatorUpdate, GuardianReleasePayload,
    GuardianRevoke, GuardianVaultEntry, GuardianVaultStore, InquiryRateTracker, InquiryTarget,
    MarketError, MarketInquiry, MarketInquiryMessage, MemberPolicy, PostBoxEntry,
    PostDeliveryReceipt, PostEnvelope, RecoveryKdfParams, RecoveryMasterPayload,
    RecoveryRequestLink, WotHandshakeAssertion, DROP_POST_MAX_BYTES, MAX_GUARDIAN_WARDS,
    SLOT_MAGIC, SLOT_MULTI_MAGIC,
};

fn make_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn identity_point_bytes() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[0] = 1;
    bytes
}

fn order_4_point_bytes() -> [u8; 32] {
    [0u8; 32]
}

fn wrap_multi_class(sub_blobs: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(SLOT_MULTI_MAGIC);
    out.push(sub_blobs.len() as u8);
    for sb in sub_blobs {
        out.extend_from_slice(&(sb.len() as u32).to_be_bytes());
        out.extend_from_slice(sb);
    }
    out
}

#[test]
fn s01_f1_rejects_nested_multi_class_slot_and_trailing_bytes() {
    // Invariant: multi-class slots reject nested containers and any trailing bytes after declared payloads.
    let reader_sk = make_key(11);
    let reader_vk = reader_sk.verifying_key();
    let reader_x_sk = ed25519_sk_to_x25519_sk(&reader_sk);

    let single = seal_slot_payload(b"inner-secret", &[reader_vk], 65_536).unwrap();
    assert_eq!(&single[0..8], SLOT_MAGIC);
    assert_eq!(
        open_slot_payload(&single, &reader_x_sk).unwrap().as_deref(),
        Some(b"inner-secret".as_slice())
    );

    // 1. 2-level nested HMSLOTM2 must be rejected cleanly by both open_slot_payload and open_multi_class_slot_blob
    let inner_multi = wrap_multi_class(&[&single]);
    let nested_multi = wrap_multi_class(&[&inner_multi]);
    assert!(
        matches!(
            open_multi_class_slot_blob(&nested_multi, &reader_x_sk),
            Err(MarketError::Slot(ref msg)) if msg.contains("Invalid SlotBlob magic header")
        ),
        "nested multi-class slot must fail single-class magic check"
    );
    assert!(
        matches!(
            open_slot_payload(&nested_multi, &reader_x_sk),
            Err(MarketError::Slot(ref msg)) if msg.contains("Invalid SlotBlob magic header")
        ),
        "open_slot_payload must reject nested multi-class slot with exact magic error"
    );

    // 2. Trailing garbage bytes after a valid multi-class slot must be rejected even if class 0 decrypts
    let mut valid_multi =
        seal_multi_class_slot_blob(&[(b"class-0-secret", &[reader_vk])], 65_536).unwrap();
    assert_eq!(
        open_slot_payload(&valid_multi, &reader_x_sk)
            .unwrap()
            .as_deref(),
        Some(b"class-0-secret".as_slice())
    );
    valid_multi.push(0xAA);
    assert!(
        matches!(
            open_multi_class_slot_blob(&valid_multi, &reader_x_sk),
            Err(MarketError::Slot(ref msg)) if msg.contains("Trailing unexpected bytes in multi-class SlotBlob")
        ),
        "trailing bytes in multi-class slot must be rejected"
    );
    assert!(
        matches!(
            open_slot_payload(&valid_multi, &reader_x_sk),
            Err(MarketError::Slot(ref msg)) if msg.contains("Trailing unexpected bytes in multi-class SlotBlob")
        ),
        "open_slot_payload must reject trailing bytes in multi-class slot"
    );
}

#[test]
fn s01_f2_rejects_small_order_keys_and_reports_payload_tampering() {
    // Invariant: small-order reader keys fail closed and payload AEAD tampering surfaces Err(Crypto), not Ok(None).
    let reader_sk = make_key(21);
    let reader_vk = reader_sk.verifying_key();
    let reader_x_sk = ed25519_sk_to_x25519_sk(&reader_sk);

    let id_vk = VerifyingKey::from_bytes(&identity_point_bytes()).unwrap();
    let ord4_vk = VerifyingKey::from_bytes(&order_4_point_bytes()).unwrap();

    // 1. seal_slot_payload must fail closed when any reader is a small-order key
    assert!(matches!(
        seal_slot_payload(b"secret", &[reader_vk, id_vk], 65_536),
        Err(MarketError::Crypto(ref msg)) if msg.contains("Non-contributory Diffie-Hellman exchange")
    ));
    assert!(matches!(
        seal_slot_payload(b"secret", &[ord4_vk], 65_536),
        Err(MarketError::Crypto(ref msg)) if msg.contains("Non-contributory Diffie-Hellman exchange")
    ));

    // 2. parse_ed25519_pubkey_hex64 must reject weak small-order points
    assert!(matches!(
        parse_ed25519_pubkey_hex64(&hex::encode(identity_point_bytes())),
        Err(MarketError::Validation(ref msg)) if msg.contains("weak small-order point")
    ));
    assert!(matches!(
        parse_ed25519_pubkey_hex64(&hex::encode(order_4_point_bytes())),
        Err(MarketError::Validation(ref msg)) if msg.contains("weak small-order point")
    ));

    // 3. Tampering payload_ciphertext of a slot readable by reader_vk must return Err(Crypto), NOT Ok(None)
    let mut blob = seal_slot_payload(b"tamper-me", &[reader_vk], 65_536).unwrap();
    assert_eq!(
        open_slot_payload(&blob, &reader_x_sk).unwrap().as_deref(),
        Some(b"tamper-me".as_slice())
    );
    let last_idx = blob.len() - 1;
    blob[last_idx] ^= 0xFF;
    assert!(matches!(
        open_slot_payload(&blob, &reader_x_sk),
        Err(MarketError::Crypto(ref msg)) if msg.contains("Slot payload authentication failed")
    ));
}

#[test]
fn s01_f2_prepare_post_senders_skips_non_contributory_sender() {
    let alice_sk = make_key(31);
    let alice_vk = alice_sk.verifying_key();
    let alice_did = create_user_id(&alice_vk, None).unwrap();

    let bob_sk = make_key(32);
    let bob_vk = bob_sk.verifying_key();

    let env = PostEnvelope {
        msg_id: "msg-resilience".to_string(),
        sender_did: alice_did.clone(),
        created_at: 1_700_000_000,
        reply_drops: vec![DropDescriptor {
            write_url: "https://drop.example/sync.php".to_string(),
            read_url: "https://drop.example/read.php".to_string(),
            slot_id: None,
            drop_pub: hex::encode(make_key(33).verifying_key().as_bytes()),
        }],
        container_bytes: vec![7u8; 64],
    };
    let entry = seal_post_box(&alice_sk, &bob_vk, &env).unwrap();

    let weak_vk = VerifyingKey::from_bytes(&identity_point_bytes()).unwrap();
    let senders = vec![
        ("did:key:zWeakFirst".to_string(), weak_vk),
        (alice_did.clone(), alice_vk),
    ];

    let prepared = prepare_post_senders(&bob_sk, &senders);
    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].sender_did, alice_did);
    let opened = try_open_post_box_prepared(&prepared, &entry).unwrap();
    assert_eq!(opened, Some((alice_did, env)));
}

fn make_indexed_key(idx: u32) -> SigningKey {
    let mut seed = [0u8; 32];
    seed[0..4].copy_from_slice(&idx.to_be_bytes());
    seed[31] = 0x42;
    SigningKey::from_bytes(&seed)
}

fn valid_drop(tag: &str) -> DropDescriptor {
    DropDescriptor {
        write_url: format!("https://sync{}.example/sync.php", tag),
        read_url: format!("https://sync{}.example/read.php", tag),
        slot_id: None,
        drop_pub: hex::encode(make_key(77).verifying_key().as_bytes()),
    }
}

#[test]
fn s01_f4_kdf_parallelism_bounds_and_rehearsal_validation() {
    // Invariant: Argon2id KDF enforces m_kib >= 8 * p_cost and rehearsal verifiers reject invalid question IDs or KDF bounds.
    let owner_did = create_user_id(&make_key(41).verifying_key(), None).unwrap();
    let answers = ["Garten", "Lindenstrasse", "Kroatien"];

    // 1. m_kib < 8 * p_cost must be rejected by validate_kdf_params
    let invalid_params = RecoveryKdfParams {
        m_kib: 8,
        t_cost: 1,
        p_cost: 4,
        salt: [1u8; 16],
    };
    assert!(matches!(
        validate_kdf_params(&invalid_params),
        Err(MarketError::Validation(ref msg)) if msg.contains("m_kib 8 must be at least 8 * p_cost (32)")
    ));

    // 2. Valid production-shaped p_cost=4 with m_kib=64 must succeed in BOTH derive_recovery_key AND rehearsal verifiers
    let p4_params = RecoveryKdfParams {
        m_kib: 64,
        t_cost: 1,
        p_cost: 4,
        salt: [2u8; 16],
    };
    validate_kdf_params(&p4_params).unwrap();
    let k1 = derive_recovery_key(&[1, 2, 3], &answers, &p4_params).unwrap();
    let k2 = derive_recovery_key(&[1, 2, 3], &answers, &p4_params).unwrap();
    assert_ne!(&*k1, &[0u8; 32], "derived recovery key must be non-zero");
    assert_eq!(
        &*k1, &*k2,
        "p_cost=4 key derivation must be deterministic for identical inputs"
    );

    let verifiers =
        compute_rehearsal_verifiers(&owner_did, 1, &[1, 2, 3], &answers, &p4_params).unwrap();
    let matches =
        verify_rehearsal_answers(&owner_did, 1, &[1, 2, 3], &answers, &verifiers, &p4_params)
            .unwrap();
    assert_eq!(matches, [true, true, true]);

    // 3. verify_rehearsal_answers must reject invalid question_ids or kdf_params even when answers are empty
    assert!(matches!(
        verify_rehearsal_answers(
            &owner_did,
            1,
            &[1, 1, 2],
            &["", "", ""],
            &verifiers,
            &p4_params
        ),
        Err(MarketError::Validation(ref msg)) if msg.contains("Question IDs must be pairwise distinct")
    ));
    assert!(matches!(
        verify_rehearsal_answers(
            &owner_did,
            1,
            &[1, 2, 3],
            &["", "", ""],
            &verifiers,
            &invalid_params
        ),
        Err(MarketError::Validation(ref msg)) if msg.contains("m_kib 8 must be at least 8 * p_cost (32)")
    ));
}

#[test]
fn s01_f3_guardian_vault_store_locator_monotonicity_canonical_ids_and_watermark_caps() {
    // Invariant: GuardianVaultStore enforces monotonic timestamps/epochs, canonical owner DIDs, and watermark bounds.
    let owner_sk = make_key(42);
    let owner_did = create_user_id(&owner_sk.verifying_key(), None).unwrap();
    let kdf = RecoveryKdfParams::new_fast_test();
    let answers = ["Garten", "Lindenstrasse", "Kroatien"];
    let payload = RecoveryMasterPayload::new(
        &owner_did,
        vec![0x11; 16],
        MnemonicLanguage::German,
        None,
        false,
    );
    let bundle = create_social_recovery_vault_bundle(
        &owner_sk,
        &owner_did,
        "Alice",
        1,
        2,
        3,
        &[1, 2, 3],
        &answers,
        &kdf,
        &payload,
        &[valid_drop("init")],
        1_700_000_200,
    )
    .unwrap();

    let mut store = GuardianVaultStore::new();
    store.upsert_ward(bundle[0].clone()).unwrap();

    // 1. Older updated_at at same epoch must NOT roll back newer descriptors (returns Ok(false))
    let older_update = GuardianLocatorUpdate {
        owner_did: owner_did.clone(),
        rotation_epoch: 1,
        backup_sync_descriptors: vec![valid_drop("old")],
        updated_at: 1_700_000_100,
    };
    assert!(!store.update_locator(&older_update).unwrap());
    assert_eq!(
        store.get_ward(&owner_did).unwrap().backup_sync_descriptors[0].write_url,
        "https://syncinit.example/sync.php"
    );

    // 2. Future epoch locator update (> entry.rotation_epoch) must fail without bumping highest_epoch_by_owner
    let future_update = GuardianLocatorUpdate {
        owner_did: owner_did.clone(),
        rotation_epoch: 5,
        backup_sync_descriptors: vec![valid_drop("future")],
        updated_at: 1_700_000_500,
    };
    assert!(matches!(
        store.update_locator(&future_update),
        Err(MarketError::Validation(ref msg)) if msg.contains("Future locator update epoch 5 > existing 1")
    ));
    assert!(store
        .highest_epoch_by_owner
        .get(&owner_did)
        .is_none_or(|&e| e <= 1));

    // 3. Invalid owner_did in update_locator and revoke_ward must return Err(Identity)
    assert!(matches!(
        store.update_locator(&GuardianLocatorUpdate {
            owner_did: "not-a-did".to_string(),
            rotation_epoch: 1,
            backup_sync_descriptors: vec![],
            updated_at: 1_700_000_500,
        }),
        Err(MarketError::Identity(ref msg)) if msg.contains("Invalid owner DID:")
    ));
    assert!(matches!(
        store.revoke_ward(&GuardianRevoke {
            owner_did: "not-a-did".to_string(),
            rotation_epoch: 1,
        }),
        Err(MarketError::Identity(ref msg)) if msg.contains("Invalid owner DID:")
    ));

    // 4. revoke_ward with rotation_epoch == 0 must be rejected
    assert!(matches!(
        store.revoke_ward(&GuardianRevoke {
            owner_did: owner_did.clone(),
            rotation_epoch: 0,
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("Revoke rotation_epoch must be >= 1")
    ));

    // 5. Watermark cap: 256 total watermarks allowed (1 active + 255 unknown revokes), 257th unknown rejected,
    //    while revoking the already-stored active ward still succeeds!
    for idx in 1..=(MAX_GUARDIAN_WARDS * 4 - 1) as u32 {
        let sk = make_indexed_key(idx);
        let did = create_user_id(&sk.verifying_key(), None).unwrap();
        assert!(store
            .revoke_ward(&GuardianRevoke {
                owner_did: did,
                rotation_epoch: 1,
            })
            .unwrap());
    }
    let extra_did = create_user_id(&make_indexed_key(999).verifying_key(), None).unwrap();
    assert!(matches!(
        store.revoke_ward(&GuardianRevoke {
            owner_did: extra_did,
            rotation_epoch: 1,
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("Guardian watermark capacity reached (maximum 256 owners)")
    ));
    assert!(store
        .revoke_ward(&GuardianRevoke {
            owner_did: owner_did.clone(),
            rotation_epoch: 1,
        })
        .unwrap());
}

#[test]
fn s01_f5_unseal_recovery_master_payload_rejects_bad_version_entropy_and_trailing_bytes() {
    // Invariant: unseal_recovery_master_payload enforces version == 1, 16|32 byte entropy, and rejects trailing bincode bytes.
    let owner_sk = make_key(51);
    let owner_did = create_user_id(&owner_sk.verifying_key(), None).unwrap();
    let s = [9u8; 32];

    let valid_master = RecoveryMasterPayload::new(
        &owner_did,
        vec![0x11; 16],
        MnemonicLanguage::German,
        None,
        false,
    );
    let r_valid = seal_recovery_master_payload(&valid_master, &s).unwrap();
    assert_eq!(
        unseal_recovery_master_payload(&r_valid, &s).unwrap(),
        valid_master
    );

    let mut bad_ver = valid_master.clone();
    bad_ver.version = 2;
    let r_bad_ver = seal_recovery_master_payload(&bad_ver, &s).unwrap();
    let bad_ver_err = unseal_recovery_master_payload(&r_bad_ver, &s).unwrap_err();
    assert!(
        matches!(bad_ver_err, MarketError::Validation(ref msg) if msg.contains("Unsupported RecoveryMasterPayload version: 2")),
        "expected unsupported version error, got: {:?}",
        bad_ver_err
    );

    let bad_ent = RecoveryMasterPayload::new(
        &owner_did,
        vec![0x11; 10],
        MnemonicLanguage::German,
        None,
        false,
    );
    let r_bad_ent = seal_recovery_master_payload(&bad_ent, &s).unwrap();
    let bad_ent_err = unseal_recovery_master_payload(&r_bad_ent, &s).unwrap_err();
    assert!(
        matches!(bad_ent_err, MarketError::Validation(ref msg) if msg.contains("Invalid mnemonic_entropy length 10 (expected 16 or 32)")),
        "expected bad entropy length error, got: {:?}",
        bad_ent_err
    );

    let mut plaintext_with_trailing = bincode::serialize(&valid_master).unwrap();
    plaintext_with_trailing.extend_from_slice(&[0xDE, 0xAD]);
    let cipher = XChaCha20Poly1305::new_from_slice(&s).unwrap();
    let nonce_bytes = [5u8; 24];
    let ct = cipher
        .encrypt(
            XNonce::from_slice(&nonce_bytes),
            Payload {
                msg: &plaintext_with_trailing,
                aad: b"HUMOCO-REC-MASTER-V1",
            },
        )
        .unwrap();
    let mut r_trailing = Vec::new();
    r_trailing.extend_from_slice(&nonce_bytes);
    r_trailing.extend_from_slice(&ct);
    let trailing_err = unseal_recovery_master_payload(&r_trailing, &s).unwrap_err();
    assert!(
        matches!(trailing_err, MarketError::Serialization(ref msg) if msg.contains("AEAD master unsealing failed:")),
        "expected trailing-bytes serialization error, got: {:?}",
        trailing_err
    );
}

#[test]
fn s01_f5_guardian_vault_entry_limits_and_reply_aad_binding() {
    // Invariant: guardian vault entries enforce descriptor/name/r_enc bounds and release replies bind eph_pk via AAD.
    let owner_sk = make_key(51);
    let owner_did = create_user_id(&owner_sk.verifying_key(), None).unwrap();
    let kdf = RecoveryKdfParams::new_fast_test();
    let answers = ["Garten", "Lindenstrasse", "Kroatien"];
    let valid_master = RecoveryMasterPayload::new(
        &owner_did,
        vec![0x11; 16],
        MnemonicLanguage::German,
        None,
        false,
    );
    let bundle = create_social_recovery_vault_bundle(
        &owner_sk,
        &owner_did,
        "Alice",
        1,
        2,
        3,
        &[1, 2, 3],
        &answers,
        &kdf,
        &valid_master,
        &[valid_drop("entry-none")],
        1_700_000_000,
    )
    .unwrap();
    verify_guardian_vault_entry(&bundle[0]).unwrap();

    let mut entry_too_many = bundle[0].clone();
    entry_too_many.backup_sync_descriptors = (0..9).map(|i| valid_drop(&i.to_string())).collect();
    assert!(matches!(
        verify_guardian_vault_entry(&entry_too_many),
        Err(MarketError::Validation(ref msg)) if msg.contains("sync descriptor count 9 exceeds maximum 8")
    ));

    let mut entry_http = bundle[0].clone();
    entry_http.backup_sync_descriptors = vec![DropDescriptor {
        write_url: "http://evil.example/sync.php".to_string(),
        read_url: "https://ok.example/read.php".to_string(),
        slot_id: None,
        drop_pub: hex::encode(make_key(78).verifying_key().as_bytes()),
    }];
    assert!(matches!(
        verify_guardian_vault_entry(&entry_http),
        Err(MarketError::Validation(ref msg)) if msg.contains("must be HTTPS or loopback HTTP")
    ));

    let mut entry_long_name = bundle[0].clone();
    entry_long_name.owner_display_name = "B".repeat(129);
    assert!(matches!(
        verify_guardian_vault_entry(&entry_long_name),
        Err(MarketError::Validation(ref msg)) if msg.contains("owner_display_name exceeds 128 chars")
    ));

    let mut entry_huge_r_enc = bundle[0].clone();
    entry_huge_r_enc.r_enc = vec![0u8; 20 * 1024];
    let huge_err = verify_guardian_vault_entry(&entry_huge_r_enc).unwrap_err();
    assert!(
        matches!(huge_err, MarketError::Validation(ref msg) if msg.contains("r_enc length 20480 out of bounds")),
        "expected out-of-bounds r_enc error, got: {:?}",
        huge_err
    );

    let eph_sk = x25519_dalek::StaticSecret::from([88u8; 32]);
    let pk_temp = x25519_dalek::PublicKey::from(&eph_sk).to_bytes();
    let sk_temp = eph_sk.to_bytes();
    let reply_payload = GuardianReleasePayload {
        entry: bundle[0].clone(),
        extra_sync_descriptors: vec![valid_drop("extra")],
        released_at: 1_700_000_100,
    };
    let reply_uri = seal_guardian_release_reply(&pk_temp, &reply_payload).unwrap();
    let opened = open_and_verify_guardian_release_reply(&sk_temp, &reply_uri).unwrap();
    assert_eq!(opened, reply_payload);

    let b64_part = reply_uri
        .strip_prefix("humoco://recovery-reply?v=1&data=")
        .unwrap();
    let mut pkg = URL_SAFE_NO_PAD.decode(b64_part).unwrap();
    // Flip bit 255 of eph_pk: X25519 ignores bit 255, so only AAD binding detects this mutation!
    pkg[31] ^= 0x80;
    let mutated_uri = format!(
        "humoco://recovery-reply?v=1&data={}",
        URL_SAFE_NO_PAD.encode(&pkg)
    );
    assert!(matches!(
        open_and_verify_guardian_release_reply(&sk_temp, &mutated_uri),
        Err(MarketError::Crypto(ref msg)) if msg.contains("Failed to decrypt recovery reply:")
    ));

    let bad_extra_uri = seal_guardian_release_reply(
        &pk_temp,
        &GuardianReleasePayload {
            entry: bundle[0].clone(),
            extra_sync_descriptors: vec![DropDescriptor {
                write_url: "http://evil.example/sync.php".to_string(),
                read_url: "https://ok.example/read.php".to_string(),
                slot_id: None,
                drop_pub: hex::encode(make_key(79).verifying_key().as_bytes()),
            }],
            released_at: 1_700_000_100,
        },
    )
    .unwrap();
    let bad_extra_res = open_and_verify_guardian_release_reply(&sk_temp, &bad_extra_uri);
    assert!(
        matches!(
            bad_extra_res,
            Err(MarketError::Validation(ref msg))
                if msg.contains("extra_sync_descriptors sync[0] write_url must be HTTPS or loopback HTTP")
        ),
        "expected HTTPS/loopback Validation error, got: {:?}",
        bad_extra_res
    );
}

#[test]
fn s01_f5_and_q2_stale_future_links_share_mismatch_and_kit_limits() {
    // Invariant: recovery URIs enforce freshness and size limits, and recovery rejects mismatched question_ids.
    let owner_sk = make_key(51);
    let owner_did = create_user_id(&owner_sk.verifying_key(), None).unwrap();
    let kdf = RecoveryKdfParams::new_fast_test();
    let answers = ["Garten", "Lindenstrasse", "Kroatien"];
    let valid_master = RecoveryMasterPayload::new(
        &owner_did,
        vec![0x11; 16],
        MnemonicLanguage::German,
        None,
        false,
    );

    let fresh_link = RecoveryRequestLink {
        version: 1,
        pk_temp: [7u8; 32],
        ts: 1_700_000_000 + 48 * 3600,
        name_hint: Some("Alice".to_string()),
    };
    assert!(!fresh_link.is_stale_48h(1_700_000_000));
    let future_link = RecoveryRequestLink {
        ts: 1_700_000_000 + 49 * 3600,
        ..fresh_link
    };
    assert!(future_link.is_stale_48h(1_700_000_000));

    let ok_uri = build_recovery_request_uri(&[7u8; 32], 1_700_000_000, Some("Alice"));
    assert_eq!(parse_recovery_request_uri(&ok_uri).unwrap().ts, 1_700_000_000);

    let long_name_uri =
        build_recovery_request_uri(&[7u8; 32], 1_700_000_000, Some(&"A".repeat(129)));
    assert!(matches!(
        parse_recovery_request_uri(&long_name_uri),
        Err(MarketError::Validation(ref msg)) if msg.contains("name_hint exceeds 128 characters")
    ));
    let huge_uri = format!("humoco://recovery-req?v=1&{}", "a".repeat(130 * 1024));
    assert!(matches!(
        parse_recovery_request_uri(&huge_uri),
        Err(MarketError::PayloadTooLarge(ref msg)) if msg.contains("Recovery request URI exceeds maximum")
    ));

    let shares = split_secret_32(&[33u8; 32], 2, 3).unwrap();
    let commitments = vec![
        compute_share_commitment(&owner_did, 1, &shares[0]),
        compute_share_commitment(&owner_did, 1, &shares[1]),
        compute_share_commitment(&owner_did, 1, &shares[2]),
    ];
    let r_enc = seal_recovery_master_payload(&valid_master, &[9u8; 32]).unwrap();
    let sig0 = owner_sk
        .sign(&guardian_entry_signing_bytes(
            &owner_did,
            1,
            2,
            3,
            &kdf,
            &[1, 2, 3],
            &r_enc,
            &commitments,
        ))
        .to_bytes();
    let sig1 = owner_sk
        .sign(&guardian_entry_signing_bytes(
            &owner_did,
            1,
            2,
            3,
            &kdf,
            &[1, 2, 4],
            &r_enc,
            &commitments,
        ))
        .to_bytes();
    let e0 = GuardianVaultEntry {
        owner_did: owner_did.clone(),
        owner_display_name: "Alice".to_string(),
        rotation_epoch: 1,
        threshold_m: 2,
        total_n: 3,
        share: shares[0].clone(),
        share_commitments: commitments.clone(),
        r_enc: r_enc.clone(),
        kdf_params: kdf.clone(),
        question_ids: [1, 2, 3],
        owner_sig: sig0,
        backup_sync_descriptors: vec![],
        updated_at: 1_700_000_000,
    };
    let e1_mismatched_q = GuardianVaultEntry {
        question_ids: [1, 2, 4],
        owner_sig: sig1,
        share: shares[1].clone(),
        ..e0.clone()
    };
    assert!(matches!(
        recover_master_payload_from_shares(&[e0, e1_mismatched_q], &answers),
        Err(MarketError::Validation(ref msg)) if msg.contains("Mismatched question_ids in entry 1")
    ));

    let oversized_kit = format!(
        "humoco://kit?v=1&ent={}&lang=1&pass=0&sync={}",
        "11".repeat(16),
        "A".repeat(100 * 1024)
    );
    let kit_err = parse_emergency_kit_uri(&oversized_kit).unwrap_err();
    assert!(
        matches!(kit_err, MarketError::PayloadTooLarge(ref msg) if msg.contains("Emergency kit sync field exceeds maximum 98304 bytes")),
        "expected PayloadTooLarge for oversized kit sync field, got: {:?}",
        kit_err
    );
}

#[test]
fn s01_f6_canonicalize_write_url_rejects_malformed_authority_credentials_and_normalizes_ipv6_and_dots(
) {
    // Invariant: canonicalize_write_url rejects malformed/credentialed URLs with exact Validation errors and normalizes IPv6/dots.
    assert_eq!(
        canonicalize_write_url("http://[0:0:0:0:0:0:0:1]:8080/").unwrap(),
        "http://[::1]:8080/sync.php"
    );
    assert_eq!(
        canonicalize_write_url("https://example.com/a/../sync.php").unwrap(),
        "https://example.com/sync.php"
    );

    for (bad, expected_substr) in [
        ("https://:443/sync.php", "Invalid write_url"),
        (
            "https://user:pass@example.com/sync.php",
            "write_url must not contain userinfo credentials",
        ),
        (
            "https://@example.com/sync.php",
            "write_url must not contain userinfo credentials",
        ),
        (
            "http://localhost@evil.com/sync.php",
            "write_url must not contain userinfo credentials",
        ),
        (
            "https://evil.com\\@localhost/sync.php",
            "write_url contains invalid whitespace, control, or backslash characters",
        ),
        ("https://[not-ipv6]/sync.php", "Invalid write_url"),
        ("https://example.com:0/sync.php", "Port 0 is not allowed"),
        (
            "https://example.com\n/sync.php",
            "write_url contains invalid whitespace, control, or backslash characters",
        ),
    ] {
        let err = canonicalize_write_url(bad).unwrap_err();
        assert!(
            matches!(err, MarketError::Validation(ref msg) if msg.contains(expected_substr)),
            "for {:?}, expected Validation containing {:?}, got: {:?}",
            bad,
            expected_substr,
            err
        );
    }
}

#[test]
fn s01_f7_and_q1_membership_denied_bridge_max_level_allow_cap_and_delta_prefix_utf8() {
    // Invariant: denied bridges confer zero Hop-2 access, deduplicated IDs keep max trust, allow caps at max_members, and prefix validation rejects invalid chars.
    let bridge_sk = make_key(61);
    let bridge_did = create_user_id(&bridge_sk.verifying_key(), None).unwrap();
    let hop2_sk = make_key(62);
    let hop2_did = create_user_id(&hop2_sk.verifying_key(), None).unwrap();

    let d1_sk = make_key(63);
    let d1_root = create_user_id(&d1_sk.verifying_key(), None).unwrap();
    let d1_sub = create_user_id(&d1_sk.verifying_key(), Some("shop")).unwrap();
    let d2_sk = make_key(64);
    let d2_did = create_user_id(&d2_sk.verifying_key(), None).unwrap();

    let stats_bridge = compute_drop_members_with_stats(
        &[],
        &[(bridge_did.clone(), 3, vec![(hop2_did.clone(), 3)])],
        &MemberPolicy::default(),
        &[],
        &[bridge_did],
        10,
    );
    assert!(!stats_bridge.member_dids.contains(&hop2_did));

    let stats_dedup = compute_drop_members_with_stats(
        &[(d1_root.clone(), 3), (d1_sub, 2), (d2_did.clone(), 2)],
        &[],
        &MemberPolicy::default(),
        &[],
        &[],
        1,
    );
    assert_eq!(stats_dedup.member_dids, vec![d1_root]);

    let allow_5: Vec<String> = (1..=5)
        .map(|i| create_user_id(&make_indexed_key(500 + i).verifying_key(), None).unwrap())
        .collect();
    let stats_capped =
        compute_drop_members_with_stats(&[], &[], &MemberPolicy::default(), &allow_5, &[], 3);
    assert_eq!(stats_capped.total_count, 3);
    assert_eq!(stats_capped.direct_count, 3);
    assert_eq!(stats_capped.member_dids.len(), 3);

    let utf8_prefix = "ä".repeat(40);
    let delta_utf8 = derive_delta_prefix(Some(&utf8_prefix));
    assert!(delta_utf8.len() <= 63);
    assert!(delta_utf8.ends_with("-delta"));

    let trailing_hyphen = derive_delta_prefix(Some("work-"));
    assert_eq!(trailing_hyphen, "work-delta");
    assert_eq!(
        validate_sync_prefix(Some(&trailing_hyphen)).unwrap(),
        Some("work-delta".to_string())
    );

    let signer = make_key(65);
    let ok_headers = sign_member_request_direct(
        &signer,
        "PUT",
        "card",
        Some("work"),
        1_700_000_000,
        "0123456789abcdef0123456789abcdef",
        b"body",
    )
    .unwrap();
    assert_eq!(ok_headers.prefix.as_deref(), Some("work"));

    assert!(matches!(
        sign_member_request_direct(
            &signer,
            "PUT",
            "card",
            Some("bad\nprefix"),
            1_700_000_000,
            "0123456789abcdef0123456789abcdef",
            b"body",
        ),
        Err(MarketError::Validation(ref msg)) if msg.contains("Sync prefix contains invalid characters:")
    ));
    assert!(matches!(
        sign_member_request_direct(
            &signer,
            "PUT",
            "card",
            Some("BAD"),
            1_700_000_000,
            "0123456789abcdef0123456789abcdef",
            b"body",
        ),
        Err(MarketError::Validation(ref msg)) if msg.contains("Sync prefix contains invalid characters:")
    ));
}

#[test]
fn s01_f9_post_tier_and_payload_validation() {
    // Invariant: post slots enforce tier sizes and receipt/handshake/inquiry validators reject malformed inputs with exact messages.
    let master_seed = [91u8; 32];
    let bad_entry = PostBoxEntry {
        msg_nonce: [1u8; 24],
        tag16: [2u8; 16],
        ciphertext: vec![0x55u8; 123],
    };
    assert!(matches!(
        encode_post_slot(&master_seed, std::slice::from_ref(&bad_entry), DROP_POST_MAX_BYTES),
        Err(MarketError::Validation(ref msg)) if msg.contains("Invalid post box ciphertext tier length: 123")
    ));
    assert!(matches!(
        pack_post_slot(&master_seed, std::slice::from_ref(&bad_entry), DROP_POST_MAX_BYTES),
        Err(MarketError::Validation(ref msg)) if msg.contains("Invalid post box ciphertext tier length: 123")
    ));

    let valid_receipt = PostDeliveryReceipt {
        acknowledged_msg_id: "msg-1".to_string(),
        acknowledged_tag16_hex: "aa".repeat(16),
        bundle_id: Some("b-1".to_string()),
        received_at: 1_700_000_000,
    };
    validate_post_delivery_receipt(&valid_receipt).unwrap();
    assert!(matches!(
        validate_post_delivery_receipt(&PostDeliveryReceipt {
            acknowledged_tag16_hex: "AA".repeat(16),
            ..valid_receipt.clone()
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("32-lowercase-hex")
    ));
    assert!(matches!(
        validate_post_delivery_receipt(&PostDeliveryReceipt {
            acknowledged_msg_id: "x".repeat(129),
            ..valid_receipt.clone()
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("acknowledged_msg_id must be non-empty and <= 128 bytes")
    ));
    assert!(matches!(
        validate_post_delivery_receipt(&PostDeliveryReceipt {
            bundle_id: Some("b".repeat(129)),
            ..valid_receipt
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("bundle_id must be non-empty and <= 128 bytes")
    ));

    let alice_did = create_user_id(&make_key(92).verifying_key(), None).unwrap();
    let long_name_handshake = WotHandshakeAssertion {
        sender_did: alice_did,
        sender_name: Some("N".repeat(129)),
        proposed_level: 1,
        wot_sync_json: "{\"v\":1}".to_string(),
        reply_requested: true,
        created_at: 1_700_000_000,
    };
    assert!(matches!(
        validate_wot_handshake_assertion(&long_name_handshake),
        Err(MarketError::Validation(ref msg)) if msg.contains("sender_name exceeds maximum length")
    ));

    let valid_inquiry = MarketInquiry {
        inquiry_id: "inq-1".to_string(),
        target: InquiryTarget::Ad {
            ad_id: "0123456789abcdef".to_string(),
            ad_title: "Honey".to_string(),
        },
        message: "hi".to_string(),
        created_at: 1_700_000_000,
    };
    validate_market_inquiry(&valid_inquiry).unwrap();
    assert!(matches!(
        validate_market_inquiry(&MarketInquiry {
            message: format!("{}hi", " ".repeat(5000)),
            ..valid_inquiry.clone()
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("message raw length exceeds 2048 bytes")
    ));
    assert!(matches!(
        validate_market_inquiry(&MarketInquiry {
            inquiry_id: "i".repeat(129),
            ..valid_inquiry
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("inquiry_id must be non-empty and <= 128 bytes")
    ));

    let base_msg = MarketInquiryMessage {
        inquiry_id: "inq-1".to_string(),
        msg_id: "msg-1".to_string(),
        text: Some("ok".to_string()),
        decision: None,
        payment_uri: None,
        amount_hint: None,
        standard_uuid: None,
        created_at: 1_700_000_000,
    };
    validate_market_inquiry_message(&base_msg).unwrap();
    assert!(matches!(
        validate_market_inquiry_message(&MarketInquiryMessage {
            text: Some(format!("{}ok", " ".repeat(5000))),
            ..base_msg.clone()
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("message text raw length exceeds 2048 bytes")
    ));
    assert!(matches!(
        validate_market_inquiry_message(&MarketInquiryMessage {
            msg_id: "m".repeat(129),
            ..base_msg.clone()
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("msg_id must be non-empty and <= 128 bytes")
    ));
    assert!(matches!(
        validate_market_inquiry_message(&MarketInquiryMessage {
            payment_uri: Some("humoco://pay?to=did:key:z123\r\nevil".to_string()),
            ..base_msg
        }),
        Err(MarketError::Validation(ref msg)) if msg.contains("control characters")
    ));
}

#[test]
fn s01_f9_inquiry_rate_tracker_bounds() {
    // Invariant: InquiryRateTracker rejects blank keys, enforces 64 distinct target keys, and defaults max_per_target == 0 safely.
    let mut tracker = InquiryRateTracker::default();
    assert!(matches!(
        tracker.record_inquiry("   ", "person"),
        Err(MarketError::Validation(ref msg)) if msg.contains("sender and target cannot be empty")
    ));
    assert!(matches!(
        tracker.record_inquiry(&"aa".repeat(32), "   "),
        Err(MarketError::Validation(ref msg)) if msg.contains("sender and target cannot be empty")
    ));
    for i in 0..64 {
        tracker
            .record_inquiry(&"aa".repeat(32), &format!("person:skill_{}", i))
            .unwrap();
    }
    assert_eq!(tracker.inquiries.len(), 64);
    assert!(matches!(
        tracker.record_inquiry(&"aa".repeat(32), "person:skill_64"),
        Err(MarketError::Validation(ref msg)) if msg.contains("maximum distinct targets (64)")
    ));

    let mut zero_cap_tracker = InquiryRateTracker {
        max_per_target: 0,
        inquiries: Default::default(),
    };
    zero_cap_tracker
        .record_inquiry(&"bb".repeat(32), "person")
        .unwrap();
    assert!(zero_cap_tracker.has_open_inquiry(&"bb".repeat(32), "person"));
}
