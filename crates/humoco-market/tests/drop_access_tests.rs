use ed25519_dalek::SigningKey;
use human_money_core::crypto::create_user_id;
use humoco_market::{
    admin_token_hash_hex, compute_drop_members, derive_access_id, derive_admin_token,
    derive_write_token_admin, derive_write_token_member, MemberPolicy,
};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct VectorMember {
    ed25519_seed_hex: String,
    did: String,
    write_token_hex: String,
    access_id: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct DropAccessVector {
    drop_secret_hex: String,
    drop_pub_b58: String,
    admin_token_hex: String,
    admin_token_hash: String,
    members: Vec<VectorMember>,
}

#[test]
fn test_drop_access_deterministic_vectors() {
    let drop_secret_hex = "0101010101010101010101010101010101010101010101010101010101010101";
    let drop_secret: [u8; 32] = hex::decode(drop_secret_hex).unwrap().try_into().unwrap();

    let admin_static = StaticSecret::from(drop_secret);
    let drop_pub_x = X25519PublicKey::from(&admin_static);
    let drop_pub_b58 = bs58::encode(drop_pub_x.as_bytes()).into_string();

    let admin_token = derive_admin_token(&drop_secret);
    let admin_token_hex = hex::encode(admin_token);
    let admin_token_hash = admin_token_hash_hex(&admin_token);

    let member_seeds = [
        "1111111111111111111111111111111111111111111111111111111111111111",
        "2222222222222222222222222222222222222222222222222222222222222222",
    ];

    let mut members = Vec::new();
    for seed_hex in member_seeds {
        let seed: [u8; 32] = hex::decode(seed_hex).unwrap().try_into().unwrap();
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        let did = create_user_id(&vk, None).unwrap();

        let write_token_admin = derive_write_token_admin(&drop_secret, &vk);
        let write_token_member = derive_write_token_member(&sk, &drop_pub_b58).unwrap();
        assert_eq!(write_token_admin, write_token_member);

        let access_id_admin = derive_access_id(&write_token_admin);
        let access_id_member = derive_access_id(&write_token_member);
        assert_eq!(access_id_admin, access_id_member);

        members.push(VectorMember {
            ed25519_seed_hex: seed_hex.to_string(),
            did,
            write_token_hex: hex::encode(write_token_admin),
            access_id: access_id_admin,
        });
    }

    let vector = DropAccessVector {
        drop_secret_hex: drop_secret_hex.to_string(),
        drop_pub_b58,
        admin_token_hex,
        admin_token_hash,
        members,
    };

    let vector_json = serde_json::to_string_pretty(&vector).unwrap();
    let vector_path = "tests/vectors/drop_access.json";

    // Write file if missing or verify match if present
    if !std::path::Path::new(vector_path).exists() {
        std::fs::write(vector_path, &vector_json).unwrap();
    }

    let loaded_content = std::fs::read_to_string(vector_path).unwrap();
    let loaded: DropAccessVector = serde_json::from_str(&loaded_content).unwrap();
    assert_eq!(vector, loaded);
}

#[test]
fn test_distinct_drops_unlinkable_access_ids() {
    let secret1 = [1u8; 32];
    let secret2 = [2u8; 32];

    let member_seed = [3u8; 32];
    let sk = SigningKey::from_bytes(&member_seed);
    let vk = sk.verifying_key();

    let wt1 = derive_write_token_admin(&secret1, &vk);
    let wt2 = derive_write_token_admin(&secret2, &vk);
    assert_ne!(wt1, wt2);

    let id1 = derive_access_id(&wt1);
    let id2 = derive_access_id(&wt2);
    assert_ne!(id1, id2);
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
        (did1.clone(), 2),      // meets direct_min = 2
        (did2.clone(), 1),      // below direct_min = 2
        (did_deny.clone(), 3),  // direct level 3, but in deny list
    ];

    let bridges = vec![
        (
            did1.clone(),
            2, // friend level meets bridge_min = 2
            vec![
                (did3.clone(), 2),     // Hop 2 meets hop2_min = 2
                (did4.clone(), 1),     // Hop 2 below hop2_min = 2
                (did_hop0.clone(), 0), // Hop 2 level 0 (must never traverse)
            ],
        ),
    ];

    let policy = MemberPolicy {
        direct_min: 2,
        hop2_enabled: true,
        bridge_min: 2,
        hop2_min: 2,
    };

    // 1. Basic test with deny
    let members = compute_drop_members(
        &direct,
        &bridges,
        &policy,
        &[],
        std::slice::from_ref(&did_deny),
        10,
    );

    assert!(members.contains(&did1));
    assert!(!members.contains(&did2)); // below direct_min
    assert!(members.contains(&did3)); // Hop 2 level 2
    assert!(!members.contains(&did4)); // Hop 2 level 1 < 2
    assert!(!members.contains(&did_hop0)); // Hop 2 level 0
    assert!(!members.contains(&did_deny)); // denied

    // 2. Allow overrides
    let members_allow = compute_drop_members(
        &direct,
        &bridges,
        &policy,
        std::slice::from_ref(&did4),
        &[],
        10,
    );
    assert!(members_allow.contains(&did4)); // was allowed

    // 3. Deny wins over allow
    let members_deny_allow = compute_drop_members(
        &direct,
        &bridges,
        &policy,
        std::slice::from_ref(&did_deny),
        std::slice::from_ref(&did_deny),
        10,
    );
    assert!(!members_deny_allow.contains(&did_deny));

    // 4. max_members capping evicts Hop-2 first
    let members_cap = compute_drop_members(
        &[(did1.clone(), 2)],
        &bridges,
        &policy,
        &[],
        &[],
        1, // only 1 allowed: direct did1 vs hop2 did3 -> did1 stays, did3 evicted
    );
    assert_eq!(members_cap, vec![did1]);
}
