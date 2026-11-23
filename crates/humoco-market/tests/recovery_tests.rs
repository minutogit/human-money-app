use std::collections::BTreeSet;

use ed25519_dalek::SigningKey;
use human_money_core::crypto::create_user_id;
use human_money_core::services::mnemonic::MnemonicLanguage;
use humoco_market::{
    build_emergency_kit_uri, build_recovery_request_uri, code_to_mnemonic_language,
    combine_shares_32, compute_recovery_sas, compute_rehearsal_verifiers, compute_share_commitment,
    create_social_recovery_vault_bundle, derive_recovery_key, gf256_inv, gf256_mul,
    mnemonic_language_to_code, normalize_recovery_input, open_and_verify_guardian_release_reply,
    parse_emergency_kit_uri, parse_recovery_request_uri, recover_master_payload_from_shares,
    seal_guardian_release_reply, seal_recovery_master_payload, split_secret_32,
    unseal_recovery_master_payload, validate_and_canonicalize_answers, validate_kdf_params,
    verify_guardian_vault_entry, verify_rehearsal_answers, DropDescriptor, EmergencyKitData,
    GuardianLocatorUpdate, GuardianReleasePayload, GuardianRevoke, GuardianVaultAck,
    GuardianVaultStore, MarketError, RecoveryKdfParams, RecoveryMasterPayload,
    ShamirShare, KDF_M_KIB_MAX, KDF_M_KIB_MIN, KDF_M_KIB_PRODUCTION, KDF_P_COST_MAX,
    KDF_P_COST_MIN, KDF_P_COST_PRODUCTION, KDF_T_COST_MAX, KDF_T_COST_MIN, KDF_T_COST_PRODUCTION,
    MAX_GUARDIAN_WARDS, PAYLOAD_GUARDIAN_ACK_V1, PAYLOAD_GUARDIAN_ASSIGN_V1,
    PAYLOAD_GUARDIAN_LOCATOR_V1, PAYLOAD_GUARDIAN_REVOKE_V1, RECOVERY_MIN_ANSWER_LEN,
    RECOVERY_MIN_TOTAL_LEN, RECOVERY_QUESTION_COUNT,
};
use proptest::prelude::*;
use rand::rngs::OsRng;
use rand::RngCore;

fn make_signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

#[track_caller]
fn assert_validation_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    match res {
        Err(MarketError::Validation(msg)) => assert!(
            msg.contains(expected_substr),
            "expected Validation error containing {:?}, got {:?}",
            expected_substr,
            msg
        ),
        other => panic!(
            "expected MarketError::Validation containing {:?}, got {:?}",
            expected_substr, other
        ),
    }
}

#[track_caller]
fn assert_crypto_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    match res {
        Err(MarketError::Crypto(msg)) => assert!(
            msg.contains(expected_substr),
            "expected Crypto error containing {:?}, got {:?}",
            expected_substr,
            msg
        ),
        other => panic!(
            "expected MarketError::Crypto containing {:?}, got {:?}",
            expected_substr, other
        ),
    }
}

fn sample_drop_descriptor(id: &str) -> DropDescriptor {
    DropDescriptor {
        write_url: format!("https://sync{}.humoco.org/drop.php", id),
        read_url: format!("https://sync{}.humoco.org/read.php", id),
        slot_id: Some(format!("slot_{}", id)),
        drop_pub: format!("pubkey_{}", id),
    }
}

// ============================================================================
// 0. Constants & Wire Type Invariants
// ============================================================================

#[test]
fn test_recovery_wire_constants_and_types() {
    assert_eq!(PAYLOAD_GUARDIAN_ASSIGN_V1, "guardian_assign_v1");
    assert_eq!(PAYLOAD_GUARDIAN_ACK_V1, "guardian_ack_v1");
    assert_eq!(PAYLOAD_GUARDIAN_LOCATOR_V1, "guardian_locator_v1");
    assert_eq!(PAYLOAD_GUARDIAN_REVOKE_V1, "guardian_revoke_v1");

    assert_eq!(RECOVERY_QUESTION_COUNT, 12);
    assert_eq!(RECOVERY_MIN_ANSWER_LEN, 2);
    assert_eq!(RECOVERY_MIN_TOTAL_LEN, 12);
    assert_eq!(MAX_GUARDIAN_WARDS, 64);

    assert_eq!(KDF_M_KIB_MIN, 8);
    assert_eq!(KDF_M_KIB_MAX, 262_144);
    assert_eq!(KDF_T_COST_MIN, 1);
    assert_eq!(KDF_T_COST_MAX, 32);
    assert_eq!(KDF_P_COST_MIN, 1);
    assert_eq!(KDF_P_COST_MAX, 4);

    let ack = GuardianVaultAck {
        owner_did: "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
        rotation_epoch: 1,
        acked_at: 1700000000,
    };
    let json = serde_json::to_string(&ack).unwrap();
    let deser: GuardianVaultAck = serde_json::from_str(&json).unwrap();
    assert_eq!(deser, ack);

    let owner_a = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
    let owner_b = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";
    let dummy_share = ShamirShare {
        index: 1,
        y: [42u8; 32],
    };
    let commit = compute_share_commitment(owner_a, 1, &dummy_share);
    let commit2 = compute_share_commitment(owner_a, 1, &dummy_share);
    assert_eq!(commit, commit2);
    assert_ne!(commit, [0u8; 16]);
    assert_ne!(commit, compute_share_commitment(owner_b, 1, &dummy_share));
    assert_ne!(commit, compute_share_commitment(owner_a, 2, &dummy_share));
    let mut diff_idx_share = dummy_share.clone();
    diff_idx_share.index = 2;
    assert_ne!(
        commit,
        compute_share_commitment(owner_a, 1, &diff_idx_share)
    );
    let mut diff_y_share = dummy_share;
    diff_y_share.y[0] ^= 0x01;
    assert_ne!(commit, compute_share_commitment(owner_a, 1, &diff_y_share));
}

#[test]
fn test_gf256_field_invariants() {
    // Identity elements
    for a in 0..=255u8 {
        assert_eq!(gf256_mul(a, 1), a);
        assert_eq!(gf256_mul(a, 0), 0);
        assert_eq!(gf256_mul(1, a), a);
        assert_eq!(gf256_mul(0, a), 0);
    }

    // Inverses for all non-zero elements
    for a in 1..=255u8 {
        let inv = gf256_inv(a).expect("Non-zero element must have inverse");
        assert_eq!(
            gf256_mul(a, inv),
            1,
            "Multiplicative inverse failed for 0x{:02x}",
            a
        );
    }

    // Zero cannot be inverted
    assert_crypto_err(gf256_inv(0), "Cannot invert zero in GF(256)");
}

#[test]
fn test_shamir_2_of_3_and_3_of_5() {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);

    // 2-of-3 split
    let shares = split_secret_32(&secret, 2, 3).unwrap();
    assert_eq!(shares.len(), 3);
    assert_eq!(shares[0].index, 1);
    assert_eq!(shares[1].index, 2);
    assert_eq!(shares[2].index, 3);

    // Reconstruct with any pair of shares
    let rec_01 = combine_shares_32(&[shares[0].clone(), shares[1].clone()], 2).unwrap();
    assert_eq!(*rec_01, secret);

    let rec_02 = combine_shares_32(&[shares[0].clone(), shares[2].clone()], 2).unwrap();
    assert_eq!(*rec_02, secret);

    let rec_12 = combine_shares_32(&[shares[1].clone(), shares[2].clone()], 2).unwrap();
    assert_eq!(*rec_12, secret);

    // 3-of-5 split
    let shares_5 = split_secret_32(&secret, 3, 5).unwrap();
    assert_eq!(shares_5.len(), 5);

    // Any 3 shares reconstruct
    let rec_3_a = combine_shares_32(
        &[
            shares_5[0].clone(),
            shares_5[2].clone(),
            shares_5[4].clone(),
        ],
        3,
    )
    .unwrap();
    assert_eq!(*rec_3_a, secret);

    let rec_3_b = combine_shares_32(
        &[
            shares_5[1].clone(),
            shares_5[3].clone(),
            shares_5[4].clone(),
        ],
        3,
    )
    .unwrap();
    assert_eq!(*rec_3_b, secret);
}

#[test]
fn test_shamir_rejections_and_failures() {
    let secret = [42u8; 32];
    let shares = split_secret_32(&secret, 3, 5).unwrap();

    // Positive baseline: 3 valid distinct shares reconstruct secret
    assert_eq!(
        *combine_shares_32(
            &[shares[0].clone(), shares[1].clone(), shares[2].clone()],
            3
        )
        .unwrap(),
        secret
    );

    // Insufficient shares (2 when 3 required)
    assert_validation_err(
        combine_shares_32(&[shares[0].clone(), shares[1].clone()], 3),
        "Insufficient shares",
    );

    // Duplicate share index rejected
    assert_validation_err(
        combine_shares_32(
            &[shares[0].clone(), shares[0].clone(), shares[1].clone()],
            3,
        ),
        "Duplicate Shamir share index",
    );

    // Share index 0 rejected
    let mut bad_share = shares[0].clone();
    bad_share.index = 0;
    assert_validation_err(
        combine_shares_32(&[bad_share, shares[1].clone(), shares[2].clone()], 3),
        "Shamir share index 0 is invalid",
    );

    // Threshold < 2 in combine_shares_32 rejected
    assert_validation_err(
        combine_shares_32(&[shares[0].clone(), shares[1].clone()], 1),
        "Threshold m must be at least 2",
    );

    // Invalid parameters in split_secret_32
    assert_validation_err(
        split_secret_32(&secret, 1, 3),
        "Invalid Shamir threshold parameters",
    ); // m < 2
    assert_validation_err(
        split_secret_32(&secret, 4, 3),
        "Invalid Shamir threshold parameters",
    ); // m > n
    assert_validation_err(
        split_secret_32(&secret, 2, 17),
        "Invalid Shamir threshold parameters",
    ); // n > 16
}

proptest! {
    #[test]
    fn proptest_shamir_roundtrip(
        secret in proptest::array::uniform32(any::<u8>()),
        m in 2u8..=5u8,
        n in 5u8..=8u8,
    ) {
        prop_assume!(m <= n);
        let shares = split_secret_32(&secret, m, n).unwrap();
        assert_eq!(shares.len(), n as usize);

        // Pick the first m shares
        let subset = &shares[..m as usize];
        let recovered = combine_shares_32(subset, m).unwrap();
        prop_assert_eq!(*recovered, secret);
    }
}

// ============================================================================
// 2. Normalization & Validation Tests
// ============================================================================

#[test]
fn test_normalize_recovery_input_spec_vectors() {
    // Exact test vector from Plan Section 3.2
    let raw = "  Öma-Erna  bäckt  ßüße  Brötchen! ";
    let expected = "oema erna baeckt ssuesse broetchen!";
    assert_eq!(normalize_recovery_input(raw), expected);

    // macOS NFD vs. Linux NFC:
    // Decomposed 'O' + Combining Diaeresis (U+0308)
    let nfd_input = "O\u{0308}ma-Erna";
    let nfc_input = "Öma-Erna";
    assert_eq!(
        normalize_recovery_input(nfd_input),
        normalize_recovery_input(nfc_input)
    );
    assert_eq!(normalize_recovery_input(nfd_input), "oema erna");

    // All German characters
    assert_eq!(normalize_recovery_input("Ä Ö Ü ß ẞ"), "ae oe ue ss ss");

    // Hyphens, underscores, multiple whitespace
    let messy = "  first__street--middle   end_  ";
    assert_eq!(normalize_recovery_input(messy), "first street middle end");
}

#[test]
fn test_validate_and_canonicalize_answers_rules() {
    let q_ids = [1, 2, 3];
    let answers = ["Lindenstrasse", "Erna", "Schulzentrum"];
    let canonical = validate_and_canonicalize_answers(&q_ids, &answers).unwrap();
    assert_eq!(
        *canonical,
        "HUMOCO-REC-KDF-V1|q3|1:lindenstrasse|2:erna|3:schulzentrum"
    );

    // Duplicate question IDs rejected
    assert_validation_err(
        validate_and_canonicalize_answers(&[1, 1, 3], &answers),
        "Question IDs must be pairwise distinct",
    );
    assert_validation_err(
        validate_and_canonicalize_answers(&[1, 2, 2], &answers),
        "Question IDs must be pairwise distinct",
    );

    // Question IDs out of 1..=12 range
    assert_validation_err(
        validate_and_canonicalize_answers(&[0, 1, 2], &answers),
        "Invalid question ID",
    );
    assert_validation_err(
        validate_and_canonicalize_answers(&[1, 2, 13], &answers),
        "Invalid question ID",
    );

    // Individual answer too short (< 2 characters)
    let short_answer = ["a", "Erna", "Schulzentrum"];
    assert_validation_err(
        validate_and_canonicalize_answers(&q_ids, &short_answer),
        "Each answer must be at least",
    );

    // Total length too short (< 12 characters)
    let too_short_total = ["ab", "cd", "ef"]; // total 6 chars < 12
    assert_validation_err(
        validate_and_canonicalize_answers(&q_ids, &too_short_total),
        "Total answer length must be at least",
    );
}

// ============================================================================
// 3. Argon2id KDF & Parameter Bounds Tests
// ============================================================================

#[test]
fn test_kdf_params_bounds_checks() {
    let mut params = RecoveryKdfParams::new_fast_test();
    assert_eq!(validate_kdf_params(&params), Ok(()));

    // m_kib inclusive bounds & bomb checks
    params.m_kib = KDF_M_KIB_MIN;
    assert_eq!(validate_kdf_params(&params), Ok(()));
    params.m_kib = KDF_M_KIB_MAX;
    assert_eq!(validate_kdf_params(&params), Ok(()));
    params.m_kib = KDF_M_KIB_MIN - 1;
    assert_validation_err(validate_kdf_params(&params), "m_kib out of bounds");
    params.m_kib = KDF_M_KIB_MAX + 1;
    assert_validation_err(validate_kdf_params(&params), "m_kib out of bounds");
    params.m_kib = 64;

    // t_cost inclusive & out-of-range bounds
    params.t_cost = KDF_T_COST_MAX;
    assert_eq!(validate_kdf_params(&params), Ok(()));
    params.t_cost = 0;
    assert_validation_err(validate_kdf_params(&params), "t_cost out of bounds");
    params.t_cost = KDF_T_COST_MAX + 1;
    assert_validation_err(validate_kdf_params(&params), "t_cost out of bounds");
    params.t_cost = 1;

    // p_cost inclusive & out-of-range bounds
    params.p_cost = KDF_P_COST_MAX;
    assert_eq!(validate_kdf_params(&params), Ok(()));
    params.p_cost = 0;
    assert_validation_err(validate_kdf_params(&params), "p_cost out of bounds");
    params.p_cost = KDF_P_COST_MAX + 1;
    assert_validation_err(validate_kdf_params(&params), "p_cost out of bounds");

    // Argon2 invariant: exact m_kib == 8 * p_cost passes; below fails
    params.m_kib = 16;
    params.t_cost = 1;
    params.p_cost = 2;
    assert_eq!(validate_kdf_params(&params), Ok(()));
    params.m_kib = 8;
    assert_validation_err(validate_kdf_params(&params), "must be at least 8 * p_cost");
}

#[test]
fn test_derive_recovery_key_fast_and_deterministic() {
    let q_ids = [1, 3, 5];
    let answers = ["Lindenstrasse", "Gymnasium", "Mallorca"];
    let mut params = RecoveryKdfParams::new_fast_test();
    params.salt = [1u8; 16];

    let key1 = derive_recovery_key(&q_ids, &answers, &params).unwrap();
    let key2 = derive_recovery_key(&q_ids, &answers, &params).unwrap();
    assert_eq!(*key1, *key2);
    assert_ne!(*key1, [0u8; 32]);

    // Different answer produces different key
    let alt_answers = ["Lindenstrasse", "Gymnasium", "Menorca"];
    let key_alt = derive_recovery_key(&q_ids, &alt_answers, &params).unwrap();
    assert_ne!(*key1, *key_alt);
}

#[test]
fn test_derive_recovery_key_production_vector() {
    // Tests the real 64 MiB, t_cost = 12, p_cost = 1 production derivation
    let q_ids = [2, 4, 6];
    let answers = ["Oma Erna", "Bello", "Geheimpasswort"];
    let params = RecoveryKdfParams {
        m_kib: KDF_M_KIB_PRODUCTION,
        t_cost: KDF_T_COST_PRODUCTION,
        p_cost: KDF_P_COST_PRODUCTION,
        salt: [42u8; 16],
    };

    let start = std::time::Instant::now();
    let key = derive_recovery_key(&q_ids, &answers, &params).expect("Production KDF must succeed");
    let elapsed = start.elapsed();

    let fast_params = RecoveryKdfParams {
        m_kib: 64,
        t_cost: 1,
        p_cost: 1,
        salt: [42u8; 16],
    };
    let fast_key = derive_recovery_key(&q_ids, &answers, &fast_params).unwrap();

    assert_ne!(*key, [0u8; 32]);
    assert_ne!(*key, *fast_key);
    println!(
        "Production Argon2id (64 MiB, t=12, p=1) completed in {:.2?}",
        elapsed
    );
}

#[test]
fn test_extended_12_question_catalog_and_rehearsal_verifiers() {
    assert_eq!(RECOVERY_QUESTION_COUNT, 12);
    let q_ids = [7, 10, 12];
    let answers = ["Sandra", "Nokia 3210", "Blauer Pinguin Fahrrad"];
    let params = RecoveryKdfParams::new_fast_test();
    let owner_did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

    let key = derive_recovery_key(&q_ids, &answers, &params)
        .expect("Question IDs 7..=12 must be accepted");
    assert_ne!(*key, [0u8; 32]);

    // Question ID 13 must be rejected
    assert_validation_err(
        derive_recovery_key(&[7, 10, 13], &answers, &params),
        "Invalid question ID 13",
    );

    let verifiers = compute_rehearsal_verifiers(owner_did, 1, &q_ids, &answers, &params).unwrap();

    // Exact & normalized inputs match per-question
    let normalized_match = verify_rehearsal_answers(
        owner_did,
        1,
        &q_ids,
        &[" sandra ", "nokia-3210", "BLAUER  pinguin_fahrrad"],
        &verifiers,
        &params,
    )
    .unwrap();
    assert_eq!(normalized_match, [true, true, true]);

    // Single wrong answer pinpoints ONLY that question
    let partial_match = verify_rehearsal_answers(
        owner_did,
        1,
        &q_ids,
        &["Sandra", "Siemens C35", "Blauer Pinguin Fahrrad"],
        &verifiers,
        &params,
    )
    .unwrap();
    assert_eq!(partial_match, [true, false, true]);

    // Different epoch invalidates verifiers
    let wrong_epoch =
        verify_rehearsal_answers(owner_did, 2, &q_ids, &answers, &verifiers, &params).unwrap();
    assert_eq!(wrong_epoch, [false, false, false]);
}

// ============================================================================
// 4. Mnemonic Language Bijective Mapping Tests
// ============================================================================

#[test]
fn test_mnemonic_language_bijective_mapping() {
    let all_languages = [
        MnemonicLanguage::English,
        MnemonicLanguage::German,
        MnemonicLanguage::French,
        MnemonicLanguage::Spanish,
        MnemonicLanguage::Italian,
        MnemonicLanguage::Japanese,
        MnemonicLanguage::Korean,
        MnemonicLanguage::ChineseSimplified,
        MnemonicLanguage::ChineseTraditional,
        MnemonicLanguage::Portuguese,
        MnemonicLanguage::Czech,
    ];

    let mut seen_codes = BTreeSet::new();
    for lang in all_languages {
        let code = mnemonic_language_to_code(lang);
        assert!((1..=11).contains(&code));
        assert!(seen_codes.insert(code), "Duplicate code {}", code);

        let recovered = code_to_mnemonic_language(code).unwrap();
        assert_eq!(recovered, lang);
    }
    assert_eq!(seen_codes.len(), 11);

    // Invalid codes
    assert_validation_err(
        code_to_mnemonic_language(0),
        "Unknown mnemonic language code: 0",
    );
    assert_validation_err(
        code_to_mnemonic_language(12),
        "Unknown mnemonic language code: 12",
    );
    assert_validation_err(
        code_to_mnemonic_language(255),
        "Unknown mnemonic language code: 255",
    );
}

// ============================================================================
// 5. Master Payload Sealing & Unsealing Tests
// ============================================================================

#[test]
fn test_master_payload_sealing_roundtrip_and_tampering() {
    let entropy = vec![0x11; 16]; // 12-word entropy
    let payload = RecoveryMasterPayload::new(
        "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
        entropy,
        MnemonicLanguage::German,
        Some("main".to_string()),
        false,
    );

    let mut s = [0u8; 32];
    OsRng.fill_bytes(&mut s);

    let r_enc = seal_recovery_master_payload(&payload, &s).unwrap();
    assert!(r_enc.len() >= 24 + 16);

    let unsealed = unseal_recovery_master_payload(&r_enc, &s).unwrap();
    assert_eq!(unsealed, payload);

    // Wrong key fails
    let wrong_s = [99u8; 32];
    assert_crypto_err(
        unseal_recovery_master_payload(&r_enc, &wrong_s),
        "AEAD master unsealing failed",
    );

    // Tampered ciphertext fails Poly1305 AEAD
    let mut tampered = r_enc.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert_crypto_err(
        unseal_recovery_master_payload(&tampered, &s),
        "AEAD master unsealing failed",
    );
}

// ============================================================================
// 6. Share Commitments & Guardian Vault Entry Verification Tests
// ============================================================================

#[test]
fn test_guardian_vault_entry_verification_and_tamper_detection() {
    let owner_sk = make_signing_key(1);
    let owner_did = create_user_id(&owner_sk.verifying_key(), None).unwrap();

    let q_ids = [1, 2, 3];
    let answers = ["Lindenstrasse", "Erna", "Schulzentrum"];
    let kdf_params = RecoveryKdfParams::new_fast_test();
    let drops = vec![sample_drop_descriptor("1"), sample_drop_descriptor("2")];

    let master_payload = RecoveryMasterPayload::new(
        &owner_did,
        vec![0xAA; 16],
        MnemonicLanguage::English,
        None,
        false,
    );

    let entries = create_social_recovery_vault_bundle(
        &owner_sk,
        &owner_did,
        "Alice",
        1,
        2,
        3,
        &q_ids,
        &answers,
        &kdf_params,
        &master_payload,
        &drops,
        1700000000,
    )
    .unwrap();

    assert_eq!(entries.len(), 3);
    for entry in &entries {
        verify_guardian_vault_entry(entry).expect("Entry must verify cleanly");
    }

    // Tamper with share.y -> triggers share commitment mismatch
    let mut poisoned_entry = entries[0].clone();
    poisoned_entry.share.y[0] ^= 0x01;
    assert_validation_err(
        verify_guardian_vault_entry(&poisoned_entry),
        "Share commitment mismatch",
    );

    // Tamper with r_enc -> triggers owner_sig failure
    let mut tampered_r_enc = entries[0].clone();
    tampered_r_enc.r_enc[10] ^= 0x01;
    assert_crypto_err(
        verify_guardian_vault_entry(&tampered_r_enc),
        "Invalid owner signature",
    );

    // Tamper with rotation_epoch -> triggers owner_sig failure
    let mut tampered_epoch = entries[0].clone();
    tampered_epoch.rotation_epoch += 1;
    assert_crypto_err(
        verify_guardian_vault_entry(&tampered_epoch),
        "Invalid owner signature",
    );

    // Tamper with kdf_params to exceed ceiling -> triggers validation error
    let mut bomb_entry = entries[0].clone();
    bomb_entry.kdf_params.m_kib = 1_000_000;
    assert_validation_err(verify_guardian_vault_entry(&bomb_entry), "m_kib out of bounds");
}

// ============================================================================
// 7. GuardianVaultStore Capacity and Lifecycle Tests
// ============================================================================

#[test]
fn test_guardian_vault_store_capacity_and_lifecycle() {
    let mut store = GuardianVaultStore::new();
    assert!(store.is_empty());

    let q_ids = [1, 2, 3];
    let answers = ["Lindenstrasse", "Erna", "Schulzentrum"];
    let kdf_params = RecoveryKdfParams::new_fast_test();

    // Fill up to MAX_GUARDIAN_WARDS (64)
    for i in 1..=MAX_GUARDIAN_WARDS as u8 {
        let sk = make_signing_key(i);
        let did = create_user_id(&sk.verifying_key(), None).unwrap();
        let payload =
            RecoveryMasterPayload::new(&did, vec![i; 16], MnemonicLanguage::English, None, false);
        let bundle = create_social_recovery_vault_bundle(
            &sk,
            &did,
            &format!("Ward {}", i),
            1,
            2,
            3,
            &q_ids,
            &answers,
            &kdf_params,
            &payload,
            &[],
            1700000000,
        )
        .unwrap();

        store.upsert_ward(bundle[0].clone()).unwrap();
    }
    assert_eq!(store.len(), MAX_GUARDIAN_WARDS);

    // 65th distinct ward is rejected
    let sk_65 = make_signing_key(65);
    let did_65 = create_user_id(&sk_65.verifying_key(), None).unwrap();
    let payload_65 = RecoveryMasterPayload::new(
        &did_65,
        vec![65; 16],
        MnemonicLanguage::English,
        None,
        false,
    );
    let bundle_65 = create_social_recovery_vault_bundle(
        &sk_65,
        &did_65,
        "Ward 65",
        1,
        2,
        3,
        &q_ids,
        &answers,
        &kdf_params,
        &payload_65,
        &[],
        1700000000,
    )
    .unwrap();

    assert_validation_err(
        store.upsert_ward(bundle_65[0].clone()),
        "Guardian vault capacity reached",
    );
    assert_eq!(store.len(), MAX_GUARDIAN_WARDS);

    // Updating existing ward with higher rotation_epoch succeeds without exceeding capacity
    let sk_1 = make_signing_key(1);
    let did_1 = create_user_id(&sk_1.verifying_key(), None).unwrap();
    let payload_1 =
        RecoveryMasterPayload::new(&did_1, vec![1; 16], MnemonicLanguage::English, None, false);
    let bundle_1_v2 = create_social_recovery_vault_bundle(
        &sk_1,
        &did_1,
        "Ward 1 Renamed",
        2, // epoch 2
        2,
        3,
        &q_ids,
        &answers,
        &kdf_params,
        &payload_1,
        &[],
        1700000100,
    )
    .unwrap();

    store.upsert_ward(bundle_1_v2[0].clone()).unwrap();
    assert_eq!(store.len(), MAX_GUARDIAN_WARDS);
    assert_eq!(store.get_ward(&did_1).unwrap().rotation_epoch, 2);

    // Updating with stale epoch is rejected
    let bundle_1_stale = create_social_recovery_vault_bundle(
        &sk_1,
        &did_1,
        "Ward 1 Stale",
        1, // epoch 1 < 2
        2,
        3,
        &q_ids,
        &answers,
        &kdf_params,
        &payload_1,
        &[],
        1700000200,
    )
    .unwrap();
    assert_validation_err(
        store.upsert_ward(bundle_1_stale[0].clone()),
        "Stale rotation epoch",
    );

    // Locator update
    let locator_update = GuardianLocatorUpdate {
        owner_did: did_1.clone(),
        rotation_epoch: 2,
        backup_sync_descriptors: vec![sample_drop_descriptor("new")],
        updated_at: 1700000300,
    };
    assert!(store.update_locator(&locator_update).unwrap());
    assert_eq!(
        store
            .get_ward(&did_1)
            .unwrap()
            .backup_sync_descriptors
            .len(),
        1
    );

    // Revoke ward
    let revoke = GuardianRevoke {
        owner_did: did_1.clone(),
        rotation_epoch: 2,
    };
    assert!(store.revoke_ward(&revoke).unwrap());
    assert_eq!(store.len(), MAX_GUARDIAN_WARDS - 1);
    assert!(store.get_ward(&did_1).is_none());
}

#[test]
fn h3_guardian_vault_blocks_replay_and_out_of_order_assign_after_revoke() {
    let q_ids = [1, 2, 3];
    let answers = ["Lindenstrasse", "Erna", "Schulzentrum"];
    let kdf_params = RecoveryKdfParams::new_fast_test();

    // Case A: Assign ep 1 -> Revoke ep 2 -> Replay ep 1 & ep 2 rejected; ep 3 accepted.
    let mut store = GuardianVaultStore::new();
    let sk_a = make_signing_key(71);
    let did_a = create_user_id(&sk_a.verifying_key(), None).unwrap();
    let payload_a = RecoveryMasterPayload::new(
        &did_a,
        vec![0x71; 16],
        MnemonicLanguage::English,
        None,
        false,
    );

    let make_entry = |sk: &SigningKey, did: &str, payload: &RecoveryMasterPayload, epoch: u32| {
        create_social_recovery_vault_bundle(
            sk,
            did,
            "Alice",
            epoch,
            2,
            3,
            &q_ids,
            &answers,
            &kdf_params,
            payload,
            &[],
            1_700_000_000 + u64::from(epoch),
        )
        .unwrap()[0]
            .clone()
    };

    let ep1 = make_entry(&sk_a, &did_a, &payload_a, 1);
    let ep2 = make_entry(&sk_a, &did_a, &payload_a, 2);
    let ep3 = make_entry(&sk_a, &did_a, &payload_a, 3);

    store.upsert_ward(ep1.clone()).unwrap();
    // Idempotent re-upsert at active non-revoked epoch 1 remains allowed
    store.upsert_ward(ep1.clone()).unwrap();

    // Revoke at epoch 2 removes active ward and records monotonic revoke watermark
    assert!(store
        .revoke_ward(&GuardianRevoke {
            owner_did: did_a.clone(),
            rotation_epoch: 2,
        })
        .unwrap());
    assert!(store.get_ward(&did_a).is_none());

    // Replaying assign at epoch 1 or same revoked epoch 2 must fail
    assert_validation_err(
        store.upsert_ward(ep1),
        "Rejected revoked or stale rotation epoch",
    );
    assert_validation_err(
        store.upsert_ward(ep2),
        "Rejected revoked or stale rotation epoch",
    );
    assert!(store.get_ward(&did_a).is_none());

    // Stale locator update at epoch 2 after revoke is rejected
    assert_validation_err(
        store.update_locator(&GuardianLocatorUpdate {
            owner_did: did_a.clone(),
            rotation_epoch: 2,
            backup_sync_descriptors: vec![sample_drop_descriptor("stale")],
            updated_at: 1_700_000_500,
        }),
        "Rejected locator update for revoked epoch",
    );

    // Strictly newer epoch 3 succeeds
    store.upsert_ward(ep3).unwrap();
    assert_eq!(store.get_ward(&did_a).unwrap().rotation_epoch, 3);

    // Case B: Out-of-order revoke at epoch 2 arrives BEFORE assign at epoch 1
    let mut store_ooo = GuardianVaultStore::new();
    let sk_b = make_signing_key(72);
    let did_b = create_user_id(&sk_b.verifying_key(), None).unwrap();
    let payload_b = RecoveryMasterPayload::new(
        &did_b,
        vec![0x72; 16],
        MnemonicLanguage::English,
        None,
        false,
    );
    assert!(
        store_ooo
            .revoke_ward(&GuardianRevoke {
                owner_did: did_b.clone(),
                rotation_epoch: 2,
            })
            .unwrap(),
        "First out-of-order revoke must report changed watermark so handler persists it"
    );
    // Duplicate same revoke does not mutate watermarks further
    assert!(!store_ooo
        .revoke_ward(&GuardianRevoke {
            owner_did: did_b.clone(),
            rotation_epoch: 2,
        })
        .unwrap());

    assert_validation_err(
        store_ooo.upsert_ward(make_entry(&sk_b, &did_b, &payload_b, 1)),
        "Rejected revoked or stale rotation epoch",
    );
    assert_validation_err(
        store_ooo.upsert_ward(make_entry(&sk_b, &did_b, &payload_b, 2)),
        "Rejected revoked or stale rotation epoch",
    );
    store_ooo
        .upsert_ward(make_entry(&sk_b, &did_b, &payload_b, 3))
        .unwrap();
    assert_eq!(store_ooo.get_ward(&did_b).unwrap().rotation_epoch, 3);
}

// ============================================================================
// 8. Ephemeral 1-Tap Handshake & SAS Verification Tests
// ============================================================================

#[test]
fn test_recovery_sas_and_request_uri() {
    let mut pk_temp = [0u8; 32];
    OsRng.fill_bytes(&mut pk_temp);

    let sas = compute_recovery_sas(&pk_temp);
    assert_eq!(sas.len(), 7); // XXX-XXX format
    assert_eq!(&sas[3..4], "-");
    let parts: Vec<&str> = sas.split('-').collect();
    assert_eq!(parts.len(), 2);
    assert!(parts[0].parse::<u32>().is_ok());
    assert!(parts[1].parse::<u32>().is_ok());

    // Request URI building & parsing
    let uri = build_recovery_request_uri(&pk_temp, 1700000000, Some("Alice"));
    assert!(uri.starts_with("humoco://recovery-req?v=1&pk="));
    assert!(uri.contains("&ts=1700000000"));
    assert!(uri.contains("&name=Alice"));

    let parsed = parse_recovery_request_uri(&uri).unwrap();
    assert_eq!(parsed.version, 1);
    assert_eq!(parsed.pk_temp, pk_temp);
    assert_eq!(parsed.ts, 1700000000);
    assert_eq!(parsed.name_hint.as_deref(), Some("Alice"));
    assert!(!parsed.is_stale_48h(1700000000 + 3600));
    assert!(parsed.is_stale_48h(1700000000 + 49 * 3600));
}

#[test]
fn test_guardian_release_reply_sealing_and_verification() {
    let owner_sk = make_signing_key(1);
    let owner_did = create_user_id(&owner_sk.verifying_key(), None).unwrap();

    let q_ids = [1, 2, 3];
    let answers = ["Lindenstrasse", "Erna", "Schulzentrum"];
    let kdf_params = RecoveryKdfParams::new_fast_test();

    let master_payload = RecoveryMasterPayload::new(
        &owner_did,
        vec![0xEE; 16],
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
        &q_ids,
        &answers,
        &kdf_params,
        &master_payload,
        &[sample_drop_descriptor("1")],
        1700000000,
    )
    .unwrap();

    // Ephemeral session keypair on new device
    let eph_sk = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let eph_pk = x25519_dalek::PublicKey::from(&eph_sk);
    let sk_temp = eph_sk.to_bytes();
    let pk_temp = eph_pk.to_bytes();

    let reply_payload = GuardianReleasePayload {
        entry: bundle[0].clone(),
        extra_sync_descriptors: vec![sample_drop_descriptor("extra")],
        released_at: 1700000500,
    };

    let reply_uri = seal_guardian_release_reply(&pk_temp, &reply_payload).unwrap();
    assert!(reply_uri.starts_with("humoco://recovery-reply?v=1&data="));

    let uri_len = reply_uri.len();
    let raw_payload_bytes = bincode::serialize(&reply_payload).unwrap();
    println!(
        "Measured raw payload bincode len: {} bytes",
        raw_payload_bytes.len()
    );
    println!("Measured recovery-reply URI length: {} chars", uri_len);
    assert!(uri_len < 2000, "Reply URI too large: {} chars", uri_len);

    let opened = open_and_verify_guardian_release_reply(&sk_temp, &reply_uri).unwrap();
    assert_eq!(opened, reply_payload);

    // Minimal entry without extra descriptors
    let minimal_payload = GuardianReleasePayload {
        entry: bundle[1].clone(),
        extra_sync_descriptors: Vec::new(),
        released_at: 1700000500,
    };
    let min_reply_uri = seal_guardian_release_reply(&pk_temp, &minimal_payload).unwrap();
    println!(
        "Measured minimal recovery-reply URI length (0 extra descriptors): {} chars",
        min_reply_uri.len()
    );
    let min_opened = open_and_verify_guardian_release_reply(&sk_temp, &min_reply_uri).unwrap();
    assert_eq!(min_opened, minimal_payload);

    // Wrong sk_temp fails
    let wrong_sk = [99u8; 32];
    assert_crypto_err(
        open_and_verify_guardian_release_reply(&wrong_sk, &reply_uri),
        "Failed to decrypt recovery reply",
    );

    // Chat message wrapping around both request and reply URIs
    let req_uri = build_recovery_request_uri(&pk_temp, 1700000000, None);
    let chat_req = format!(
        "Hallo Bernd!\nKannst du mir helfen?\n{}\nMeine Kontrollzahl ist {}\nDanke!",
        req_uri,
        compute_recovery_sas(&pk_temp)
    );
    let parsed_req = parse_recovery_request_uri(&chat_req).unwrap();
    assert_eq!(parsed_req.pk_temp, pk_temp);
    assert_eq!(parsed_req.ts, 1700000000);

    let chat_reply = format!(
        "Klar, hier ist meine Freigabe:\n\"{}\"\nSag Bescheid, wenn es geklappt hat!",
        reply_uri
    );
    let opened_from_chat = open_and_verify_guardian_release_reply(&sk_temp, &chat_reply).unwrap();
    assert_eq!(opened_from_chat, reply_payload);
}

// ============================================================================
// 9. Full End-to-End Social Recovery Flow
// ============================================================================

#[test]
fn test_end_to_end_social_recovery_success_and_failures() {
    let alice_sk = make_signing_key(10);
    let alice_did = create_user_id(&alice_sk.verifying_key(), None).unwrap();

    let question_ids = [1, 3, 5];
    let correct_answers = ["Blumenstrasse", "Berufsschule", "Teneriffa"];
    let kdf_params = RecoveryKdfParams::new_fast_test();

    let initial_entropy = vec![0x42; 16]; // 12 BIP-39 words
    let master_payload = RecoveryMasterPayload::new(
        &alice_did,
        initial_entropy.clone(),
        MnemonicLanguage::German,
        Some("savings".to_string()),
        false,
    );

    // Alice creates 3-of-5 vault bundle
    let bundle = create_social_recovery_vault_bundle(
        &alice_sk,
        &alice_did,
        "Alice Wonder",
        1,
        3, // M = 3
        5, // N = 5
        &question_ids,
        &correct_answers,
        &kdf_params,
        &master_payload,
        &[sample_drop_descriptor("alice_backup")],
        1700000000,
    )
    .unwrap();

    assert_eq!(bundle.len(), 5);

    // Alice loses device. On new device, gets 3 replies (e.g. from guardians 1, 3, 4)
    let collected_entries = vec![bundle[0].clone(), bundle[2].clone(), bundle[3].clone()];

    // Success with correct answers
    let recovered =
        recover_master_payload_from_shares(&collected_entries, &correct_answers).unwrap();
    assert_eq!(recovered.target_did, alice_did);
    assert_eq!(recovered.mnemonic_entropy, initial_entropy);
    assert_eq!(recovered.language().unwrap(), MnemonicLanguage::German);
    assert_eq!(recovered.user_prefix.as_deref(), Some("savings"));
    assert!(!recovered.has_bip39_passphrase);

    // Failure with wrong answers: Poly1305 AEAD fails
    let wrong_answers = ["Blumenstrasse", "Berufsschule", "Mallorca"]; // 3rd wrong
    assert_crypto_err(
        recover_master_payload_from_shares(&collected_entries, &wrong_answers),
        "AEAD master unsealing failed",
    );

    // Failure with insufficient shares (2 when 3 required)
    let too_few = vec![bundle[0].clone(), bundle[1].clone()];
    assert_validation_err(
        recover_master_payload_from_shares(&too_few, &correct_answers),
        "Insufficient guardian entries",
    );

    // Failure if a guardian gave a poisoned share: caught by entry verification
    let mut poisoned = collected_entries.clone();
    poisoned[0].share.y[5] ^= 0xFF;
    assert_validation_err(
        recover_master_payload_from_shares(&poisoned, &correct_answers),
        "Share commitment mismatch",
    );
}

// ============================================================================
// 10. Emergency Kit URI Tests
// ============================================================================

#[test]
fn test_emergency_kit_uri_roundtrip_12_and_24_words() {
    let drops = vec![sample_drop_descriptor("a"), sample_drop_descriptor("b")];

    // 16-byte entropy (12 words)
    let kit_12 = EmergencyKitData {
        version: 1,
        entropy_hex: hex::encode(vec![0x12; 16]),
        language_code: 2, // German
        user_prefix: Some("sub-account".to_string()),
        has_bip39_passphrase: false,
        sync_descriptors: drops.clone(),
    };

    let uri_12 = build_emergency_kit_uri(&kit_12).unwrap();
    assert!(uri_12.starts_with("humoco://kit?v=1&ent="));
    assert!(uri_12.contains("&lang=2"));
    assert!(uri_12.contains("&pass=0"));
    assert!(uri_12.contains("&prefix=sub-account"));

    let parsed_12 = parse_emergency_kit_uri(&uri_12).unwrap();
    assert_eq!(parsed_12, kit_12);

    // HTML tag suffix & trailing chat punctuation must be stripped cleanly
    let wrapped_html = format!("<div>URI: {}</div>", uri_12);
    assert_eq!(parse_emergency_kit_uri(&wrapped_html).unwrap(), kit_12);
    let wrapped_punct = format!("({})!", uri_12);
    assert_eq!(parse_emergency_kit_uri(&wrapped_punct).unwrap(), kit_12);

    // 32-byte entropy (24 words) with passphrase
    let kit_24 = EmergencyKitData {
        version: 1,
        entropy_hex: hex::encode(vec![0x24; 32]),
        language_code: 1, // English
        user_prefix: None,
        has_bip39_passphrase: true,
        sync_descriptors: drops,
    };

    let uri_24 = build_emergency_kit_uri(&kit_24).unwrap();
    assert!(uri_24.contains("&pass=1"));
    assert!(!uri_24.contains("&prefix="));

    let parsed_24 = parse_emergency_kit_uri(&uri_24).unwrap();
    assert_eq!(parsed_24, kit_24);

    // Invalid entropy lengths rejected
    let mut bad_kit = kit_12.clone();
    bad_kit.entropy_hex = hex::encode(vec![0x00; 10]); // 10 bytes invalid
    assert_validation_err(
        build_emergency_kit_uri(&bad_kit),
        "Emergency kit entropy must be 16 or 32 bytes",
    );

    // Invalid language code rejected
    bad_kit.entropy_hex = hex::encode(vec![0x00; 16]);
    bad_kit.language_code = 15;
    assert_validation_err(
        build_emergency_kit_uri(&bad_kit),
        "Invalid language code: 15",
    );
}

#[test]
fn test_recovery_uris_tolerate_chat_punctuation_and_uppercase_scheme() {
    use rand::rngs::OsRng;

    let owner_sk = make_signing_key(77);
    let owner_did = create_user_id(&owner_sk.verifying_key(), None).unwrap();
    let kdf_params = RecoveryKdfParams::new_fast_test();
    let q_ids = [1, 2, 3];
    let answers = ["Garten", "Lindenstrasse", "Kroatien"];
    let master_payload = RecoveryMasterPayload::new(
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
        &q_ids,
        &answers,
        &kdf_params,
        &master_payload,
        &[sample_drop_descriptor("1")],
        1700000123,
    )
    .unwrap();

    let eph_sk = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let pk_temp = x25519_dalek::PublicKey::from(&eph_sk).to_bytes();
    let sk_temp = eph_sk.to_bytes();

    let req_uri = build_recovery_request_uri(&pk_temp, 1700000123, Some("Alice"));
    let upper_scheme_req = req_uri.replacen("humoco://recovery-req?", "HUMOCO://RECOVERY-REQ?", 1);
    let chat_req = format!("Hallo Max, bitte öffnen: ({}).", upper_scheme_req);
    let parsed_req = parse_recovery_request_uri(&chat_req).unwrap();
    assert_eq!(parsed_req.pk_temp, pk_temp);
    assert_eq!(parsed_req.ts, 1700000123);
    assert_eq!(parsed_req.name_hint.as_deref(), Some("Alice"));

    let reply_payload = GuardianReleasePayload {
        entry: bundle[0].clone(),
        extra_sync_descriptors: vec![],
        released_at: 1700000200,
    };
    let reply_uri = seal_guardian_release_reply(&pk_temp, &reply_payload).unwrap();
    let upper_scheme_reply =
        reply_uri.replacen("humoco://recovery-reply?", "HUMOCO://RECOVERY-REPLY?", 1);
    let chat_reply = format!("Hier deine Antwort: {}!", upper_scheme_reply);
    let opened = open_and_verify_guardian_release_reply(&sk_temp, &chat_reply).unwrap();
    assert_eq!(opened, reply_payload);

    // Wrapped reply across email lines + trailing prose
    let wrapped_reply = format!(
        "Hallo Hans,\n{}\n{}\nViele Gruesse!",
        &reply_uri[..reply_uri.len() / 2],
        &reply_uri[reply_uri.len() / 2..],
    );
    let opened_wrapped = open_and_verify_guardian_release_reply(&sk_temp, &wrapped_reply).unwrap();
    assert_eq!(opened_wrapped, reply_payload);
}

#[test]
fn test_sloppy_kit_uri_and_prefixed_vault_lookup() {
    let kit_uri = build_emergency_kit_uri(&EmergencyKitData {
        version: 1,
        entropy_hex: "11".repeat(16),
        language_code: 2,
        user_prefix: Some("laptop".to_string()),
        has_bip39_passphrase: false,
        sync_descriptors: vec![],
    })
    .unwrap();
    let noisy = format!(
        "Hast du das kit? Hier ist es: {}?",
        kit_uri.replacen("humoco://kit?", "HUMOCO://KIT?", 1)
    );
    let parsed_kit = parse_emergency_kit_uri(&noisy).unwrap();
    assert_eq!(parsed_kit.user_prefix.as_deref(), Some("laptop"));

    let req_with_q = format!(
        "Kannst du helfen: {}?",
        build_recovery_request_uri(&[5u8; 32], 1700000500, None)
    );
    let parsed_req = parse_recovery_request_uri(&req_with_q).unwrap();
    assert_eq!(parsed_req.ts, 1700000500);

    // Prefixed subaccounts on the same root key store and manage independent guardian vault entries
    let sk = make_signing_key(7);
    let shop_did = create_user_id(&sk.verifying_key(), Some("laden")).unwrap();
    let phone_did = create_user_id(&sk.verifying_key(), Some("phone")).unwrap();
    let shop_master = RecoveryMasterPayload::new(
        &shop_did,
        vec![0x22; 16],
        MnemonicLanguage::German,
        Some("laden".to_string()),
        false,
    );
    let phone_master = RecoveryMasterPayload::new(
        &phone_did,
        vec![0x33; 16],
        MnemonicLanguage::German,
        Some("phone".to_string()),
        false,
    );
    let shop_bundle = create_social_recovery_vault_bundle(
        &sk,
        &shop_did,
        "Laden",
        1,
        2,
        3,
        &[1, 2, 3],
        &["Garten", "Lindenstrasse", "Kroatien"],
        &RecoveryKdfParams::new_fast_test(),
        &shop_master,
        &[],
        1700000000,
    )
    .unwrap();
    let phone_bundle = create_social_recovery_vault_bundle(
        &sk,
        &phone_did,
        "Privat",
        1,
        2,
        3,
        &[1, 2, 3],
        &["Garten", "Lindenstrasse", "Kroatien"],
        &RecoveryKdfParams::new_fast_test(),
        &phone_master,
        &[],
        1700000010,
    )
    .unwrap();
    let mut store = GuardianVaultStore::new();
    store.upsert_ward(shop_bundle[0].clone()).unwrap();
    store.upsert_ward(phone_bundle[0].clone()).unwrap();
    assert_eq!(store.len(), 2);
    assert_eq!(
        store.get_ward(&shop_did).unwrap().owner_display_name,
        "Laden"
    );
    assert_eq!(
        store.get_ward(&phone_did).unwrap().owner_display_name,
        "Privat"
    );
    assert!(store
        .update_locator(&GuardianLocatorUpdate {
            owner_did: phone_did.clone(),
            rotation_epoch: 1,
            backup_sync_descriptors: vec![sample_drop_descriptor("1")],
            updated_at: 1700000050,
        })
        .unwrap());
    assert!(store
        .get_ward(&shop_did)
        .unwrap()
        .backup_sync_descriptors
        .is_empty());
    assert_eq!(
        store
            .get_ward(&phone_did)
            .unwrap()
            .backup_sync_descriptors
            .len(),
        1
    );
    assert!(store
        .revoke_ward(&GuardianRevoke {
            owner_did: phone_did.clone(),
            rotation_epoch: 1,
        })
        .unwrap());
    assert_eq!(store.len(), 1);
    assert_eq!(
        store.get_ward(&shop_did).unwrap().owner_display_name,
        "Laden"
    );
    assert!(store.get_ward(&phone_did).is_none());

    // F6 Zero-Compat: short recovery URI without `humoco://` scheme must be rejected.
    let canonical = build_recovery_request_uri(&[7u8; 32], 1700000500, Some("Alice"));
    let legacy_short = canonical.strip_prefix("humoco://").unwrap();
    assert_validation_err(
        parse_recovery_request_uri(legacy_short),
        "missing 'recovery-req?'",
    );
}

#[test]
fn test_f2_guardian_vault_store_rejects_missing_epoch_watermarks() {
    let missing_watermarks = serde_json::json!({ "wards": {} });
    let err = serde_json::from_value::<GuardianVaultStore>(missing_watermarks).unwrap_err();
    assert!(
        err.to_string().contains("highest_epoch_by_owner")
            || err.to_string().contains("revoked_epoch_by_owner")
    );

    let missing_revoked = serde_json::json!({
        "wards": {},
        "highest_epoch_by_owner": {}
    });
    let err_revoked = serde_json::from_value::<GuardianVaultStore>(missing_revoked).unwrap_err();
    assert!(
        err_revoked.to_string().contains("revoked_epoch_by_owner"),
        "expected missing field revoked_epoch_by_owner, got {}",
        err_revoked
    );

    let store = GuardianVaultStore::new();
    let encoded = serde_json::to_string(&store).unwrap();
    let decoded: GuardianVaultStore = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, store);
}
