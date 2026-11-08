use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{SigningKey, VerifyingKey};
use hkdf::Hkdf;
use human_money_core::crypto::{
    create_user_id, ed25519_pub_to_x25519, ed25519_sk_to_x25519_sk, get_pubkey_from_user_id,
};
use rand::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::error::MarketError;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MemberPolicy {
    pub direct_min: u8,
    pub hop2_enabled: bool,
    pub bridge_min: u8,
    pub hop2_min: u8,
}

impl Default for MemberPolicy {
    fn default() -> Self {
        Self {
            direct_min: 2,
            hop2_enabled: true,
            bridge_min: 2,
            hop2_min: 2,
        }
    }
}

pub fn generate_drop_keypair(rng: &mut (impl RngCore + CryptoRng)) -> ([u8; 32], String) {
    let secret = StaticSecret::random_from_rng(rng);
    let secret_bytes = secret.to_bytes();
    let public = X25519PublicKey::from(&secret);
    let drop_pub = bs58::encode(public.as_bytes()).into_string();
    (secret_bytes, drop_pub)
}

pub fn derive_write_token_admin(
    drop_secret: &[u8; 32],
    member_ed_pub: &VerifyingKey,
) -> [u8; 32] {
    let admin_secret = StaticSecret::from(*drop_secret);
    let drop_pub = X25519PublicKey::from(&admin_secret);
    let member_x25519_pub = ed25519_pub_to_x25519(member_ed_pub);

    let shared = admin_secret.diffie_hellman(&member_x25519_pub);

    let mut info = Vec::with_capacity(b"humoco-drop-write".len() + 32);
    info.extend_from_slice(b"humoco-drop-write");
    info.extend_from_slice(drop_pub.as_bytes());

    let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut write_token = [0u8; 32];
    hkdf.expand(&info, &mut write_token)
        .expect("32 bytes is valid length for HKDF-SHA256");
    write_token
}

pub fn derive_write_token_member(
    member_ed_sk: &SigningKey,
    drop_pub_b58: &str,
) -> Result<[u8; 32], MarketError> {
    let drop_pub_vec = bs58::decode(drop_pub_b58)
        .into_vec()
        .map_err(|e| MarketError::Validation(format!("Invalid base58 drop_pub: {}", e)))?;
    let drop_pub_arr: [u8; 32] = drop_pub_vec
        .try_into()
        .map_err(|_| MarketError::Validation("drop_pub must decode to 32 bytes".to_string()))?;
    let drop_pub = X25519PublicKey::from(drop_pub_arr);

    let member_x25519_sk = ed25519_sk_to_x25519_sk(member_ed_sk);
    let shared = member_x25519_sk.diffie_hellman(&drop_pub);

    let mut info = Vec::with_capacity(b"humoco-drop-write".len() + 32);
    info.extend_from_slice(b"humoco-drop-write");
    info.extend_from_slice(drop_pub.as_bytes());

    let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut write_token = [0u8; 32];
    hkdf.expand(&info, &mut write_token)
        .expect("32 bytes is valid length for HKDF-SHA256");
    Ok(write_token)
}

pub fn derive_access_id(write_token: &[u8; 32]) -> String {
    let hash = Sha256::digest(write_token);
    hex::encode(&hash[..16])
}

pub fn derive_admin_token(drop_secret: &[u8; 32]) -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(None, drop_secret);
    let mut admin_token = [0u8; 32];
    hkdf.expand(b"humoco-drop-admin", &mut admin_token)
        .expect("32 bytes is valid length for HKDF-SHA256");
    admin_token
}

pub fn admin_token_hash_hex(admin_token: &[u8; 32]) -> String {
    let hash = Sha256::digest(admin_token);
    hex::encode(hash)
}

fn canonicalize_did(did: &str) -> Option<String> {
    let pk = get_pubkey_from_user_id(did.trim()).ok()?;
    create_user_id(&pk, None).ok()
}

pub type DisclosedContact = (String, u8);
pub type BridgeContact = (String, u8, Vec<DisclosedContact>);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DropMembersStats {
    pub member_dids: Vec<String>,
    pub direct_count: usize,
    pub hop2_count: usize,
    pub total_count: usize,
}

pub fn compute_drop_members(
    direct: &[(String, u8)],
    bridges: &[BridgeContact],
    policy: &MemberPolicy,
    allow: &[String],
    deny: &[String],
    max_members: usize,
) -> Vec<String> {
    compute_drop_members_with_stats(direct, bridges, policy, allow, deny, max_members).member_dids
}

pub fn compute_drop_members_with_stats(
    direct: &[(String, u8)],
    bridges: &[BridgeContact],
    policy: &MemberPolicy,
    allow: &[String],
    deny: &[String],
    max_members: usize,
) -> DropMembersStats {
    let deny_set: BTreeSet<String> = deny.iter().filter_map(|d| canonicalize_did(d)).collect();

    // Map: canonical_did -> (is_direct, effective_level)
    let mut direct_members: BTreeMap<String, u8> = BTreeMap::new();
    let mut hop2_members: BTreeMap<String, u8> = BTreeMap::new();

    // 1. Direct contacts
    for (did, level) in direct {
        if let Some(can_did) = canonicalize_did(did) {
            if deny_set.contains(&can_did) {
                continue;
            }
            if *level >= policy.direct_min {
                direct_members.insert(can_did, *level);
            }
        }
    }

    // 2. Hop 2 contacts (if policy.hop2_enabled)
    if policy.hop2_enabled {
        let bridge_min_threshold = policy.bridge_min.max(1);
        let hop2_min_threshold = policy.hop2_min.max(1);

        for (friend_did, my_level_for_friend, disclosed) in bridges {
            if canonicalize_did(friend_did).is_none() {
                continue;
            }
            if *my_level_for_friend < bridge_min_threshold {
                continue;
            }

            for (contact_did, contact_level) in disclosed {
                if let Some(can_did) = canonicalize_did(contact_did) {
                    if deny_set.contains(&can_did) {
                        continue;
                    }
                    // ADR-00: Level 0 never traverses Hop 2. Clamped to >= 1.
                    if *contact_level < hop2_min_threshold {
                        continue;
                    }
                    // Direct members take precedence over Hop 2
                    if direct_members.contains_key(&can_did) {
                        continue;
                    }

                    let entry = hop2_members.entry(can_did).or_insert(*contact_level);
                    *entry = (*entry).max(*contact_level);
                }
            }
        }
    }

    // 3. Allow list (highest priority unless denied)
    let allow_set: BTreeSet<String> = allow
        .iter()
        .filter_map(|d| canonicalize_did(d))
        .filter(|d| !deny_set.contains(d))
        .collect();

    for allowed_did in &allow_set {
        // Direct with maximum priority
        direct_members.insert(allowed_did.clone(), u8::MAX);
        hop2_members.remove(allowed_did);
    }

    // 4. Enforce max_members
    // Evict Hop-2 entries with lowest level first
    // Tie-break: lowest level first, then largest canonical DID
    let total_count = direct_members.len() + hop2_members.len();
    if total_count > max_members {
        let mut hop2_vec: Vec<(String, u8)> = hop2_members.into_iter().collect();
        // Sort ascending: smallest level first, largest DID first
        hop2_vec.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)));

        let mut current_total = direct_members.len() + hop2_vec.len();
        let mut kept_hop2 = Vec::new();

        for (did, level) in hop2_vec {
            if current_total > max_members {
                current_total -= 1;
            } else {
                kept_hop2.push((did, level));
            }
        }

        hop2_members = kept_hop2.into_iter().collect();

        // If still over max_members, evict Direct entries (except those in allow_set)
        if direct_members.len() > max_members {
            let mut direct_vec: Vec<(String, u8)> = direct_members
                .into_iter()
                .filter(|(did, _)| !allow_set.contains(did))
                .collect();
            direct_vec.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)));

            let allowed_count = allow_set.len();
            let direct_budget = max_members.saturating_sub(allowed_count);
            let kept_direct_non_allow = if direct_vec.len() > direct_budget {
                let to_skip = direct_vec.len() - direct_budget;
                &direct_vec[to_skip..]
            } else {
                &direct_vec[..]
            };

            let mut final_direct = BTreeMap::new();
            for did in &allow_set {
                final_direct.insert(did.clone(), u8::MAX);
            }
            for (did, lvl) in kept_direct_non_allow {
                final_direct.insert(did.clone(), *lvl);
            }
            direct_members = final_direct;
        }
    }

    let direct_count = direct_members.len();
    let hop2_count = hop2_members.len();
    let total_count = direct_count + hop2_count;

    // Collect all canonical DIDs and sort deterministically
    let mut member_dids: Vec<String> = direct_members
        .into_keys()
        .chain(hop2_members.into_keys())
        .collect();
    member_dids.sort();
    member_dids.dedup();

    DropMembersStats {
        member_dids,
        direct_count,
        hop2_count,
        total_count,
    }
}
