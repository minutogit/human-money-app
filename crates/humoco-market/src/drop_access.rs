use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use human_money_core::crypto::{create_user_id, get_pubkey_from_user_id};
use human_money_core::services::crypto::dh::{ed25519_pub_to_x25519, ed25519_sk_to_x25519_sk};
use human_money_core::services::crypto::keys::{
    derive_stealth_ephemeral_point, derive_stealth_secret_key,
};
use human_money_core::services::crypto::utils::sign_ed25519_scalar;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::MarketError;

pub const DROP_ROTATION_GRACE_SECS: u64 = 180 * 86_400;

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

pub type DisclosedContact = (String, u8);
pub type BridgeContact = (String, u8, Vec<DisclosedContact>);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DropMembersStats {
    pub member_dids: Vec<String>,
    pub direct_count: usize,
    pub hop2_count: usize,
    pub total_count: usize,
}

fn canonicalize_did(did: &str) -> Option<String> {
    let pk = get_pubkey_from_user_id(did.trim()).ok()?;
    create_user_id(&pk, None).ok()
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

    // Map: canonical_did -> effective_level
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
                    // Level 0 never traverses Hop 2.
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

// =========================================================================
//  PARSING & VALIDATION
// =========================================================================

pub fn parse_ed25519_pubkey_hex64(hex_str: &str) -> Result<VerifyingKey, MarketError> {
    if hex_str.len() != 64 || !hex_str.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(MarketError::Validation(format!(
            "Public key must be a 64-character lowercase hex string, got '{}'",
            hex_str
        )));
    }
    let mut bytes = [0u8; 32];
    hex::decode_to_slice(hex_str, &mut bytes)
        .map_err(|e| MarketError::Validation(format!("Invalid hex encoding: {}", e)))?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|e| MarketError::Validation(format!("Invalid Ed25519 public key bytes: {}", e)))
}

fn sha256_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

// =========================================================================
//  DETERMINISTIC ADMIN KEY & DATADIR (§7.1)
// =========================================================================

pub fn derive_drop_admin_seed(
    master_seed: &[u8; 32],
    drop_index: u32,
    generation: u32,
) -> [u8; 32] {
    let mut material = [0u8; 40];
    material[..32].copy_from_slice(master_seed);
    material[32..36].copy_from_slice(&drop_index.to_be_bytes());
    material[36..40].copy_from_slice(&generation.to_be_bytes());
    blake3::derive_key("HUMOCO_DROP_ADMIN_KEY_V1", &material)
}

pub fn derive_drop_admin_keypair(
    master_seed: &[u8; 32],
    drop_index: u32,
    generation: u32,
) -> (SigningKey, VerifyingKey, String) {
    let seed = derive_drop_admin_seed(master_seed, drop_index, generation);
    let sk = SigningKey::from_bytes(&seed);
    let vk = sk.verifying_key();
    let vk_hex = hex::encode(vk.as_bytes());
    (sk, vk, vk_hex)
}

pub fn derive_drop_data_dir(master_seed: &[u8; 32], drop_index: u32) -> String {
    let seed_0 = derive_drop_admin_seed(master_seed, drop_index, 0);
    let derived = blake3::derive_key("HUMOCO_DROP_DATADIR_V1", &seed_0);
    format!("d-{}", &hex::encode(derived)[..16])
}

// =========================================================================
//  64-HEX ED25519 STEALTH MEMBER KEYS (§7.1)
// =========================================================================

pub fn derive_member_stealth_pubkey_admin(
    admin_ed_sk: &SigningKey,
    member_ed_pub: &VerifyingKey,
) -> Result<String, MarketError> {
    let admin_x_sk = ed25519_sk_to_x25519_sk(admin_ed_sk);
    let member_x_pub = ed25519_pub_to_x25519(member_ed_pub);
    let shared = admin_x_sk.diffie_hellman(&member_x_pub);
    if !shared.was_contributory() {
        return Err(MarketError::Crypto(
            "Non-contributory Diffie-Hellman exchange".to_string(),
        ));
    }
    let stealth_seed = blake3::derive_key("HUMOCO_DROP_STEALTH_SEED_V1", shared.as_bytes());
    let point = derive_stealth_ephemeral_point(member_ed_pub, &stealth_seed)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    Ok(hex::encode(point.compress().as_bytes()))
}

pub fn derive_member_stealth_pubkey(
    member_ed_sk: &SigningKey,
    drop_pub_hex64: &str,
) -> Result<String, MarketError> {
    let drop_pub = parse_ed25519_pubkey_hex64(drop_pub_hex64)?;
    let member_x_sk = ed25519_sk_to_x25519_sk(member_ed_sk);
    let drop_x_pub = ed25519_pub_to_x25519(&drop_pub);
    let shared = member_x_sk.diffie_hellman(&drop_x_pub);
    if !shared.was_contributory() {
        return Err(MarketError::Crypto(
            "Non-contributory Diffie-Hellman exchange".to_string(),
        ));
    }
    let stealth_seed = blake3::derive_key("HUMOCO_DROP_STEALTH_SEED_V1", shared.as_bytes());
    let (_, _, p_pub) = derive_stealth_secret_key(member_ed_sk, &stealth_seed)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    Ok(hex::encode(p_pub.as_bytes()))
}

pub fn sign_member_stealth_message(
    member_ed_sk: &SigningKey,
    drop_pub_hex64: &str,
    message: &[u8],
) -> Result<(String, String), MarketError> {
    let drop_pub = parse_ed25519_pubkey_hex64(drop_pub_hex64)?;
    let member_x_sk = ed25519_sk_to_x25519_sk(member_ed_sk);
    let drop_x_pub = ed25519_pub_to_x25519(&drop_pub);
    let shared = member_x_sk.diffie_hellman(&drop_x_pub);
    if !shared.was_contributory() {
        return Err(MarketError::Crypto(
            "Non-contributory Diffie-Hellman exchange".to_string(),
        ));
    }
    let stealth_seed = blake3::derive_key("HUMOCO_DROP_STEALTH_SEED_V1", shared.as_bytes());
    let (scalar, public_point, p_pub) = derive_stealth_secret_key(member_ed_sk, &stealth_seed)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    let sig = sign_ed25519_scalar(&scalar, &public_point, message);
    Ok((hex::encode(p_pub.as_bytes()), hex::encode(sig.to_bytes())))
}

// =========================================================================
//  PUBLIC / PROVIDER DROP REGISTRATION KEYS (§7.3)
// =========================================================================

pub fn canonicalize_write_url(raw_url: &str) -> Result<String, MarketError> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err(MarketError::Validation(
            "write_url cannot be empty".to_string(),
        ));
    }
    let url_string = if !trimmed.contains("://") {
        format!("https://{}", trimmed)
    } else {
        trimmed.to_string()
    };

    let (scheme, rest) = url_string
        .split_once("://")
        .ok_or_else(|| MarketError::Validation("Missing URL scheme".to_string()))?;
    let scheme_lower = scheme.to_ascii_lowercase();

    // Strip fragment (#) and query (?)
    let before_fragment = rest.split_once('#').map(|(a, _)| a).unwrap_or(rest);
    let before_query = before_fragment
        .split_once('?')
        .map(|(a, _)| a)
        .unwrap_or(before_fragment);

    // Split authority and path
    let (authority, raw_path) = match before_query.split_once('/') {
        Some((auth, path)) => (auth, format!("/{}", path)),
        None => (before_query, "/".to_string()),
    };

    if authority.is_empty() {
        return Err(MarketError::Validation("Missing host in URL".to_string()));
    }

    // Split host and port in authority.
    let (host, port_opt) = if authority.starts_with('[') {
        if let Some(end_bracket) = authority.find(']') {
            let host_part = &authority[..=end_bracket];
            let after = &authority[end_bracket + 1..];
            if let Some(port_str) = after.strip_prefix(':') {
                (host_part, Some(port_str))
            } else if after.is_empty() {
                (host_part, None)
            } else {
                return Err(MarketError::Validation(format!(
                    "Invalid IPv6 authority: {}",
                    authority
                )));
            }
        } else {
            return Err(MarketError::Validation(format!(
                "Unclosed IPv6 bracket in {}",
                authority
            )));
        }
    } else if let Some((h, p)) = authority.split_once(':') {
        (h, Some(p))
    } else {
        (authority, None)
    };

    let host_lower = host.to_ascii_lowercase();

    let port_num: Option<u16> = if let Some(p) = port_opt {
        let p_num = p.parse::<u16>().map_err(|e| {
            MarketError::Validation(format!("Invalid port '{}': {}", p, e))
        })?;
        Some(p_num)
    } else {
        None
    };

    let is_loopback = host_lower == "localhost"
        || host_lower == "127.0.0.1"
        || host_lower == "[::1]";

    match scheme_lower.as_str() {
        "https" => {}
        "http" => {
            if !is_loopback {
                return Err(MarketError::Validation(format!(
                    "http:// is only allowed for loopback addresses (127.0.0.1, localhost, [::1]), got host '{}'",
                    host_lower
                )));
            }
        }
        other => {
            return Err(MarketError::Validation(format!(
                "Unsupported scheme '{}', only https (and loopback http) are allowed",
                other
            )));
        }
    }

    // Strip default ports: 443 for https, 80 for http
    let final_port = match (scheme_lower.as_str(), port_num) {
        ("https", Some(443)) => None,
        ("http", Some(80)) => None,
        (_, other_port) => other_port,
    };

    let canonical_authority = match final_port {
        Some(p) => format!("{}:{}", host_lower, p),
        None => host_lower,
    };

    // Normalize path
    let canonical_path = if raw_path.is_empty() || raw_path == "/" {
        "/drop.php".to_string()
    } else if raw_path.ends_with('/') {
        format!("{}drop.php", raw_path)
    } else {
        raw_path
    };

    Ok(format!(
        "{}://{}{}",
        scheme_lower, canonical_authority, canonical_path
    ))
}

pub fn derive_public_drop_keypair(
    master_seed: &[u8; 32],
    write_url: &str,
) -> Result<(SigningKey, VerifyingKey, String), MarketError> {
    let canonical = canonicalize_write_url(write_url)?;
    let mut material = Vec::with_capacity(32 + canonical.len());
    material.extend_from_slice(master_seed);
    material.extend_from_slice(canonical.as_bytes());
    let seed_reg = blake3::derive_key("HUMOCO_PUBLIC_DROP_USER_V1", &material);
    let r = SigningKey::from_bytes(&seed_reg);
    let r_pk = r.verifying_key();
    let r_hex = hex::encode(r_pk.as_bytes());
    Ok((r, r_pk, r_hex))
}

// =========================================================================
//  CANONICAL HTTP REQUEST SIGNING & VERIFICATION HELPERS
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedDropHeaders {
    pub pubkey_hex: String,
    pub timestamp: u64,
    pub nonce_hex: String,
    pub signature_hex: String,
}

pub fn format_member_request_message(
    method: &str,
    channel: &str,
    timestamp: u64,
    nonce_hex: &str,
    body: &[u8],
) -> String {
    format!(
        "HUMOCO-DROP-REQ-V1\n{}\n{}\n{}\n{}\n{}",
        method,
        channel,
        timestamp,
        nonce_hex,
        sha256_hex(body)
    )
}

pub fn format_admin_request_message(
    action: &str,
    channel: &str,
    timestamp: u64,
    nonce_hex: &str,
    body: &[u8],
) -> String {
    format!(
        "HUMOCO-DROP-ADMIN-V1\n{}\n{}\n{}\n{}\n{}",
        action,
        channel,
        timestamp,
        nonce_hex,
        sha256_hex(body)
    )
}

pub fn verify_ed25519_detached_hex(
    pubkey_hex64: &str,
    message: &[u8],
    sig_hex128: &str,
) -> Result<(), MarketError> {
    let vk = parse_ed25519_pubkey_hex64(pubkey_hex64)?;
    if sig_hex128.len() != 128
        || !sig_hex128
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(MarketError::Crypto(format!(
            "Signature must be a 128-character lowercase hex string, got '{}'",
            sig_hex128
        )));
    }
    let mut sig_bytes = [0u8; 64];
    hex::decode_to_slice(sig_hex128, &mut sig_bytes)
        .map_err(|e| MarketError::Crypto(format!("Invalid signature hex: {}", e)))?;
    let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
    vk.verify_strict(message, &sig)
        .map_err(|e| MarketError::Crypto(format!("Invalid Ed25519 signature: {}", e)))
}

pub fn sign_member_request_stealth(
    member_ed_sk: &SigningKey,
    drop_pub_hex64: &str,
    method: &str,
    channel: &str,
    timestamp: u64,
    nonce_hex: &str,
    body: &[u8],
) -> Result<SignedDropHeaders, MarketError> {
    let msg = format_member_request_message(method, channel, timestamp, nonce_hex, body);
    let (pubkey_hex, signature_hex) =
        sign_member_stealth_message(member_ed_sk, drop_pub_hex64, msg.as_bytes())?;
    Ok(SignedDropHeaders {
        pubkey_hex,
        timestamp,
        nonce_hex: nonce_hex.to_string(),
        signature_hex,
    })
}

pub fn sign_member_request_direct(
    signing_key: &SigningKey,
    method: &str,
    channel: &str,
    timestamp: u64,
    nonce_hex: &str,
    body: &[u8],
) -> SignedDropHeaders {
    let msg = format_member_request_message(method, channel, timestamp, nonce_hex, body);
    let sig = signing_key.sign(msg.as_bytes());
    SignedDropHeaders {
        pubkey_hex: hex::encode(signing_key.verifying_key().as_bytes()),
        timestamp,
        nonce_hex: nonce_hex.to_string(),
        signature_hex: hex::encode(sig.to_bytes()),
    }
}

pub fn sign_admin_request(
    admin_sk: &SigningKey,
    action: &str,
    channel: &str,
    timestamp: u64,
    nonce_hex: &str,
    body: &[u8],
) -> SignedDropHeaders {
    let msg = format_admin_request_message(action, channel, timestamp, nonce_hex, body);
    let sig = admin_sk.sign(msg.as_bytes());
    SignedDropHeaders {
        pubkey_hex: hex::encode(admin_sk.verifying_key().as_bytes()),
        timestamp,
        nonce_hex: nonce_hex.to_string(),
        signature_hex: hex::encode(sig.to_bytes()),
    }
}

// =========================================================================
//  180-DAY KEY ROTATION CERTIFICATE (§7.2)
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropRotationCertificate {
    pub old_pubkey: String,
    pub new_pubkey: String,
    pub retire_at: u64,
    pub cert_sig: String,
}

impl DropRotationCertificate {
    pub fn to_header_value(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.old_pubkey, self.new_pubkey, self.retire_at, self.cert_sig
        )
    }

    pub fn from_header_value(hdr: &str) -> Result<Self, MarketError> {
        let parts: Vec<&str> = hdr.split(':').collect();
        if parts.len() != 4 {
            return Err(MarketError::Validation(format!(
                "Invalid DropRotationCertificate header, expected 4 colon-separated parts, got {}",
                parts.len()
            )));
        }
        let old_pubkey = parts[0].to_string();
        let new_pubkey = parts[1].to_string();
        let retire_at = parts[2].parse::<u64>().map_err(|e| {
            MarketError::Validation(format!("Invalid retire_at in rotation certificate: {}", e))
        })?;
        let cert_sig = parts[3].to_string();
        Ok(Self {
            old_pubkey,
            new_pubkey,
            retire_at,
            cert_sig,
        })
    }
}

pub fn create_drop_rotation_certificate(
    old_admin_sk: &SigningKey,
    new_admin_pk: &VerifyingKey,
    retire_at: u64,
) -> DropRotationCertificate {
    let old_pubkey = hex::encode(old_admin_sk.verifying_key().as_bytes());
    let new_pubkey = hex::encode(new_admin_pk.as_bytes());
    let canonical = format!(
        "HUMOCO-DROP-ROTATE-V1\n{}\n{}\n{}",
        old_pubkey, new_pubkey, retire_at
    );
    let sig = old_admin_sk.sign(canonical.as_bytes());
    DropRotationCertificate {
        old_pubkey,
        new_pubkey,
        retire_at,
        cert_sig: hex::encode(sig.to_bytes()),
    }
}

pub fn verify_drop_rotation_certificate(
    cert: &DropRotationCertificate,
    expected_old_pubkey_hex64: &str,
    now_secs: u64,
) -> Result<(), MarketError> {
    if cert.old_pubkey != expected_old_pubkey_hex64 {
        return Err(MarketError::Validation(format!(
            "Rotation certificate old_pubkey mismatch: expected '{}', got '{}'",
            expected_old_pubkey_hex64, cert.old_pubkey
        )));
    }
    if cert.old_pubkey == cert.new_pubkey {
        return Err(MarketError::Validation(
            "Rotation certificate old_pubkey and new_pubkey must not be identical".to_string(),
        ));
    }
    if now_secs >= cert.retire_at {
        return Err(MarketError::Validation(format!(
            "Rotation certificate expired: now {} >= retire_at {}",
            now_secs, cert.retire_at
        )));
    }
    parse_ed25519_pubkey_hex64(&cert.old_pubkey)?;
    parse_ed25519_pubkey_hex64(&cert.new_pubkey)?;

    let canonical = format!(
        "HUMOCO-DROP-ROTATE-V1\n{}\n{}\n{}",
        cert.old_pubkey, cert.new_pubkey, cert.retire_at
    );
    verify_ed25519_detached_hex(&cert.old_pubkey, canonical.as_bytes(), &cert.cert_sig)
}
