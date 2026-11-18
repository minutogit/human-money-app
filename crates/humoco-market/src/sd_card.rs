use std::collections::{BTreeMap, HashSet};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, SigningKey};
use human_money_core::crypto::{
    create_user_id, get_prefix_from_user_id, get_pubkey_from_user_id, sign_ed25519, verify_ed25519,
};
use human_money_core::models::profile::PublicProfile;
use human_money_core::models::voucher::Address;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::MarketError;
use crate::model::{
    is_false, Ad, CardPayloadInput, DropDescriptor, Skill, WireFilter, AD_MAX_DURATION_SECS,
    CARD_SD_PADDED_LEN, MAX_ADS_PER_CARD, MAX_AD_TEXT_CHARS, MAX_AD_TITLE_CHARS, MAX_CARD_DROPS,
    MAX_CARD_WIRE_BYTES, MAX_PRICING_CHARS, MAX_SKILLS_PER_CARD, MAX_SKILL_TEXT_CHARS,
    MAX_SKILL_TITLE_CHARS,
};

pub const SD_CARD_TYP: &str = "humoco-card+sd-jwt";
pub const SD_CARD_ALG: &str = "EdDSA";

pub type Digest128 = String;

pub fn compute_digest128(raw_b64: &str) -> Digest128 {
    let hash = Sha256::digest(raw_b64.as_bytes());
    URL_SAFE_NO_PAD.encode(&hash[0..16])
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardJwsHeader {
    pub alg: String,
    pub typ: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardPayload {
    pub did: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient_did: Option<String>,
    pub v: u64,
    pub valid_until: u64,
    pub max_hops: u8,
    pub ad_count: u8,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_inquiries: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drops: Vec<DropDescriptor>,
    pub _sd: Vec<Digest128>,
}

impl CardPayload {
    /// Returns the exact account ID to address payments and PoST transfers to
    /// (preserving any subaccount prefix, or falling back to the root `did`).
    pub fn recipient_id(&self) -> &str {
        self.recipient_did.as_deref().unwrap_or(&self.did)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "k", content = "v", rename_all = "snake_case")]
pub enum CardItem {
    Name {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        first_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_name: Option<String>,
    },
    Organization(String),
    Community(String),
    City {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        zip_code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        city: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        country: Option<String>,
    },
    Street {
        street: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        house_number: Option<String>,
    },
    Coordinates(String),
    Email(String),
    Phone(String),
    Url(String),
    Picture(String),
    Gender(String),
    Skill(Skill),
    Ad(Ad),
}

impl CardItem {
    pub fn singleton_kind(&self) -> Option<CardItemKind> {
        match self {
            CardItem::Name { .. } => Some(CardItemKind::Name),
            CardItem::Organization(_) => Some(CardItemKind::Organization),
            CardItem::Community(_) => Some(CardItemKind::Community),
            CardItem::City { .. } => Some(CardItemKind::City),
            CardItem::Street { .. } => Some(CardItemKind::Street),
            CardItem::Coordinates(_) => Some(CardItemKind::Coordinates),
            CardItem::Email(_) => Some(CardItemKind::Email),
            CardItem::Phone(_) => Some(CardItemKind::Phone),
            CardItem::Url(_) => Some(CardItemKind::Url),
            CardItem::Picture(_) => Some(CardItemKind::Picture),
            CardItem::Gender(_) => Some(CardItemKind::Gender),
            CardItem::Skill(_) | CardItem::Ad(_) => None,
        }
    }

    /// Fixed priority ranking for FitBytes budget truncation:
    /// Name, Community, Organization, City, Phone, Email, Url, Street, Coordinates, Gender, Skill, Picture, Ad.
    pub fn priority_rank(&self) -> u8 {
        match self {
            CardItem::Name { .. } => 1,
            CardItem::Community(_) => 2,
            CardItem::Organization(_) => 3,
            CardItem::City { .. } => 4,
            CardItem::Phone(_) => 5,
            CardItem::Email(_) => 6,
            CardItem::Url(_) => 7,
            CardItem::Street { .. } => 8,
            CardItem::Coordinates(_) => 9,
            CardItem::Gender(_) => 10,
            CardItem::Skill(_) => 11,
            CardItem::Picture(_) => 12,
            CardItem::Ad(_) => 13,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CardItemKind {
    Name,
    Organization,
    Community,
    City,
    Street,
    Coordinates,
    Email,
    Phone,
    Url,
    Picture,
    Gender,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdCard {
    pub jws: String,
    pub payload: CardPayload,
    pub disclosures: BTreeMap<Digest128, String>, // Digest -> raw Base64URL disclosure
}

pub fn new_ad_id() -> String {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn is_loopback_http(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://127.0.0.1")
        || lower.starts_with("http://localhost")
        || lower.starts_with("http://[::1]")
}

fn validate_drops(drops: &[DropDescriptor]) -> Result<(), MarketError> {
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
        crate::sync_access::parse_ed25519_pubkey_hex64(&drop.drop_pub)
            .map_err(|e| MarketError::Validation(format!("drop[{}] drop_pub error: {}", i, e)))?;

        if let Some(ref slot_id) = drop.slot_id {
            crate::sync_access::parse_slot_id(slot_id).map_err(|e| {
                MarketError::Validation(format!("drop[{}] slot_id error: {}", i, e))
            })?;
        }
    }
    Ok(())
}

fn check_strict_base64url(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn validate_card_item_fields(item: &CardItem, valid_until: u64) -> Result<(), MarketError> {
    match item {
        CardItem::Name {
            first_name,
            last_name,
        } => {
            let has_first = first_name.as_ref().is_some_and(|s| !s.trim().is_empty());
            let has_last = last_name.as_ref().is_some_and(|s| !s.trim().is_empty());
            if !has_first && !has_last {
                return Err(MarketError::Validation(
                    "Name item must have at least one non-empty field".to_string(),
                ));
            }
            if let Some(ref f) = first_name {
                if f.chars().count() > 80 {
                    return Err(MarketError::Validation(
                        "first_name exceeds 80 characters".to_string(),
                    ));
                }
            }
            if let Some(ref l) = last_name {
                if l.chars().count() > 80 {
                    return Err(MarketError::Validation(
                        "last_name exceeds 80 characters".to_string(),
                    ));
                }
            }
        }
        CardItem::Organization(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 100 {
                return Err(MarketError::Validation(
                    "Organization must be 1..=100 characters".to_string(),
                ));
            }
        }
        CardItem::Community(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 100 {
                return Err(MarketError::Validation(
                    "Community must be 1..=100 characters".to_string(),
                ));
            }
        }
        CardItem::City {
            zip_code,
            city,
            country,
        } => {
            let has_zip = zip_code.as_ref().is_some_and(|s| !s.trim().is_empty());
            let has_city = city.as_ref().is_some_and(|s| !s.trim().is_empty());
            let has_country = country.as_ref().is_some_and(|s| !s.trim().is_empty());
            if !has_zip && !has_city && !has_country {
                return Err(MarketError::Validation(
                    "City item must have at least one non-empty field".to_string(),
                ));
            }
            if let Some(ref z) = zip_code {
                if z.chars().count() > 20 {
                    return Err(MarketError::Validation(
                        "zip_code exceeds 20 characters".to_string(),
                    ));
                }
            }
            if let Some(ref c) = city {
                if c.chars().count() > 80 {
                    return Err(MarketError::Validation(
                        "city exceeds 80 characters".to_string(),
                    ));
                }
            }
            if let Some(ref co) = country {
                if co.chars().count() > 80 {
                    return Err(MarketError::Validation(
                        "country exceeds 80 characters".to_string(),
                    ));
                }
            }
        }
        CardItem::Street {
            street,
            house_number,
        } => {
            let trimmed_st = street.trim();
            if trimmed_st.is_empty() || trimmed_st.chars().count() > 100 {
                return Err(MarketError::Validation(
                    "street must be 1..=100 characters".to_string(),
                ));
            }
            if let Some(ref hn) = house_number {
                if hn.chars().count() > 20 {
                    return Err(MarketError::Validation(
                        "house_number exceeds 20 characters".to_string(),
                    ));
                }
            }
        }
        CardItem::Coordinates(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 60 {
                return Err(MarketError::Validation(
                    "Coordinates must be 1..=60 characters".to_string(),
                ));
            }
        }
        CardItem::Email(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 100 {
                return Err(MarketError::Validation(
                    "Email must be 1..=100 characters".to_string(),
                ));
            }
        }
        CardItem::Phone(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 40 {
                return Err(MarketError::Validation(
                    "Phone must be 1..=40 characters".to_string(),
                ));
            }
        }
        CardItem::Url(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 200 {
                return Err(MarketError::Validation(
                    "Url must be 1..=200 characters".to_string(),
                ));
            }
        }
        CardItem::Picture(s) => {
            if crate::media::is_supported_media_data_uri(s) {
                crate::media::validate_avatar_data_uri(s)?;
            } else {
                let trimmed = s.trim();
                if trimmed.is_empty() || trimmed.chars().count() > 200 {
                    return Err(MarketError::Validation(
                        "Picture must be 1..=200 characters".to_string(),
                    ));
                }
            }
        }
        CardItem::Gender(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 20 {
                return Err(MarketError::Validation(
                    "Gender must be 1..=20 characters".to_string(),
                ));
            }
        }
        CardItem::Skill(s) => {
            let title_trimmed = s.title.trim();
            let title_count = title_trimmed.chars().count();
            if title_count == 0 || title_count > MAX_SKILL_TITLE_CHARS {
                return Err(MarketError::Validation(format!(
                    "Skill title length {} must be between 1 and {}",
                    title_count, MAX_SKILL_TITLE_CHARS
                )));
            }
            if let Some(ref text) = s.text {
                let text_count = text.chars().count();
                if text_count > MAX_SKILL_TEXT_CHARS {
                    return Err(MarketError::Validation(format!(
                        "Skill text length {} exceeds maximum {}",
                        text_count, MAX_SKILL_TEXT_CHARS
                    )));
                }
            }
            if let Some(ref pricing) = s.pricing {
                let pricing_count = pricing.chars().count();
                if pricing_count > MAX_PRICING_CHARS {
                    return Err(MarketError::Validation(format!(
                        "Skill pricing length {} exceeds maximum {}",
                        pricing_count, MAX_PRICING_CHARS
                    )));
                }
            }
        }
        CardItem::Ad(ad) => {
            if ad.id.len() != 16 || !ad.id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
                return Err(MarketError::Validation(format!(
                    "Ad id must be exactly 16 lowercase hex chars, got {}",
                    ad.id
                )));
            }
            let title_trimmed = ad.title.trim();
            let title_count = title_trimmed.chars().count();
            if title_count == 0 || title_count > MAX_AD_TITLE_CHARS {
                return Err(MarketError::Validation(format!(
                    "Ad title length {} must be between 1 and {}",
                    title_count, MAX_AD_TITLE_CHARS
                )));
            }
            if let Some(ref text) = ad.text {
                let text_count = text.chars().count();
                if text_count > MAX_AD_TEXT_CHARS {
                    return Err(MarketError::Validation(format!(
                        "Ad text length {} exceeds maximum {}",
                        text_count, MAX_AD_TEXT_CHARS
                    )));
                }
            }
            if let Some(ref pricing) = ad.pricing {
                let pricing_count = pricing.chars().count();
                if pricing_count > MAX_PRICING_CHARS {
                    return Err(MarketError::Validation(format!(
                        "Ad pricing length {} exceeds maximum {}",
                        pricing_count, MAX_PRICING_CHARS
                    )));
                }
            }
            let base = ad.renewed_at.unwrap_or(ad.published_at);
            if base < ad.published_at {
                return Err(MarketError::Validation(
                    "Ad renewed_at cannot be earlier than published_at".to_string(),
                ));
            }
            if ad.expires_at <= base {
                return Err(MarketError::Validation(
                    "Ad expires_at must be strictly greater than published/renewed base"
                        .to_string(),
                ));
            }
            if ad.expires_at - base > AD_MAX_DURATION_SECS {
                return Err(MarketError::Validation(format!(
                    "Ad duration {} secs exceeds AD_MAX_DURATION_SECS {}",
                    ad.expires_at - base,
                    AD_MAX_DURATION_SECS
                )));
            }
            if ad.expires_at > valid_until {
                return Err(MarketError::Validation(format!(
                    "Ad expires_at {} exceeds card valid_until {}",
                    ad.expires_at, valid_until
                )));
            }
            if let Some(ref token) = ad.image_token {
                if !crate::media::is_valid_image_token(token) {
                    return Err(MarketError::Validation(format!(
                        "Invalid ad image_token '{}': expected 32 lowercase hex chars",
                        token
                    )));
                }
            }
        }
    }
    Ok(())
}

pub fn validate_sd_card(card: &SdCard, wire_len: usize) -> Result<(), MarketError> {
    if wire_len > MAX_CARD_WIRE_BYTES {
        return Err(MarketError::Validation(format!(
            "Card wire size {} exceeds MAX_CARD_WIRE_BYTES {}",
            wire_len, MAX_CARD_WIRE_BYTES
        )));
    }

    if !(1..=3).contains(&card.payload.max_hops) {
        return Err(MarketError::Validation(format!(
            "max_hops must be 1..=3, got {}",
            card.payload.max_hops
        )));
    }

    if card.payload._sd.len() != CARD_SD_PADDED_LEN {
        return Err(MarketError::Validation(format!(
            "_sd length {} must equal CARD_SD_PADDED_LEN {}",
            card.payload._sd.len(),
            CARD_SD_PADDED_LEN
        )));
    }

    // Verify _sd digests are strictly sorted ascending and unique
    let mut seen_sd = HashSet::with_capacity(CARD_SD_PADDED_LEN);
    for (i, d) in card.payload._sd.iter().enumerate() {
        if !seen_sd.insert(d.clone()) {
            return Err(MarketError::Validation(format!(
                "Duplicate digest in _sd: {}",
                d
            )));
        }
        if i > 0 && card.payload._sd[i - 1] >= *d {
            return Err(MarketError::Validation(
                "_sd digests must be strictly sorted ascending".to_string(),
            ));
        }
    }

    if card.payload.ad_count > MAX_ADS_PER_CARD as u8 {
        return Err(MarketError::Validation(format!(
            "ad_count {} exceeds MAX_ADS_PER_CARD {}",
            card.payload.ad_count, MAX_ADS_PER_CARD
        )));
    }

    validate_drops(&card.payload.drops)?;

    let mut seen_kinds = HashSet::new();
    let mut skill_count = 0;
    let mut ad_count = 0;
    let mut seen_ad_ids = HashSet::new();

    for (digest, raw_b64) in &card.disclosures {
        if !seen_sd.contains(digest) {
            return Err(MarketError::Validation(format!(
                "Disclosure digest {} is not in card _sd",
                digest
            )));
        }
        let computed = compute_digest128(raw_b64);
        if &computed != digest {
            return Err(MarketError::Validation(format!(
                "Disclosure digest mismatch: computed {}, stored {}",
                computed, digest
            )));
        }

        let disc_bytes = URL_SAFE_NO_PAD.decode(raw_b64).map_err(|e| {
            MarketError::Validation(format!("Disclosure base64 decode failed: {e}"))
        })?;

        let (salt_b64, max_distance, item): (String, u8, CardItem) =
            serde_json::from_slice(&disc_bytes).map_err(|e| {
                MarketError::Validation(format!("Disclosure JSON parse failed: {e}"))
            })?;

        let salt_bytes = URL_SAFE_NO_PAD
            .decode(&salt_b64)
            .map_err(|e| MarketError::Validation(format!("Salt base64 decode failed: {e}")))?;
        if salt_bytes.len() < 16 {
            return Err(MarketError::Validation(format!(
                "Salt length {} must be at least 16 bytes",
                salt_bytes.len()
            )));
        }

        if max_distance < 1 || max_distance > card.payload.max_hops {
            return Err(MarketError::Validation(format!(
                "Disclosure max_distance {} out of range 1..={}",
                max_distance, card.payload.max_hops
            )));
        }

        if let Some(kind) = item.singleton_kind() {
            if !seen_kinds.insert(kind) {
                return Err(MarketError::Validation(format!(
                    "Duplicate singleton profile item kind {:?}",
                    kind
                )));
            }
        }

        match &item {
            CardItem::Skill(_) => {
                skill_count += 1;
                if skill_count > MAX_SKILLS_PER_CARD {
                    return Err(MarketError::Validation(format!(
                        "Skill count exceeds MAX_SKILLS_PER_CARD {}",
                        MAX_SKILLS_PER_CARD
                    )));
                }
            }
            CardItem::Ad(ad) => {
                ad_count += 1;
                if ad_count > MAX_ADS_PER_CARD {
                    return Err(MarketError::Validation(format!(
                        "Ad count exceeds MAX_ADS_PER_CARD {}",
                        MAX_ADS_PER_CARD
                    )));
                }
                if !seen_ad_ids.insert(ad.id.clone()) {
                    return Err(MarketError::Validation(format!(
                        "Duplicate Ad id on card: {}",
                        ad.id
                    )));
                }
            }
            _ => {}
        }

        validate_card_item_fields(&item, card.payload.valid_until)?;
    }

    Ok(())
}

pub fn sign_sd_card(
    payload_base: CardPayloadInput,
    items: Vec<(CardItem, u8)>,
    signing_key: &SigningKey,
) -> Result<SdCard, MarketError> {
    let vk = signing_key.verifying_key();
    let root_did = create_user_id(&vk, None)
        .map_err(|e| MarketError::Identity(format!("Failed to create root did: {}", e)))?;

    let mut recipient_did: Option<String> = None;
    if !payload_base.did.trim().is_empty() {
        let trimmed_did = payload_base.did.trim();
        let pk = get_pubkey_from_user_id(trimmed_did)
            .map_err(|e| MarketError::Identity(format!("Invalid card did: {}", e)))?;
        if pk.as_bytes() != vk.as_bytes() {
            return Err(MarketError::Identity(
                "Signing key does not match card DID public key".to_string(),
            ));
        }
        if let Some(prefix) = get_prefix_from_user_id(trimmed_did) {
            let full_id = create_user_id(&pk, Some(prefix))
                .map_err(|e| MarketError::Identity(format!("Invalid prefixed card did: {}", e)))?;
            if trimmed_did != full_id {
                return Err(MarketError::Identity(format!(
                    "Card recipient DID must be canonical (expected {}, got {})",
                    full_id, trimmed_did
                )));
            }
            recipient_did = Some(full_id);
        }
    }

    if !(1..=3).contains(&payload_base.max_hops) {
        return Err(MarketError::Validation(format!(
            "max_hops must be 1..=3, got {}",
            payload_base.max_hops
        )));
    }

    if items.len() > CARD_SD_PADDED_LEN {
        return Err(MarketError::Validation(format!(
            "Item count {} exceeds CARD_SD_PADDED_LEN {}",
            items.len(),
            CARD_SD_PADDED_LEN
        )));
    }

    let mut disclosures = BTreeMap::new();
    let mut sd_digests = Vec::with_capacity(CARD_SD_PADDED_LEN);
    let mut seen_digests = HashSet::new();

    for (item, max_distance) in &items {
        if *max_distance < 1 || *max_distance > payload_base.max_hops {
            return Err(MarketError::Validation(format!(
                "Item max_distance {} exceeds card max_hops {}",
                max_distance, payload_base.max_hops
            )));
        }

        validate_card_item_fields(item, payload_base.valid_until)?;

        let mut salt_bytes = [0u8; 16];
        OsRng.fill_bytes(&mut salt_bytes);
        let salt_b64 = URL_SAFE_NO_PAD.encode(salt_bytes);

        let tuple = (&salt_b64, max_distance, item);
        let tuple_json =
            serde_json::to_string(&tuple).map_err(|e| MarketError::Serialization(e.to_string()))?;
        let raw_b64 = URL_SAFE_NO_PAD.encode(tuple_json.as_bytes());

        let digest = compute_digest128(&raw_b64);
        if !seen_digests.insert(digest.clone()) {
            return Err(MarketError::Validation(
                "Digest collision during card signing".to_string(),
            ));
        }
        sd_digests.push(digest.clone());
        disclosures.insert(digest, raw_b64);
    }

    // Pad with random 16-byte CSPRNG decoy digests to CARD_SD_PADDED_LEN (32)
    while sd_digests.len() < CARD_SD_PADDED_LEN {
        let mut decoy_bytes = [0u8; 16];
        OsRng.fill_bytes(&mut decoy_bytes);
        let decoy_digest = URL_SAFE_NO_PAD.encode(decoy_bytes);
        if seen_digests.insert(decoy_digest.clone()) {
            sd_digests.push(decoy_digest);
        }
    }

    sd_digests.sort();

    let payload = CardPayload {
        did: root_did,
        recipient_did,
        v: payload_base.v,
        valid_until: payload_base.valid_until,
        max_hops: payload_base.max_hops,
        ad_count: payload_base.ad_count,
        allow_inquiries: payload_base.allow_inquiries,
        drops: payload_base.drops,
        _sd: sd_digests,
    };

    let header = CardJwsHeader {
        alg: SD_CARD_ALG.to_string(),
        typ: SD_CARD_TYP.to_string(),
    };

    let header_json =
        serde_json::to_string(&header).map_err(|e| MarketError::Serialization(e.to_string()))?;
    let payload_canonical_json = serde_json_canonicalizer::to_string(&payload)
        .map_err(|e| MarketError::Serialization(e.to_string()))?;

    let header_b64 = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_canonical_json.as_bytes());

    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let signature = sign_ed25519(signing_key, signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    let jws = format!("{}.{}.{}", header_b64, payload_b64, sig_b64);

    let card = SdCard {
        jws,
        payload,
        disclosures,
    };

    let full_wire = render_wire(&card, 1, WireFilter::Full)
        .ok_or_else(|| MarketError::Validation("Failed to render wire for signed card".into()))?;

    validate_sd_card(&card, full_wire.len())?;

    Ok(card)
}

pub fn verify_sd_card(wire: &str) -> Result<SdCard, MarketError> {
    if wire.len() > MAX_CARD_WIRE_BYTES {
        return Err(MarketError::Validation(format!(
            "Card wire size {} exceeds MAX_CARD_WIRE_BYTES {}",
            wire.len(),
            MAX_CARD_WIRE_BYTES
        )));
    }

    let parts: Vec<&str> = wire.split('~').collect();
    if parts.is_empty() {
        return Err(MarketError::InvalidJws("Empty card wire".to_string()));
    }

    let jws = parts[0];
    let jws_parts: Vec<&str> = jws.split('.').collect();
    if jws_parts.len() != 3 {
        return Err(MarketError::InvalidJws(format!(
            "Expected 3 parts in JWS separated by '.', got {}",
            jws_parts.len()
        )));
    }

    let (header_b64, payload_b64, sig_b64) = (jws_parts[0], jws_parts[1], jws_parts[2]);
    if !check_strict_base64url(header_b64)
        || !check_strict_base64url(payload_b64)
        || !check_strict_base64url(sig_b64)
    {
        return Err(MarketError::InvalidJws(
            "JWS parts must be strictly unpadded URL-safe base64".to_string(),
        ));
    }

    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|e| MarketError::InvalidJws(format!("Header base64 decode failed: {}", e)))?;
    let header: CardJwsHeader = serde_json::from_slice(&header_bytes)
        .map_err(|e| MarketError::InvalidJws(format!("Header JSON invalid: {}", e)))?;

    if header.alg != SD_CARD_ALG || header.typ != SD_CARD_TYP {
        return Err(MarketError::InvalidJws(format!(
            "Invalid JWS header alg/typ: expected {} / {}, got {} / {}",
            SD_CARD_ALG, SD_CARD_TYP, header.alg, header.typ
        )));
    }

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| MarketError::InvalidJws(format!("Payload base64 decode failed: {}", e)))?;

    let payload: CardPayload = serde_json::from_slice(&payload_bytes)
        .map_err(|e| MarketError::Serialization(format!("Failed to parse CardPayload: {}", e)))?;

    let pk = get_pubkey_from_user_id(&payload.did)
        .map_err(|e| MarketError::Identity(format!("Invalid card DID: {}", e)))?;
    let root_did = create_user_id(&pk, None)
        .map_err(|e| MarketError::Identity(format!("Failed to recreate root DID: {}", e)))?;

    if payload.did != root_did {
        return Err(MarketError::Identity(format!(
            "Card DID must be canonical root prefix-free did:key, got {}",
            payload.did
        )));
    }
    if let Some(ref rec_did) = payload.recipient_did {
        let rec_pk = get_pubkey_from_user_id(rec_did)
            .map_err(|e| MarketError::Identity(format!("Invalid card recipient_did: {}", e)))?;
        if rec_pk != pk {
            return Err(MarketError::Identity(
                "Card recipient_did public key does not match payload.did".to_string(),
            ));
        }
        let rec_prefix = get_prefix_from_user_id(rec_did).ok_or_else(|| {
            MarketError::Identity(
                "Card recipient_did must only be set for prefixed subaccount IDs".to_string(),
            )
        })?;
        let canonical_rec = create_user_id(&rec_pk, Some(rec_prefix))
            .map_err(|e| MarketError::Identity(format!("Invalid card recipient_did: {}", e)))?;
        if rec_did != &canonical_rec {
            return Err(MarketError::Identity(format!(
                "Card recipient_did must be canonical (expected {}, got {})",
                canonical_rec, rec_did
            )));
        }
    }

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|e| MarketError::InvalidJws(format!("Signature base64 decode failed: {}", e)))?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| MarketError::InvalidJws("Signature must be exactly 64 bytes".to_string()))?;
    let signature = Signature::from_bytes(&sig_arr);

    let signing_input = format!("{}.{}", header_b64, payload_b64);
    if !verify_ed25519(&pk, signing_input.as_bytes(), &signature) {
        return Err(MarketError::Crypto("Invalid JWS signature".to_string()));
    }

    let valid_sd_set: HashSet<&str> = payload._sd.iter().map(|s| s.as_str()).collect();
    let mut disclosures = BTreeMap::new();

    for raw_disc in &parts[1..] {
        if raw_disc.is_empty() {
            continue;
        }
        if !check_strict_base64url(raw_disc) {
            return Err(MarketError::Validation(
                "Disclosure token must be strictly unpadded URL-safe base64".to_string(),
            ));
        }

        let digest = compute_digest128(raw_disc);
        if !valid_sd_set.contains(digest.as_str()) {
            return Err(MarketError::Validation(format!(
                "Disclosure digest {} is not present in _sd",
                digest
            )));
        }

        // Duplicate disclosures in incoming wire are ignored (first occurrence retained)
        disclosures
            .entry(digest)
            .or_insert_with(|| raw_disc.to_string());
    }

    let card = SdCard {
        jws: jws.to_string(),
        payload,
        disclosures,
    };

    validate_sd_card(&card, wire.len())?;

    Ok(card)
}

pub fn decoded_items(card: &SdCard) -> Vec<(CardItem, u8)> {
    let mut items = Vec::new();
    for raw_b64 in card.disclosures.values() {
        if let Ok(bytes) = URL_SAFE_NO_PAD.decode(raw_b64) {
            if let Ok((_salt, max_distance, item)) =
                serde_json::from_slice::<(String, u8, CardItem)>(&bytes)
            {
                items.push((item, max_distance));
            }
        }
    }
    items
}

pub fn extract_public_profile(card: &SdCard) -> PublicProfile {
    let mut profile = PublicProfile {
        id: Some(card.payload.did.clone()),
        ..Default::default()
    };

    let mut street_opt: Option<(String, Option<String>)> = None;
    let mut city_opt: Option<(Option<String>, Option<String>, Option<String>)> = None;

    for (item, _) in decoded_items(card) {
        match item {
            CardItem::Name {
                first_name,
                last_name,
            } => {
                profile.first_name = first_name;
                profile.last_name = last_name;
            }
            CardItem::Organization(s) => profile.organization = Some(s),
            CardItem::Community(s) => profile.community = Some(s),
            CardItem::City {
                zip_code,
                city,
                country,
            } => {
                city_opt = Some((zip_code, city, country));
            }
            CardItem::Street {
                street,
                house_number,
            } => {
                street_opt = Some((street, house_number));
            }
            CardItem::Coordinates(s) => profile.coordinates = Some(s),
            CardItem::Email(s) => profile.email = Some(s),
            CardItem::Phone(s) => profile.phone = Some(s),
            CardItem::Url(s) => profile.url = Some(s),
            CardItem::Picture(s) => profile.picture_url = Some(s),
            CardItem::Gender(s) => profile.gender = Some(s),
            CardItem::Skill(_) | CardItem::Ad(_) => {}
        }
    }

    if street_opt.is_some() || city_opt.is_some() {
        let (st, hn) = street_opt.unwrap_or_default();
        let (zc, ci, co) = city_opt.unwrap_or_default();
        let mut parts = Vec::new();
        if !st.is_empty() {
            if let Some(ref h) = hn {
                parts.push(format!("{} {}", st, h));
            } else {
                parts.push(st.clone());
            }
        }
        let city_part = match (&zc, &ci) {
            (Some(z), Some(c)) => format!("{} {}", z, c),
            (Some(z), None) => z.clone(),
            (None, Some(c)) => c.clone(),
            (None, None) => String::new(),
        };
        if !city_part.is_empty() {
            parts.push(city_part);
        }
        if let Some(ref country_str) = co {
            parts.push(country_str.clone());
        }

        profile.address = Some(Address {
            street: st,
            house_number: hn.unwrap_or_default(),
            zip_code: zc.unwrap_or_default(),
            city: ci.unwrap_or_default(),
            country: co.unwrap_or_default(),
            full_address: parts.join(", "),
        });
    }

    profile
}

pub fn extract_skills(card: &SdCard) -> Vec<Skill> {
    decoded_items(card)
        .into_iter()
        .filter_map(|(item, _)| match item {
            CardItem::Skill(s) => Some(s),
            _ => None,
        })
        .collect()
}

pub fn active_ads(card: &SdCard, now: u64) -> Vec<Ad> {
    decoded_items(card)
        .into_iter()
        .filter_map(|(item, _)| match item {
            CardItem::Ad(ad) if ad.expires_at > now => Some(ad),
            _ => None,
        })
        .collect()
}

pub fn render_wire(card: &SdCard, target_distance: u8, filter: WireFilter) -> Option<String> {
    if target_distance == 0 || target_distance > card.payload.max_hops {
        return None;
    }

    // Bare wire with zero disclosures is `<jws>~`
    let bare_wire = format!("{}~", card.jws);

    let mut eligible: Vec<(&str, CardItem)> = Vec::new();
    for raw_b64 in card.disclosures.values() {
        if let Ok(bytes) = URL_SAFE_NO_PAD.decode(raw_b64) {
            if let Ok((_salt, max_distance, item)) =
                serde_json::from_slice::<(String, u8, CardItem)>(&bytes)
            {
                if max_distance >= target_distance {
                    eligible.push((raw_b64.as_str(), item));
                }
            }
        }
    }

    match filter {
        WireFilter::Full => {
            if eligible.is_empty() {
                Some(bare_wire)
            } else {
                let mut discs: Vec<&str> = eligible.into_iter().map(|(r, _)| r).collect();
                discs.sort();
                Some(format!("{}~{}~", card.jws, discs.join("~")))
            }
        }
        WireFilter::NoAds => {
            let mut discs: Vec<&str> = eligible
                .into_iter()
                .filter(|(_, item)| !matches!(item, CardItem::Ad(_)))
                .map(|(r, _)| r)
                .collect();
            if discs.is_empty() {
                Some(bare_wire)
            } else {
                discs.sort();
                Some(format!("{}~{}~", card.jws, discs.join("~")))
            }
        }
        WireFilter::FitBytes(budget) => {
            if bare_wire.len() > budget {
                return None;
            }

            // Exclude Picture and Ad items so QR codes remain 100% image-free and ad-free
            let mut eligible: Vec<(&str, CardItem)> = eligible
                .into_iter()
                .filter(|(_, item)| !matches!(item, CardItem::Picture(_) | CardItem::Ad(_)))
                .collect();

            // Sort eligible disclosures by item priority_rank ascending
            eligible.sort_by_key(|(_, item)| item.priority_rank());

            let mut selected = Vec::new();
            let mut current_len = bare_wire.len();

            for (raw_b64, _) in eligible {
                // Each added disclosure adds raw_b64.len() + 1 ('~')
                let additional_len = raw_b64.len() + 1;
                if current_len + additional_len <= budget {
                    selected.push(raw_b64);
                    current_len += additional_len;
                }
            }

            if selected.is_empty() {
                Some(bare_wire)
            } else {
                selected.sort();
                Some(format!("{}~{}~", card.jws, selected.join("~")))
            }
        }
    }
}

pub fn merge(stored: &mut SdCard, incoming: SdCard) -> bool {
    if stored.jws != incoming.jws {
        return false;
    }
    for (digest, raw_b64) in incoming.disclosures {
        stored.disclosures.entry(digest).or_insert(raw_b64);
    }
    true
}

pub fn strip_disclosures_beyond_distance(card: &mut SdCard, hops: u8) {
    card.disclosures.retain(|_, raw_b64| {
        if let Ok(bytes) = URL_SAFE_NO_PAD.decode(raw_b64) {
            if let Ok((_salt, max_distance, _item)) =
                serde_json::from_slice::<(String, u8, CardItem)>(&bytes)
            {
                return max_distance >= hops;
            }
        }
        false
    });
}
