use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::sd_card::SdCard;

pub const MAX_AVATAR_DATA_URI_CHARS: usize = 4096;
pub const MAX_AD_MEDIA_BYTES: usize = 102_400;
pub const MAX_AD_IMAGE_DIM: u16 = 1024;
pub const AVATAR_IMAGE_DIM: u16 = 96;

pub const MAX_SKILLS_PER_CARD: usize = 8;
pub const MAX_SKILL_TITLE_CHARS: usize = 60;
pub const MAX_SKILL_TEXT_CHARS: usize = 160;
pub const MAX_ADS_PER_CARD: usize = 5;
pub const MAX_AD_TITLE_CHARS: usize = 80;
pub const MAX_AD_TEXT_CHARS: usize = 280;
pub const MAX_PRICING_CHARS: usize = 40;

pub const AD_DURATION_CHOICES_DAYS: [u64; 3] = [30, 90, 180];
pub const AD_DEFAULT_DURATION_DAYS: u64 = 90;
pub const AD_MAX_DURATION_SECS: u64 = 180 * 86_400;

pub const CARD_DEFAULT_VALIDITY_SECS: u64 = 2 * 365 * 86_400;
pub const CARD_AUTO_RENEW_THRESHOLD_SECS: u64 = 30 * 86_400;
pub const CARD_SD_PADDED_LEN: usize = 32;

pub const MAX_CARD_WIRE_BYTES: usize = 10_240;
pub const MAX_BUNDLE_CARD_WIRE_BYTES: usize = 7_168;
pub const MAX_QR_SHARE_BYTES: usize = 2900;

pub const DEFAULT_MAX_HOPS: u8 = 2;
pub const MAX_CARD_DROPS: usize = 3;
pub const MAX_CARDS_PER_PUBKEY: usize = 3;
pub const MAX_STORED_CARDS: usize = 2000;
pub const MAX_SLOT_RECIPIENTS: usize = 256;

pub fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub title: String, // 1..=MAX_SKILL_TITLE_CHARS
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>, // ..=MAX_SKILL_TEXT_CHARS
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<String>, // ..=MAX_PRICING_CHARS
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_remote: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdKind {
    Offer,
    Request,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ad {
    pub id: String, // 16 lowercase hex chars, CSPRNG, stable across edits & renewals
    pub kind: AdKind,
    pub title: String, // 1..=MAX_AD_TITLE_CHARS
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>, // ..=MAX_AD_TEXT_CHARS
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<String>, // ..=MAX_PRICING_CHARS
    pub published_at: u64, // Unix seconds, unchanged on renewal
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renewed_at: Option<u64>, // Last renewal timestamp
    pub expires_at: u64, // published_at/renewed_at + duration, max +180 days
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_inquiries: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_remote: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireFilter {
    Full,
    NoAds,
    FitBytes(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardPayloadInput {
    pub did: String,
    pub v: u64,
    pub valid_until: u64,
    pub max_hops: u8,
    pub ad_count: u8,
    pub allow_inquiries: bool,
    pub drops: Vec<DropDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropDescriptor {
    pub write_url: String,
    pub read_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_id: Option<String>,
    pub drop_pub: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CardEnvelope {
    pub card_wire: String,
    pub hops: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StoredCard {
    pub card: SdCard,
    pub card_wire: String,
    pub hops: u8,
    pub received_at: u64,
}

pub mod hex_key_map {
    use serde::{de::Deserializer, ser::Serializer, Deserialize, Serialize};
    use std::collections::BTreeMap;

    pub fn serialize<S, V>(map: &BTreeMap<[u8; 32], V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: Serialize,
    {
        use serde::ser::SerializeMap;
        let mut ser_map = serializer.serialize_map(Some(map.len()))?;
        for (k, v) in map {
            ser_map.serialize_entry(&hex::encode(k), v)?;
        }
        ser_map.end()
    }

    pub fn deserialize<'de, D, V>(deserializer: D) -> Result<BTreeMap<[u8; 32], V>, D::Error>
    where
        D: Deserializer<'de>,
        V: Deserialize<'de>,
    {
        let str_map = BTreeMap::<String, V>::deserialize(deserializer)?;
        let mut result = BTreeMap::new();
        for (k_str, v) in str_map {
            let bytes = hex::decode(&k_str).map_err(serde::de::Error::custom)?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| serde::de::Error::custom("expected 32 bytes for hex key"))?;
            result.insert(arr, v);
        }
        Ok(result)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct CardStore {
    pub cards: BTreeMap<String, StoredCard>,
    #[serde(with = "hex_key_map")]
    pub sent_log: BTreeMap<[u8; 32], u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntrySource {
    Voucher,
    Card { hops: u8 },
    Drop { read_url: String },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustTier {
    Trusted,
    Unknown,
    Hidden,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StandardRef {
    pub name: String,
    pub uuid: String,
}
