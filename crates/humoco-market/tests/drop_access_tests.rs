use ed25519_dalek::SigningKey;
use human_money_core::crypto::create_user_id;
use humoco_market::{
    canonicalize_write_url, create_drop_rotation_certificate, derive_drop_admin_keypair,
    derive_drop_admin_seed, derive_drop_data_dir, derive_member_stealth_pubkey,
    derive_member_stealth_pubkey_admin, derive_public_drop_keypair, format_admin_request_message,
    format_member_request_message, sign_admin_request, sign_member_request_direct,
    sign_member_request_stealth, sign_member_stealth_message, verify_drop_rotation_certificate,
    verify_ed25519_detached_hex, DropRotationCertificate, MemberPolicy, DROP_ROTATION_GRACE_SECS,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct AdminKeyVector {
    generation: u32,
    seed_hex: String,
    admin_pubkey_hex: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct StealthMemberVector {
    member_seed_hex: String,
    member_did: String,
    stealth_pubkey_hex: String,
    test_message: String,
    stealth_signature_hex: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct PublicDropVector {
    input_url: String,
    canonical_url: String,
    public_user_pubkey_hex: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct DropAccessVectors {
    master_seed_hex: String,
    drop_index: u32,
    generation_0: AdminKeyVector,
    generation_1: AdminKeyVector,
    data_dir: String,
    members: Vec<StealthMemberVector>,
    public_drops: Vec<PublicDropVector>,
    rotation_certificate: DropRotationCertificate,
    sample_member_request_message: String,
    sample_admin_request_message: String,
}

#[test]
fn test_drop_access_deterministic_vectors() {
    let master_seed = [1u8; 32];
    let master_seed_hex = hex::encode(master_seed);
    let drop_index = 0u32;

    // Generation 0 & 1 admin keypairs
    let (admin_sk_0, _admin_vk_0, admin_pub_0) = derive_drop_admin_keypair(&master_seed, drop_index, 0);
    let seed_0 = derive_drop_admin_seed(&master_seed, drop_index, 0);
    let gen_0 = AdminKeyVector {
        generation: 0,
        seed_hex: hex::encode(seed_0),
        admin_pubkey_hex: admin_pub_0.clone(),
    };

    let (_admin_sk_1, admin_vk_1, admin_pub_1) = derive_drop_admin_keypair(&master_seed, drop_index, 1);
    let seed_1 = derive_drop_admin_seed(&master_seed, drop_index, 1);
    let gen_1 = AdminKeyVector {
        generation: 1,
        seed_hex: hex::encode(seed_1),
        admin_pubkey_hex: admin_pub_1.clone(),
    };

    let data_dir = derive_drop_data_dir(&master_seed, drop_index);
    assert!(data_dir.starts_with("d-"));
    assert_eq!(data_dir.len(), 18); // "d-" + 16 hex chars

    // Member stealth key derivation
    let member_seeds = [
        [17u8; 32],
        [34u8; 32],
    ];

    let mut members = Vec::new();
    let test_message = b"Humoco Stealth Protocol Test Message V1";

    for m_seed in member_seeds {
        let member_sk = SigningKey::from_bytes(&m_seed);
        let member_vk = member_sk.verifying_key();
        let member_did = create_user_id(&member_vk, None).unwrap();

        let stealth_pk_admin = derive_member_stealth_pubkey_admin(&admin_sk_0, &member_vk).unwrap();
        let stealth_pk_member = derive_member_stealth_pubkey(&member_sk, &admin_pub_0).unwrap();
        assert_eq!(stealth_pk_admin, stealth_pk_member);

        let (p_pub_hex, sig_hex) =
            sign_member_stealth_message(&member_sk, &admin_pub_0, test_message).unwrap();
        assert_eq!(p_pub_hex, stealth_pk_admin);

        // Verify with strict detached Ed25519
        verify_ed25519_detached_hex(&stealth_pk_admin, test_message, &sig_hex).unwrap();

        members.push(StealthMemberVector {
            member_seed_hex: hex::encode(m_seed),
            member_did,
            stealth_pubkey_hex: stealth_pk_admin,
            test_message: String::from_utf8_lossy(test_message).to_string(),
            stealth_signature_hex: sig_hex,
        });
    }

    // Public drop registration key derivation
    let public_drop_inputs = [
        "https://drop.humoco.org/drop.php",
        "drop.verein.de/",
        "http://127.0.0.1:8080/humoco/",
    ];

    let mut public_drops = Vec::new();
    for input_url in public_drop_inputs {
        let canonical_url = canonicalize_write_url(input_url).unwrap();
        let (_r, _r_pk, r_hex) = derive_public_drop_keypair(&master_seed, input_url).unwrap();
        public_drops.push(PublicDropVector {
            input_url: input_url.to_string(),
            canonical_url,
            public_user_pubkey_hex: r_hex,
        });
    }

    // Rotation certificate
    let retire_at = 1800000000u64;
    let rotation_certificate = create_drop_rotation_certificate(&admin_sk_0, &admin_vk_1, retire_at);
    verify_drop_rotation_certificate(&rotation_certificate, &admin_pub_0, 1700000000u64).unwrap();

    // Canonical request messages
    let sample_body = b"{\"hello\":\"world\"}";
    let sample_member_request_message = format_member_request_message(
        "PUT",
        "card",
        1700000000,
        "0123456789abcdef0123456789abcdef",
        sample_body,
    );
    let sample_admin_request_message = format_admin_request_message(
        "PUT_MEMBERS",
        "card",
        1700000000,
        "0123456789abcdef0123456789abcdef",
        sample_body,
    );

    let vectors = DropAccessVectors {
        master_seed_hex,
        drop_index,
        generation_0: gen_0,
        generation_1: gen_1,
        data_dir,
        members,
        public_drops,
        rotation_certificate,
        sample_member_request_message,
        sample_admin_request_message,
    };

    let vector_path = "tests/vectors/drop_access.json";
    if !std::path::Path::new(vector_path).exists() {
        let json = serde_json::to_string_pretty(&vectors).unwrap();
        std::fs::write(vector_path, json).unwrap();
    }

    let loaded_content = std::fs::read_to_string(vector_path).unwrap();
    let loaded: DropAccessVectors = serde_json::from_str(&loaded_content).unwrap();
    assert_eq!(vectors, loaded);
}

#[test]
fn test_canonicalize_write_url_edge_cases() {
    // Normal https
    assert_eq!(
        canonicalize_write_url("https://example.com/drop.php").unwrap(),
        "https://example.com/drop.php"
    );
    // Bare domain prepends https:// and /drop.php
    assert_eq!(
        canonicalize_write_url("example.com").unwrap(),
        "https://example.com/drop.php"
    );
    // Trailing slash appends drop.php
    assert_eq!(
        canonicalize_write_url("https://example.com/my-drop/").unwrap(),
        "https://example.com/my-drop/drop.php"
    );
    // Port 443 stripped on https
    assert_eq!(
        canonicalize_write_url("https://EXAMPLE.COM:443/drop.php?query=1#frag").unwrap(),
        "https://example.com/drop.php"
    );
    // Custom https port preserved
    assert_eq!(
        canonicalize_write_url("https://example.com:8443/drop.php").unwrap(),
        "https://example.com:8443/drop.php"
    );
    // Loopback IPv4 http
    assert_eq!(
        canonicalize_write_url("http://127.0.0.1:8080/drop.php").unwrap(),
        "http://127.0.0.1:8080/drop.php"
    );
    // Loopback localhost port 80 stripped
    assert_eq!(
        canonicalize_write_url("http://localhost:80/").unwrap(),
        "http://localhost/drop.php"
    );
    // Loopback IPv6 http
    assert_eq!(
        canonicalize_write_url("http://[::1]:9000/").unwrap(),
        "http://[::1]:9000/drop.php"
    );

    // Non-loopback http rejected
    assert!(canonicalize_write_url("http://example.com/drop.php").is_err());
    assert!(canonicalize_write_url("http://192.168.1.1/drop.php").is_err());

    // Unsupported scheme rejected
    assert!(canonicalize_write_url("ftp://example.com/drop.php").is_err());

    // Empty URL rejected
    assert!(canonicalize_write_url("   ").is_err());
}

#[test]
fn test_rotation_certificate_header_roundtrip_and_verification() {
    let old_sk = SigningKey::from_bytes(&[55u8; 32]);
    let new_sk = SigningKey::from_bytes(&[66u8; 32]);
    let old_pk_hex = hex::encode(old_sk.verifying_key().as_bytes());

    let now = 1000u64;
    let retire_at = now + DROP_ROTATION_GRACE_SECS;
    let cert = create_drop_rotation_certificate(&old_sk, &new_sk.verifying_key(), retire_at);

    // Verify valid cert
    assert!(verify_drop_rotation_certificate(&cert, &old_pk_hex, now).is_ok());

    // Header serialization & deserialization roundtrip
    let header_val = cert.to_header_value();
    let parsed_cert = DropRotationCertificate::from_header_value(&header_val).unwrap();
    assert_eq!(cert, parsed_cert);

    // Expired cert fails
    assert!(verify_drop_rotation_certificate(&cert, &old_pk_hex, retire_at).is_err());
    assert!(verify_drop_rotation_certificate(&cert, &old_pk_hex, retire_at + 1).is_err());

    // Wrong expected old pubkey fails
    assert!(verify_drop_rotation_certificate(&cert, "0000000000000000000000000000000000000000000000000000000000000000", now).is_err());

    // Tampered signature fails
    let mut tampered = cert.clone();
    tampered.cert_sig = hex::encode([0u8; 64]);
    assert!(verify_drop_rotation_certificate(&tampered, &old_pk_hex, now).is_err());

    // Identical old and new fails
    let self_cert = create_drop_rotation_certificate(&old_sk, &old_sk.verifying_key(), retire_at);
    assert!(verify_drop_rotation_certificate(&self_cert, &old_pk_hex, now).is_err());
}

#[test]
fn test_http_request_signers() {
    let master_seed = [42u8; 32];
    let (admin_sk, _admin_vk, admin_pub) = derive_drop_admin_keypair(&master_seed, 0, 0);

    let member_sk = SigningKey::from_bytes(&[99u8; 32]);
    let timestamp = 1700000000u64;
    let nonce = "11223344556677881122334455667788";
    let body = b"test payload";

    // Stealth member signer
    let stealth_headers = sign_member_request_stealth(
        &member_sk,
        &admin_pub,
        "PUT",
        "card",
        timestamp,
        nonce,
        body,
    )
    .unwrap();
    assert_eq!(stealth_headers.timestamp, timestamp);
    assert_eq!(stealth_headers.nonce_hex, nonce);

    let expected_msg = format_member_request_message("PUT", "card", timestamp, nonce, body);
    assert!(verify_ed25519_detached_hex(
        &stealth_headers.pubkey_hex,
        expected_msg.as_bytes(),
        &stealth_headers.signature_hex
    )
    .is_ok());

    // Direct member signer (for public drop user R)
    let direct_headers = sign_member_request_direct(&member_sk, "PUT", "card", timestamp, nonce, body);
    assert_eq!(direct_headers.pubkey_hex, hex::encode(member_sk.verifying_key().as_bytes()));
    assert!(verify_ed25519_detached_hex(
        &direct_headers.pubkey_hex,
        expected_msg.as_bytes(),
        &direct_headers.signature_hex
    )
    .is_ok());

    // Admin request signer
    let admin_headers = sign_admin_request(&admin_sk, "PUT_MEMBERS", "card", timestamp, nonce, body);
    assert_eq!(admin_headers.pubkey_hex, admin_pub);
    let admin_msg = format_admin_request_message("PUT_MEMBERS", "card", timestamp, nonce, body);
    assert!(verify_ed25519_detached_hex(
        &admin_headers.pubkey_hex,
        admin_msg.as_bytes(),
        &admin_headers.signature_hex
    )
    .is_ok());
}

#[test]
fn test_compute_drop_members_rules() {
    let sk1 = SigningKey::from_bytes(&[10u8; 32]);
    let did1 = create_user_id(&sk1.verifying_key(), None).unwrap();

    let sk2 = SigningKey::from_bytes(&[20u8; 32]);
    let did2 = create_user_id(&sk2.verifying_key(), None).unwrap();

    let sk3 = SigningKey::from_bytes(&[30u8; 32]);
    let did3 = create_user_id(&sk3.verifying_key(), None).unwrap();

    let sk4 = SigningKey::from_bytes(&[40u8; 32]);
    let did4 = create_user_id(&sk4.verifying_key(), None).unwrap();

    let sk_deny = SigningKey::from_bytes(&[50u8; 32]);
    let did_deny = create_user_id(&sk_deny.verifying_key(), None).unwrap();

    let sk_hop0 = SigningKey::from_bytes(&[60u8; 32]);
    let did_hop0 = create_user_id(&sk_hop0.verifying_key(), None).unwrap();

    let direct = vec![
        (did1.clone(), 2),     // meets direct_min = 2
        (did2.clone(), 1),     // below direct_min = 2
        (did_deny.clone(), 3), // direct level 3, but in deny list
    ];

    let bridges = vec![(
        did1.clone(),
        2, // friend level meets bridge_min = 2
        vec![
            (did3.clone(), 2),     // Hop 2 meets hop2_min = 2
            (did4.clone(), 1),     // Hop 2 below hop2_min = 2
            (did_hop0.clone(), 0), // Hop 2 level 0 (must never traverse)
        ],
    )];

    let policy = MemberPolicy {
        direct_min: 2,
        hop2_enabled: true,
        bridge_min: 2,
        hop2_min: 2,
    };

    let allow = vec![did2.clone()];
    let deny = vec![did_deny.clone()];

    let stats = humoco_market::compute_drop_members_with_stats(
        &direct, &bridges, &policy, &allow, &deny, 300,
    );

    // did1 (direct level 2), did2 (allow list), did3 (hop 2 level 2)
    assert!(stats.member_dids.contains(&did1));
    assert!(stats.member_dids.contains(&did2));
    assert!(stats.member_dids.contains(&did3));
    assert!(!stats.member_dids.contains(&did4));
    assert!(!stats.member_dids.contains(&did_deny));
    assert!(!stats.member_dids.contains(&did_hop0));
}

// =========================================================================
//  PROPTEST INVARIANTS
// =========================================================================

proptest! {
    #[test]
    fn proptest_stealth_admin_member_equality(
        admin_seed in prop::array::uniform32(1u8..=255u8),
        member_seed in prop::array::uniform32(1u8..=255u8),
        msg in prop::collection::vec(any::<u8>(), 0..256),
    ) {
        let admin_sk = SigningKey::from_bytes(&admin_seed);
        let admin_vk = admin_sk.verifying_key();
        let admin_pk_hex = hex::encode(admin_vk.as_bytes());

        let member_sk = SigningKey::from_bytes(&member_seed);
        let member_vk = member_sk.verifying_key();

        let stealth_admin = derive_member_stealth_pubkey_admin(&admin_sk, &member_vk).unwrap();
        let stealth_member = derive_member_stealth_pubkey(&member_sk, &admin_pk_hex).unwrap();
        prop_assert_eq!(&stealth_admin, &stealth_member);

        let (p_hex, sig_hex) = sign_member_stealth_message(&member_sk, &admin_pk_hex, &msg).unwrap();
        prop_assert_eq!(&p_hex, &stealth_admin);
        prop_assert!(verify_ed25519_detached_hex(&p_hex, &msg, &sig_hex).is_ok());
    }

    #[test]
    fn proptest_unlinkability_across_drops(
        admin_seed_1 in prop::array::uniform32(1u8..=127u8),
        admin_seed_2 in prop::array::uniform32(128u8..=255u8),
        member_seed in prop::array::uniform32(1u8..=255u8),
    ) {
        let admin_sk_1 = SigningKey::from_bytes(&admin_seed_1);
        let admin_sk_2 = SigningKey::from_bytes(&admin_seed_2);

        let member_sk = SigningKey::from_bytes(&member_seed);
        let member_vk = member_sk.verifying_key();

        let stealth_1 = derive_member_stealth_pubkey_admin(&admin_sk_1, &member_vk).unwrap();
        let stealth_2 = derive_member_stealth_pubkey_admin(&admin_sk_2, &member_vk).unwrap();

        prop_assert_ne!(stealth_1, stealth_2);
    }

    #[test]
    fn proptest_public_drop_isolation(
        master_seed in prop::array::uniform32(1u8..=255u8),
        host_suffix_1 in "[a-z0-9]{3,8}",
        host_suffix_2 in "[a-z0-9]{3,8}",
    ) {
        prop_assume!(host_suffix_1 != host_suffix_2);
        let url_1 = format!("https://drop-{}.org/drop.php", host_suffix_1);
        let url_2 = format!("https://drop-{}.org/drop.php", host_suffix_2);

        let (_r1, _r1_pk, r1_hex) = derive_public_drop_keypair(&master_seed, &url_1).unwrap();
        let (_r2, _r2_pk, r2_hex) = derive_public_drop_keypair(&master_seed, &url_2).unwrap();

        prop_assert_ne!(r1_hex, r2_hex);
    }

    #[test]
    fn proptest_rotation_certificate_mitm_rejection(
        old_seed in prop::array::uniform32(1u8..=127u8),
        new_seed in prop::array::uniform32(128u8..=255u8),
        attacker_seed in prop::array::uniform32(1u8..=255u8),
        now in 1000u64..10000u64,
        delta in 1u64..10000u64,
    ) {
        let old_sk = SigningKey::from_bytes(&old_seed);
        let new_sk = SigningKey::from_bytes(&new_seed);
        let attacker_sk = SigningKey::from_bytes(&attacker_seed);

        let retire_at = now + delta;
        let valid_cert = create_drop_rotation_certificate(&old_sk, &new_sk.verifying_key(), retire_at);

        let old_pk_hex = hex::encode(old_sk.verifying_key().as_bytes());
        prop_assert!(verify_drop_rotation_certificate(&valid_cert, &old_pk_hex, now).is_ok());

        // Forged with attacker key must be rejected
        let forged_cert = create_drop_rotation_certificate(&attacker_sk, &new_sk.verifying_key(), retire_at);
        prop_assert!(verify_drop_rotation_certificate(&forged_cert, &old_pk_hex, now).is_err());
    }
}
