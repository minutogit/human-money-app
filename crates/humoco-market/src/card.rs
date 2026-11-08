use ed25519_dalek::{Signature, SigningKey};
use human_money_core::crypto::{
    create_user_id, decode_base64, encode_base64, get_pubkey_from_user_id, sign_ed25519,
    verify_ed25519,
};
use serde::Deserialize;

use crate::error::MarketError;
use crate::model::{
    CardEnvelope, ContactCard, DropDescriptor, Listing, StoredCard, MAX_CARD_BYTES,
    MAX_CARD_DROPS, MAX_LISTINGS_PER_CARD, MAX_LISTING_TEXT_CHARS, MAX_LISTING_TITLE_CHARS,
    MAX_PRICING_CHARS,
};

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct CardJwsHeader {
    alg: String,
    typ: String,
}

pub fn next_card_version(last_v: Option<u64>, now_unix: u64) -> u64 {
    last_v.map_or(now_unix, |lv| lv.saturating_add(1).max(now_unix))
}

fn is_loopback_http(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://127.0.0.1")
        || lower.starts_with("http://localhost")
        || lower.starts_with("http://[::1]")
}

fn validate_card_fields(
    max_hops: u8,
    listings: &[Listing],
    drops: &[DropDescriptor],
) -> Result<(), MarketError> {
    if !(1..=3).contains(&max_hops) {
        return Err(MarketError::Validation(format!(
            "max_hops must be between 1 and 3, got {}",
            max_hops
        )));
    }

    if listings.len() > MAX_LISTINGS_PER_CARD {
        return Err(MarketError::Validation(format!(
            "listings count {} exceeds maximum {}",
            listings.len(),
            MAX_LISTINGS_PER_CARD
        )));
    }

    for (i, listing) in listings.iter().enumerate() {
        let trimmed_title = listing.title.trim();
        if trimmed_title.is_empty() {
            return Err(MarketError::Validation(format!(
                "listing[{}] title cannot be empty",
                i
            )));
        }
        let title_chars = trimmed_title.chars().count();
        if title_chars > MAX_LISTING_TITLE_CHARS {
            return Err(MarketError::Validation(format!(
                "listing[{}] title length {} exceeds maximum {}",
                i, title_chars, MAX_LISTING_TITLE_CHARS
            )));
        }

        if let Some(ref text) = listing.text {
            let text_chars = text.chars().count();
            if text_chars > MAX_LISTING_TEXT_CHARS {
                return Err(MarketError::Validation(format!(
                    "listing[{}] text length {} exceeds maximum {}",
                    i, text_chars, MAX_LISTING_TEXT_CHARS
                )));
            }
        }

        if let Some(ref pricing) = listing.pricing {
            let pricing_chars = pricing.chars().count();
            if pricing_chars > MAX_PRICING_CHARS {
                return Err(MarketError::Validation(format!(
                    "listing[{}] pricing length {} exceeds maximum {}",
                    i, pricing_chars, MAX_PRICING_CHARS
                )));
            }
        }
    }

    if drops.len() > MAX_CARD_DROPS {
        return Err(MarketError::Validation(format!(
            "drops count {} exceeds maximum {}",
            drops.len(),
            MAX_CARD_DROPS
        )));
    }

    for (i, drop) in drops.iter().enumerate() {
        if !drop.write_url.starts_with("https://") && !is_loopback_http(&drop.write_url) {
            return Err(MarketError::Validation(format!(
                "drop[{}] write_url must start with https://, got {}",
                i, drop.write_url
            )));
        }
        if !drop.read_url.starts_with("https://") && !is_loopback_http(&drop.read_url) {
            return Err(MarketError::Validation(format!(
                "drop[{}] read_url must start with https://, got {}",
                i, drop.read_url
            )));
        }
        crate::drop_access::parse_ed25519_pubkey_hex64(&drop.drop_pub)
            .map_err(|e| MarketError::Validation(format!("drop[{}] drop_pub error: {}", i, e)))?;

        if let Some(ref slot_id) = drop.slot_id {
            crate::drop_access::parse_ed25519_pubkey_hex64(slot_id)
                .map_err(|e| MarketError::Validation(format!("drop[{}] slot_id error: {}", i, e)))?;
        }
    }

    Ok(())
}

fn check_strict_base64url(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn sign_card(mut card: ContactCard, signing_key: &SigningKey) -> Result<String, MarketError> {
    let vk = signing_key.verifying_key();
    let root_did = create_user_id(&vk, None)
        .map_err(|e| MarketError::Identity(format!("Failed to create root did: {}", e)))?;

    if !card.did.trim().is_empty() {
        let pk = get_pubkey_from_user_id(&card.did)
            .map_err(|e| MarketError::Identity(format!("Invalid card did: {}", e)))?;
        if pk.as_bytes() != vk.as_bytes() {
            return Err(MarketError::Identity(
                "Signing key does not match card DID public key".to_string(),
            ));
        }
    }

    card.did = root_did.clone();
    if card.profile.id.is_some() {
        card.profile.id = Some(root_did);
    }

    validate_card_fields(card.max_hops, &card.listings, &card.drops)?;

    let payload_canonical_json = serde_json_canonicalizer::to_string(&card)
        .map_err(|e| MarketError::Serialization(e.to_string()))?;

    if payload_canonical_json.len() > MAX_CARD_BYTES {
        return Err(MarketError::Validation(format!(
            "Canonical card JSON size {} exceeds MAX_CARD_BYTES {}",
            payload_canonical_json.len(),
            MAX_CARD_BYTES
        )));
    }

    let header_json = r#"{"alg":"EdDSA","typ":"hmc-card+jwt"}"#;
    let header_b64 = encode_base64(header_json.as_bytes());
    let payload_b64 = encode_base64(payload_canonical_json.as_bytes());

    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let signature = sign_ed25519(signing_key, signing_input.as_bytes());
    let sig_b64 = encode_base64(&signature.to_bytes());

    Ok(format!("{}.{}.{}", header_b64, payload_b64, sig_b64))
}

pub fn verify_card(jws: &str) -> Result<ContactCard, MarketError> {
    let parts: Vec<&str> = jws.split('.').collect();
    if parts.len() != 3 {
        return Err(MarketError::InvalidJws(format!(
            "Expected 3 parts separated by '.', got {}",
            parts.len()
        )));
    }

    let (header_b64, payload_b64, sig_b64) = (parts[0], parts[1], parts[2]);

    if !check_strict_base64url(header_b64)
        || !check_strict_base64url(payload_b64)
        || !check_strict_base64url(sig_b64)
    {
        return Err(MarketError::InvalidJws(
            "JWS parts must be strictly unpadded URL-safe base64".to_string(),
        ));
    }

    let header_bytes = decode_base64(header_b64)
        .map_err(|e| MarketError::InvalidJws(format!("Header base64 decode failed: {}", e)))?;
    let header: CardJwsHeader = serde_json::from_slice(&header_bytes)
        .map_err(|e| MarketError::InvalidJws(format!("Header JSON invalid: {}", e)))?;

    if header.alg != "EdDSA" || header.typ != "hmc-card+jwt" {
        return Err(MarketError::InvalidJws(format!(
            "Invalid JWS header alg/typ: expected EdDSA / hmc-card+jwt, got {} / {}",
            header.alg, header.typ
        )));
    }

    let payload_bytes = decode_base64(payload_b64)
        .map_err(|e| MarketError::InvalidJws(format!("Payload base64 decode failed: {}", e)))?;

    if payload_bytes.len() > MAX_CARD_BYTES {
        return Err(MarketError::Validation(format!(
            "Payload length {} exceeds MAX_CARD_BYTES {}",
            payload_bytes.len(),
            MAX_CARD_BYTES
        )));
    }

    let card: ContactCard = serde_json::from_slice(&payload_bytes)
        .map_err(|e| MarketError::Serialization(format!("Failed to parse ContactCard: {}", e)))?;

    let pk = get_pubkey_from_user_id(&card.did)
        .map_err(|e| MarketError::Identity(format!("Invalid card DID: {}", e)))?;
    let root_did = create_user_id(&pk, None)
        .map_err(|e| MarketError::Identity(format!("Failed to recreate root DID: {}", e)))?;

    if card.did != root_did {
        return Err(MarketError::Identity(format!(
            "Card DID must be canonical root prefix-free did:key, got {}",
            card.did
        )));
    }

    let sig_bytes = decode_base64(sig_b64)
        .map_err(|e| MarketError::InvalidJws(format!("Signature base64 decode failed: {}", e)))?;
    let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| {
        MarketError::InvalidJws("Signature must be exactly 64 bytes".to_string())
    })?;
    let signature = Signature::from_bytes(&sig_arr);

    let signing_input = format!("{}.{}", header_b64, payload_b64);
    if !verify_ed25519(&pk, signing_input.as_bytes(), &signature) {
        return Err(MarketError::Crypto("Invalid JWS signature".to_string()));
    }

    validate_card_fields(card.max_hops, &card.listings, &card.drops)?;

    Ok(card)
}

pub fn own_envelope(card_jws: String) -> CardEnvelope {
    CardEnvelope { card_jws, hops: 1 }
}

pub fn forward_envelope(stored: &StoredCard) -> Option<CardEnvelope> {
    if stored.hops < stored.card.max_hops {
        Some(CardEnvelope {
            card_jws: stored.card_jws.clone(),
            hops: stored.hops + 1,
        })
    } else {
        None
    }
}
