use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

pub const MAX_LISTINGS_PER_CARD: usize = 10;
pub const MAX_LISTING_TITLE_CHARS: usize = 80;
pub const MAX_LISTING_TEXT_CHARS: usize = 280;
pub const MAX_PRICING_CHARS: usize = 40;
pub const MAX_CARD_BYTES: usize = 4096;
pub const CARD_DEFAULT_VALIDITY_SECS: u64 = 60 * 86_400; // 60 days
pub const DEFAULT_MAX_HOPS: u8 = 2;
pub const MAX_CARD_DROPS: usize = 3;
pub const MAX_STORED_CARDS: usize = 2000;
pub const MAX_FORWARDED_CARDS: usize = 20;
pub const MAX_SLOT_RECIPIENTS: usize = 256;
pub const MAX_QR_SHARE_BYTES: usize = 2900;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ListingType {
    Offer,
    Request,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ListingOrigin {
    Voucher,
    Card,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub listing_type: ListingType,
    pub title: String,
    pub text: Option<String>,
    pub pricing: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DropDescriptor {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ContactCard {
    pub did: String, // canonical, prefix-free did:key
    pub v: u64,      // max(last_v + 1, now_unix)
    pub valid_until: u64,
    pub max_hops: u8, // 1..=3
    pub profile: human_money_core::models::profile::PublicProfile,
    pub listings: Vec<Listing>,
    #[serde(default)]
    pub drops: Vec<DropDescriptor>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CardEnvelope {
    pub card_jws: String,
    pub hops: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StoredCard {
    pub card: ContactCard,
    pub card_jws: String,
    pub hops: u8,
    pub received_at: u64,
}

pub mod hex_key_map {
    use std::collections::BTreeMap;
    use serde::{de::Deserializer, ser::Serializer, Deserialize, Serialize};

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

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct CardStore {
    #[serde(with = "hex_key_map")]
    pub cards: BTreeMap<[u8; 32], StoredCard>,
    #[serde(with = "hex_key_map")]
    pub sent_log: BTreeMap<[u8; 32], u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MarketListing {
    pub listing_type: ListingType,
    pub title: String,
    pub text: Option<String>,
    pub pricing: Option<String>,
    pub origin: ListingOrigin,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MarketPerson {
    pub did: String,
    pub profile: human_money_core::models::profile::PublicProfile,
    pub listings: Vec<MarketListing>,
    pub sources: Vec<EntrySource>,
    pub as_of: u64,
    pub standards: Vec<StandardRef>,
    pub trust: TrustTier,
    pub can_forward: bool,
    pub card_drops: Vec<DropDescriptor>,
}
