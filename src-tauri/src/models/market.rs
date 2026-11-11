use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldAudience {
    Everyone,
    Contacts,
    Nobody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardFieldVisibility {
    pub name: FieldAudience,
    pub organization: FieldAudience,
    pub community: FieldAudience,
    pub city: FieldAudience,
    pub url: FieldAudience,
    pub picture: FieldAudience,
    pub street: FieldAudience,
    pub email: FieldAudience,
    pub phone: FieldAudience,
    pub coordinates: FieldAudience,
    pub gender: FieldAudience,
}

impl Default for CardFieldVisibility {
    fn default() -> Self {
        Self {
            name: FieldAudience::Everyone,
            organization: FieldAudience::Everyone,
            community: FieldAudience::Everyone,
            city: FieldAudience::Everyone,
            url: FieldAudience::Everyone,
            picture: FieldAudience::Everyone,
            street: FieldAudience::Contacts,
            email: FieldAudience::Contacts,
            phone: FieldAudience::Contacts,
            coordinates: FieldAudience::Everyone,
            gender: FieldAudience::Nobody,
        }
    }
}

fn default_max_hops() -> u8 {
    humoco_market::DEFAULT_MAX_HOPS
}

fn default_validity_years() -> u8 {
    2
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredOwnMarketProfile {
    #[serde(default)]
    pub visibility: CardFieldVisibility,
    #[serde(default)]
    pub skills: Vec<humoco_market::Skill>,
    #[serde(default)]
    pub ads: Vec<humoco_market::Ad>,
    #[serde(default = "default_max_hops")]
    pub max_hops: u8,
    #[serde(default = "default_validity_years")]
    pub validity_years: u8,
    #[serde(default = "default_true")]
    pub allow_inquiries: bool,
    #[serde(default)]
    pub published: bool,
    #[serde(default)]
    pub last_card: Option<humoco_market::SdCard>,
}

impl Default for StoredOwnMarketProfile {
    fn default() -> Self {
        Self {
            visibility: CardFieldVisibility::default(),
            skills: Vec::new(),
            ads: Vec::new(),
            max_hops: humoco_market::DEFAULT_MAX_HOPS,
            validity_years: 2,
            allow_inquiries: true,
            published: false,
            last_card: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FrontendSkillOrigin {
    Card,
    Voucher,
}

impl From<humoco_market::SkillOrigin> for FrontendSkillOrigin {
    fn from(o: humoco_market::SkillOrigin) -> Self {
        match o {
            humoco_market::SkillOrigin::Card => Self::Card,
            humoco_market::SkillOrigin::Voucher => Self::Voucher,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMarketSkill {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<String>,
    #[serde(default)]
    pub is_remote: bool,
    pub origin: FrontendSkillOrigin,
}

impl From<humoco_market::MarketSkill> for FrontendMarketSkill {
    fn from(s: humoco_market::MarketSkill) -> Self {
        Self {
            title: s.title,
            text: s.text,
            pricing: s.pricing,
            is_remote: s.is_remote,
            origin: s.origin.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMyAd {
    pub id: String,
    pub kind: humoco_market::AdKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<String>,
    pub published_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renewed_at: Option<u64>,
    pub expires_at: u64,
    pub allow_inquiries: bool,
    #[serde(default)]
    pub is_remote: bool,
    pub status: String,
    pub days_left: u64,
}

impl FrontendMyAd {
    pub fn from_ad(ad: &humoco_market::Ad, now: u64) -> Self {
        let (status, days_left) = if ad.expires_at > now {
            let diff = ad.expires_at - now;
            let days = diff.div_ceil(86_400);
            ("active".to_string(), days)
        } else {
            ("expired".to_string(), 0)
        };

        Self {
            id: ad.id.clone(),
            kind: ad.kind,
            title: ad.title.clone(),
            text: ad.text.clone(),
            pricing: ad.pricing.clone(),
            published_at: ad.published_at,
            renewed_at: ad.renewed_at,
            expires_at: ad.expires_at,
            allow_inquiries: ad.allow_inquiries,
            is_remote: ad.is_remote,
            status,
            days_left,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMyMarketProfile {
    pub profile: crate::models::FrontendUserProfile,
    pub visibility: CardFieldVisibility,
    pub skills: Vec<humoco_market::Skill>,
    pub ads: Vec<FrontendMyAd>,
    pub max_hops: u8,
    pub validity_years: u8,
    pub allow_inquiries: bool,
    pub published: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_wire: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_svg: Option<String>,
    #[serde(default)]
    pub qr_is_partial: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSaveMyMarketProfileRequest {
    pub profile: crate::models::FrontendUserProfile,
    pub visibility: CardFieldVisibility,
    pub skills: Vec<humoco_market::Skill>,
    pub max_hops: u8,
    #[serde(default = "default_validity_years")]
    pub validity_years: u8,
    pub allow_inquiries: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAdInput {
    pub id: Option<String>,
    pub kind: humoco_market::AdKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<String>,
    pub duration_days: u64,
    pub allow_inquiries: bool,
    #[serde(default)]
    pub is_remote: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendBoardItem {
    pub owner_did: String,
    pub owner_name: String,
    pub trust: FrontendTrustTier,
    pub ad: humoco_market::Ad,
    pub days_left: u64,
}

impl FrontendBoardItem {
    pub fn from_board_item(bi: humoco_market::BoardItem, now: u64) -> Self {
        let days_left = if bi.ad.expires_at > now {
            (bi.ad.expires_at - now).div_ceil(86_400)
        } else {
            0
        };
        Self {
            owner_did: bi.owner_did,
            owner_name: bi.owner_name,
            trust: bi.trust.into(),
            ad: bi.ad,
            days_left,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FrontendEntrySource {
    Voucher,
    Card { hops: u8 },
    #[serde(rename_all = "camelCase")]
    Sync { read_url: String },
}

impl From<humoco_market::EntrySource> for FrontendEntrySource {
    fn from(s: humoco_market::EntrySource) -> Self {
        match s {
            humoco_market::EntrySource::Voucher => Self::Voucher,
            humoco_market::EntrySource::Card { hops } => Self::Card { hops },
            humoco_market::EntrySource::Drop { read_url } => Self::Sync { read_url },
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
pub struct FrontendSyncDescriptor {
    pub write_url: String,
    pub read_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_id: Option<String>,
    pub drop_pub: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error_at: Option<u64>,
    #[serde(default)]
    pub fail_count: u32,
}

impl From<humoco_market::DropDescriptor> for FrontendSyncDescriptor {
    fn from(d: humoco_market::DropDescriptor) -> Self {
        Self {
            write_url: d.write_url,
            read_url: d.read_url,
            slot_id: d.slot_id,
            drop_pub: d.drop_pub,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
        }
    }
}

impl From<FrontendSyncDescriptor> for humoco_market::DropDescriptor {
    fn from(d: FrontendSyncDescriptor) -> Self {
        Self {
            write_url: d.write_url,
            read_url: d.read_url,
            slot_id: d.slot_id,
            drop_pub: d.drop_pub,
        }
    }
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
    pub skills: Vec<FrontendMarketSkill>,
    pub active_ads: Vec<humoco_market::Ad>,
    pub ads_pending: u8,
    pub allow_inquiries: bool,
    pub sources: Vec<FrontendEntrySource>,
    pub as_of: u64,
    pub standards: Vec<FrontendStandardRef>,
    pub trust: FrontendTrustTier,
    pub can_forward: bool,
    pub card_syncs: Vec<FrontendSyncDescriptor>,
}

impl From<humoco_market::MarketPerson> for FrontendMarketPerson {
    fn from(p: humoco_market::MarketPerson) -> Self {
        Self {
            did: p.did,
            profile: p.profile.into(),
            skills: p.skills.into_iter().map(Into::into).collect(),
            active_ads: p.active_ads,
            ads_pending: p.ads_pending,
            allow_inquiries: p.allow_inquiries,
            sources: p.sources.into_iter().map(Into::into).collect(),
            as_of: p.as_of,
            standards: p.standards.into_iter().map(Into::into).collect(),
            trust: p.trust.into(),
            can_forward: p.can_forward,
            card_syncs: p.card_drops.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMarketSnapshot {
    pub board: Vec<FrontendBoardItem>,
    pub people: Vec<FrontendMarketPerson>,
    #[serde(default)]
    pub stored_card_count: usize,
    pub now: u64,
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
pub struct FrontendSyncSetupDraft {
    pub descriptor: FrontendSyncDescriptor,
    pub data_dir: String,
    pub admin_pubkey: String,
    pub generation: u32,
    pub max_members: usize,
    pub max_total_storage_mb: usize,
    pub policy: FrontendMemberPolicyDto,
    pub zip_base64: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSyncAdminStatus {
    pub descriptor: FrontendSyncDescriptor,
    pub admin_pubkey: String,
    pub generation: u32,
    pub rotation_retire_at: Option<u64>,
    pub data_dir: String,
    pub max_members: usize,
    pub max_total_storage_mb: usize,
    pub used_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
    pub soft_warning: bool,
    pub card_policy: FrontendMemberPolicyDto,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub blocked_pubkeys: Vec<String>,
    pub direct_member_count: usize,
    pub hop2_member_count: usize,
    pub total_member_count: usize,
    pub active_slot_ids: Vec<String>,
    #[serde(default)]
    pub channel_counts: std::collections::BTreeMap<String, usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSyncAdminUpdateRequest {
    pub card_policy: Option<FrontendMemberPolicyDto>,
    pub allow: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
    pub blocked_pubkeys: Option<Vec<String>>,
    pub max_members: Option<usize>,
    pub max_total_storage_mb: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPublicSyncInfo {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
    pub register_url: Option<String>,
    pub access_pubkey: String,
    pub authorized_channels: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSyncOverviewStatus {
    pub connected_count: usize,
    pub pending_count: usize,
    pub friend_syncs: usize,
    pub public_syncs: usize,
    pub has_own_sync: bool,
    pub own_sync_soft_warning: bool,
    pub last_sync_at: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSyncConnectionTestResult {
    pub success: bool,
    pub message: String,
    pub write_ok: bool,
    pub read_ok: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDiscoveredSync {
    pub descriptor: FrontendSyncDescriptor,
    pub is_authorized: bool,
    pub is_selected: bool,
    pub source_label: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMarketSyncResult {
    pub skipped_wifi_only: bool,
    pub uploaded_to: usize,
    pub polled_syncs: usize,
    pub decrypted_slots: usize,
    pub imported_cards: usize,
    pub imported_wot_lists: usize,
    pub errors: Vec<String>,
    pub retry_after_secs: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTrustChangeImpact {
    pub can_read_my_market_before: bool,
    pub can_read_my_market_after: bool,
    pub can_read_my_wot_before: bool,
    pub can_read_my_wot_after: bool,
    pub can_write_my_sync_before: bool,
    pub can_write_my_sync_after: bool,
    pub can_send_me_post_before: bool,
    pub can_send_me_post_after: bool,
}
