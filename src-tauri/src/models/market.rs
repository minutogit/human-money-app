use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FrontendListingType {
    Offer,
    Request,
}

impl From<humoco_market::ListingType> for FrontendListingType {
    fn from(t: humoco_market::ListingType) -> Self {
        match t {
            humoco_market::ListingType::Offer => Self::Offer,
            humoco_market::ListingType::Request => Self::Request,
        }
    }
}

impl From<FrontendListingType> for humoco_market::ListingType {
    fn from(t: FrontendListingType) -> Self {
        match t {
            FrontendListingType::Offer => Self::Offer,
            FrontendListingType::Request => Self::Request,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FrontendListingOrigin {
    Voucher,
    Card,
}

impl From<humoco_market::ListingOrigin> for FrontendListingOrigin {
    fn from(o: humoco_market::ListingOrigin) -> Self {
        match o {
            humoco_market::ListingOrigin::Voucher => Self::Voucher,
            humoco_market::ListingOrigin::Card => Self::Card,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMarketListing {
    pub listing_type: FrontendListingType,
    pub title: String,
    pub text: Option<String>,
    pub pricing: Option<String>,
    pub origin: FrontendListingOrigin,
}

impl From<humoco_market::MarketListing> for FrontendMarketListing {
    fn from(l: humoco_market::MarketListing) -> Self {
        Self {
            listing_type: l.listing_type.into(),
            title: l.title,
            text: l.text,
            pricing: l.pricing,
            origin: l.origin.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FrontendEntrySource {
    Voucher,
    Card { hops: u8 },
    #[serde(rename_all = "camelCase")]
    Drop { read_url: String },
}

impl From<humoco_market::EntrySource> for FrontendEntrySource {
    fn from(s: humoco_market::EntrySource) -> Self {
        match s {
            humoco_market::EntrySource::Voucher => Self::Voucher,
            humoco_market::EntrySource::Card { hops } => Self::Card { hops },
            humoco_market::EntrySource::Drop { read_url } => Self::Drop { read_url },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FrontendTrustTier {
    Trusted,
    Unknown,
    Hidden,
}

impl From<humoco_market::TrustTier> for FrontendTrustTier {
    fn from(t: humoco_market::TrustTier) -> Self {
        match t {
            humoco_market::TrustTier::Trusted => Self::Trusted,
            humoco_market::TrustTier::Unknown => Self::Unknown,
            humoco_market::TrustTier::Hidden => Self::Hidden,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendStandardRef {
    pub name: String,
    pub uuid: String,
}

impl From<humoco_market::StandardRef> for FrontendStandardRef {
    fn from(s: humoco_market::StandardRef) -> Self {
        Self {
            name: s.name,
            uuid: s.uuid,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDropDescriptor {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
}

impl From<humoco_market::DropDescriptor> for FrontendDropDescriptor {
    fn from(d: humoco_market::DropDescriptor) -> Self {
        Self {
            write_url: d.write_url,
            read_url: d.read_url,
            drop_pub: d.drop_pub,
        }
    }
}

impl From<FrontendDropDescriptor> for humoco_market::DropDescriptor {
    fn from(d: FrontendDropDescriptor) -> Self {
        Self {
            write_url: d.write_url,
            read_url: d.read_url,
            drop_pub: d.drop_pub,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendCardListingInput {
    pub listing_type: FrontendListingType,
    pub title: String,
    pub text: Option<String>,
    pub pricing: Option<String>,
}

impl From<humoco_market::Listing> for FrontendCardListingInput {
    fn from(l: humoco_market::Listing) -> Self {
        Self {
            listing_type: l.listing_type.into(),
            title: l.title,
            text: l.text,
            pricing: l.pricing,
        }
    }
}

impl From<FrontendCardListingInput> for humoco_market::Listing {
    fn from(l: FrontendCardListingInput) -> Self {
        Self {
            listing_type: l.listing_type.into(),
            title: l.title,
            text: l.text,
            pricing: l.pricing,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendContactCardDto {
    pub did: String,
    pub v: u64,
    pub valid_until: u64,
    pub max_hops: u8,
    pub profile: crate::models::FrontendUserProfile,
    pub listings: Vec<FrontendCardListingInput>,
    pub drops: Vec<FrontendDropDescriptor>,
}

impl From<humoco_market::ContactCard> for FrontendContactCardDto {
    fn from(c: humoco_market::ContactCard) -> Self {
        Self {
            did: c.did,
            v: c.v,
            valid_until: c.valid_until,
            max_hops: c.max_hops,
            profile: c.profile.into(),
            listings: c.listings.into_iter().map(Into::into).collect(),
            drops: c.drops.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendOwnContactCard {
    pub card: FrontendContactCardDto,
    pub card_jws: String,
    pub qr_svg: Option<String>,
    pub share_uri: String,
    pub envelope_json: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSaveOwnCardRequest {
    pub profile: crate::models::FrontendUserProfile,
    pub listings: Vec<FrontendCardListingInput>,
    pub max_hops: u8,
    pub validity_days: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendForwardCardExport {
    pub did: String,
    pub envelope_json: String,
    pub share_uri: String,
    pub qr_svg: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendImportCardResult {
    pub status: String,
    pub did: Option<String>,
    pub replaced: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMarketPerson {
    pub did: String,
    pub profile: crate::models::FrontendUserProfile,
    pub listings: Vec<FrontendMarketListing>,
    pub sources: Vec<FrontendEntrySource>,
    pub as_of: u64,
    pub standards: Vec<FrontendStandardRef>,
    pub trust: FrontendTrustTier,
    pub can_forward: bool,
    pub card_drops: Vec<FrontendDropDescriptor>,
}

impl From<humoco_market::MarketPerson> for FrontendMarketPerson {
    fn from(p: humoco_market::MarketPerson) -> Self {
        Self {
            did: p.did,
            profile: p.profile.into(),
            listings: p.listings.into_iter().map(Into::into).collect(),
            sources: p.sources.into_iter().map(Into::into).collect(),
            as_of: p.as_of,
            standards: p.standards.into_iter().map(Into::into).collect(),
            trust: p.trust.into(),
            can_forward: p.can_forward,
            card_drops: p.card_drops.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMemberPolicyDto {
    pub direct_min: u8,
    pub hop2_enabled: bool,
    pub bridge_min: u8,
    pub hop2_min: u8,
}

impl Default for FrontendMemberPolicyDto {
    fn default() -> Self {
        humoco_market::MemberPolicy::default().into()
    }
}

impl From<humoco_market::MemberPolicy> for FrontendMemberPolicyDto {
    fn from(p: humoco_market::MemberPolicy) -> Self {
        Self {
            direct_min: p.direct_min,
            hop2_enabled: p.hop2_enabled,
            bridge_min: p.bridge_min,
            hop2_min: p.hop2_min,
        }
    }
}

impl From<FrontendMemberPolicyDto> for humoco_market::MemberPolicy {
    fn from(p: FrontendMemberPolicyDto) -> Self {
        Self {
            direct_min: p.direct_min,
            hop2_enabled: p.hop2_enabled,
            bridge_min: p.bridge_min,
            hop2_min: p.hop2_min,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDropSetupDraft {
    pub descriptor: FrontendDropDescriptor,
    pub data_dir: String,
    pub admin_token_hash: String,
    pub max_members: usize,
    pub policy: FrontendMemberPolicyDto,
    pub zip_base64: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDropAdminStatus {
    pub descriptor: FrontendDropDescriptor,
    pub data_dir: String,
    pub max_members: usize,
    pub card_policy: FrontendMemberPolicyDto,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub blocked_access_ids: Vec<String>,
    pub direct_member_count: usize,
    pub hop2_member_count: usize,
    pub total_member_count: usize,
    pub active_slot_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDropAdminUpdateRequest {
    pub card_policy: Option<FrontendMemberPolicyDto>,
    pub allow: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
    pub blocked_access_ids: Option<Vec<String>>,
    pub max_members: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDropConnectionTestResult {
    pub success: bool,
    pub message: String,
    pub write_ok: bool,
    pub read_ok: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDiscoveredDrop {
    pub descriptor: FrontendDropDescriptor,
    pub is_authorized: bool,
    pub is_selected: bool,
    pub source_label: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDropSyncResult {
    pub skipped_wifi_only: bool,
    pub uploaded_to: usize,
    pub polled_drops: usize,
    pub decrypted_slots: usize,
    pub imported_cards: usize,
    pub errors: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTrustChangeImpact {
    pub can_read_my_market_before: bool,
    pub can_read_my_market_after: bool,
    pub can_write_my_drop_before: bool,
    pub can_write_my_drop_after: bool,
}
