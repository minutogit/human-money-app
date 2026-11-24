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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredOwnAd {
    pub ad: humoco_market::Ad,
    pub published: bool,
}

impl StoredOwnAd {
    pub fn is_published_active(&self, now: u64) -> bool {
        self.published && self.ad.expires_at > now
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredOwnMarketProfile {
    #[serde(default)]
    pub visibility: CardFieldVisibility,
    #[serde(default)]
    pub skills: Vec<humoco_market::Skill>,
    #[serde(default)]
    pub ads: Vec<StoredOwnAd>,
    #[serde(default = "default_max_hops")]
    pub max_hops: u8,
    pub shrink_hops: u8,
    pub shrink_until: u64,
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
            shrink_hops: 0,
            shrink_until: 0,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_at: Option<u64>,
    #[serde(default)]
    pub event_all_day: bool,
    pub published: bool,
}

impl FrontendMyAd {
    pub fn from_own_ad(entry: &StoredOwnAd, now: u64) -> Self {
        let ad = &entry.ad;
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
            image_token: ad.image_token.clone(),
            event_at: ad.event_at,
            event_all_day: ad.event_all_day,
            published: entry.published,
        }
    }
}

fn default_media_max_bytes() -> usize {
    humoco_market::MAX_AD_MEDIA_BYTES
}

fn default_media_max_count() -> usize {
    humoco_market::MAX_ADS_PER_CARD
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
    #[serde(default = "default_media_max_bytes")]
    pub media_max_bytes: usize,
    #[serde(default = "default_media_max_count")]
    pub media_max_count: usize,
    #[serde(default)]
    pub media_used_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrontendSaveMyMarketProfileRequest {
    pub profile: crate::models::FrontendUserProfile,
    pub visibility: CardFieldVisibility,
    pub skills: Vec<humoco_market::Skill>,
    pub max_hops: u8,
    #[serde(default = "default_validity_years")]
    pub validity_years: u8,
    pub allow_inquiries: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<bool>,
}

fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_double_option"
    )]
    pub image_webp_data_uri: Option<Option<String>>,
    #[serde(default)]
    pub event_at: Option<u64>,
    #[serde(default)]
    pub event_all_day: bool,
    #[serde(default)]
    pub published: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendBoardItem {
    pub owner_did: String,
    pub recipient_did: String,
    pub owner_name: String,
    pub trust: FrontendTrustTier,
    pub ad: humoco_market::Ad,
    pub days_left: u64,
    pub is_own: bool,
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
            recipient_did: bi.recipient_did,
            owner_name: bi.owner_name,
            trust: bi.trust.into(),
            ad: bi.ad,
            days_left,
            is_own: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FrontendEntrySource {
    Voucher,
    Card {
        hops: u8,
    },
    #[serde(rename_all = "camelCase")]
    Sync {
        read_url: String,
    },
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrontendSyncReplacementTarget {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
}

impl From<crate::sync_client::SyncReplacementTarget> for FrontendSyncReplacementTarget {
    fn from(r: crate::sync_client::SyncReplacementTarget) -> Self {
        Self {
            write_url: r.write_url,
            read_url: r.read_url,
            drop_pub: r.drop_pub,
        }
    }
}

impl From<FrontendSyncReplacementTarget> for crate::sync_client::SyncReplacementTarget {
    fn from(r: FrontendSyncReplacementTarget) -> Self {
        Self {
            write_url: r.write_url,
            read_url: r.read_url,
            drop_pub: r.drop_pub,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    #[serde(default = "default_sync_status")]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_probed_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunset_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<FrontendSyncReplacementTarget>,
}

pub trait SyncRevocationFields {
    fn status_mut(&mut self) -> &mut String;
    fn sunset_at_mut(&mut self) -> &mut Option<u64>;
    fn revocation_reason_mut(&mut self) -> &mut Option<String>;
    fn replacement_mut(&mut self) -> &mut Option<FrontendSyncReplacementTarget>;
}

impl SyncRevocationFields for FrontendSyncDescriptor {
    fn status_mut(&mut self) -> &mut String {
        &mut self.status
    }

    fn sunset_at_mut(&mut self) -> &mut Option<u64> {
        &mut self.sunset_at
    }

    fn revocation_reason_mut(&mut self) -> &mut Option<String> {
        &mut self.revocation_reason
    }

    fn replacement_mut(&mut self) -> &mut Option<FrontendSyncReplacementTarget> {
        &mut self.replacement
    }
}

pub fn default_sync_status() -> String {
    "unverified".to_string()
}

impl Default for FrontendSyncDescriptor {
    fn default() -> Self {
        Self {
            write_url: String::new(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: default_sync_status(),
            last_probed_at: None,
            sunset_at: None,
            revocation_reason: None,
            replacement: None,
        }
    }
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
            status: "unverified".to_string(),
            last_probed_at: None,
            sunset_at: None,
            revocation_reason: None,
            replacement: None,
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
    pub recipient_did: String,
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
            recipient_did: p.recipient_did,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    pub max_members: usize,
    pub max_total_storage_mb: usize,
    pub max_user_storage_mb: usize,
    pub policy: FrontendMemberPolicyDto,
    pub zip_base64: String,
    #[serde(default)]
    pub rust_config_toml: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSyncRevocationNotice {
    pub drop_pub: String,
    pub issued_at: u64,
    pub effective_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<FrontendSyncReplacementTarget>,
    pub signature: String,
}

impl From<crate::sync_client::SyncRevocationNotice> for FrontendSyncRevocationNotice {
    fn from(n: crate::sync_client::SyncRevocationNotice) -> Self {
        Self {
            drop_pub: n.drop_pub,
            issued_at: n.issued_at,
            effective_at: n.effective_at,
            reason: n.reason,
            replacement: n.replacement.map(Into::into),
            signature: n.signature,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSyncAdminStatus {
    pub descriptor: FrontendSyncDescriptor,
    pub admin_pubkey: String,
    pub data_dir: String,
    pub max_members: usize,
    pub max_total_storage_mb: usize,
    pub max_user_storage_mb: usize,
    pub used_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
    pub soft_warning: bool,
    pub card_policy: FrontendMemberPolicyDto,
    pub allow: Vec<String>,
    pub total_member_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<FrontendSyncRevocationNotice>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrontendSyncAdminUpdateRequest {
    pub card_policy: Option<FrontendMemberPolicyDto>,
    pub allow: Option<Vec<String>>,
    pub max_members: Option<usize>,
    pub max_total_storage_mb: Option<usize>,
    pub max_user_storage_mb: Option<usize>,
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
    pub last_sync_at: Option<u64>,
    #[serde(default)]
    pub pending_auth_count: usize,
    #[serde(default)]
    pub failing_count: usize,
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
    #[serde(default = "default_sync_status")]
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MarketSyncStatus {
    Completed,
    SkippedWifiOnly,
    SkippedCooldown,
    SessionSuperseded,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMarketSyncResult {
    pub status: MarketSyncStatus,
    pub uploaded_to: usize,
    pub polled_syncs: usize,
    pub decrypted_slots: usize,
    pub imported_cards: usize,
    pub new_ads: usize,
    pub imported_wot_lists: usize,
    pub errors: Vec<String>,
    pub retry_after_secs: Option<u64>,
    #[serde(default)]
    pub revoked_own_syncs: Vec<FrontendSyncDescriptor>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAppNotice {
    pub id: String,
    pub kind: String, // "payment" | "inquiry" | "market" | "trust"
    pub created_at: u64,
    pub read: bool,
    pub imported_payments: usize,
    pub imported_receipts: usize,
    pub imported_signatures: usize,
    pub new_sign_requests: usize,
    pub new_wot_handshakes: usize,
    pub new_inquiries: usize,
    pub new_inquiry_messages: usize,
    pub imported_cards: usize,
    pub new_ads: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inquiry_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct StoredAppNotices {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<u64>,
    #[serde(default)]
    pub notices: Vec<FrontendAppNotice>,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendResolvedCardRecipient {
    pub recipient_id: String,
    pub did: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[cfg(test)]
mod s11_tests {
    use super::*;

    fn assert_accepts_base_and_rejects_unknown<
        T: serde::de::DeserializeOwned + std::fmt::Debug,
    >(
        label: &str,
        valid_json: &str,
        with_unknown_json: &str,
        unknown_field: &str,
    ) {
        serde_json::from_str::<T>(valid_json).unwrap_or_else(|e| {
            panic!("{}: baseline valid JSON must deserialize cleanly: {}", label, e)
        });
        let err = serde_json::from_str::<T>(with_unknown_json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("unknown field `{}`", unknown_field)),
            "{}: expected unknown field `{}` error, got: {}",
            label,
            unknown_field,
            msg
        );
    }

    #[test]
    fn s11_f1_market_dtos_deny_unknown_fields_and_revocation_camel_case() {
        // Invariant: all frontend-facing request/config DTOs enforce deny_unknown_fields on top of valid baselines.
        assert_accepts_base_and_rejects_unknown::<FrontendSyncReplacementTarget>(
            "FrontendSyncReplacementTarget",
            r#"{"writeUrl":"https://a.example/sync.php","readUrl":"https://a.example/d","dropPub":"aa"}"#,
            r#"{"writeUrl":"https://a.example/sync.php","readUrl":"https://a.example/d","dropPub":"aa","extra":1}"#,
            "extra",
        );
        assert_accepts_base_and_rejects_unknown::<FrontendSyncDescriptor>(
            "FrontendSyncDescriptor",
            r#"{"writeUrl":"https://a.example/sync.php","readUrl":"https://a.example/d","dropPub":"aa"}"#,
            r#"{"writeUrl":"https://a.example/sync.php","readUrl":"https://a.example/d","dropPub":"aa","extra":1}"#,
            "extra",
        );
        assert_accepts_base_and_rejects_unknown::<FrontendMemberPolicyDto>(
            "FrontendMemberPolicyDto",
            r#"{"directMin":2,"hop2Enabled":true,"bridgeMin":2,"hop2Min":2}"#,
            r#"{"directMin":2,"hop2Enabled":true,"bridgeMin":2,"hop2Min":2,"extra":1}"#,
            "extra",
        );
        assert_accepts_base_and_rejects_unknown::<FrontendSyncAdminUpdateRequest>(
            "FrontendSyncAdminUpdateRequest",
            r#"{}"#,
            r#"{"unknown":1}"#,
            "unknown",
        );
        assert_accepts_base_and_rejects_unknown::<FrontendAdInput>(
            "FrontendAdInput",
            r#"{"id":null,"kind":"offer","title":"T","durationDays":30,"allowInquiries":true}"#,
            r#"{"id":null,"kind":"offer","title":"T","durationDays":30,"allowInquiries":true,"bogus":1}"#,
            "bogus",
        );

        let valid_profile_req = FrontendSaveMyMarketProfileRequest {
            profile: crate::models::FrontendUserProfile::default(),
            visibility: CardFieldVisibility::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(false),
        };
        let valid_profile_json = serde_json::to_string(&valid_profile_req).unwrap();
        let mut with_bogus = serde_json::to_value(&valid_profile_req).unwrap();
        with_bogus
            .as_object_mut()
            .unwrap()
            .insert("bogus".to_string(), serde_json::json!(true));
        let with_bogus_json = serde_json::to_string(&with_bogus).unwrap();
        let parsed_profile =
            serde_json::from_str::<FrontendSaveMyMarketProfileRequest>(&valid_profile_json)
                .expect("valid FrontendSaveMyMarketProfileRequest JSON must deserialize");
        assert_eq!(parsed_profile, valid_profile_req);
        let profile_err =
            serde_json::from_str::<FrontendSaveMyMarketProfileRequest>(&with_bogus_json)
                .expect_err("FrontendSaveMyMarketProfileRequest must reject unknown field `bogus`");
        assert!(
            profile_err.to_string().contains("unknown field `bogus`"),
            "expected unknown field `bogus` error, got: {}",
            profile_err
        );

        let notice = crate::sync_client::SyncRevocationNotice {
            drop_pub: "11".repeat(32),
            issued_at: 1_700_000_000,
            effective_at: 1_700_100_000,
            reason: Some("Migrating host".to_string()),
            replacement: Some(crate::sync_client::SyncReplacementTarget {
                write_url: "https://new.example/sync.php".to_string(),
                read_url: "https://new.example/data".to_string(),
                drop_pub: "22".repeat(32),
            }),
            signature: "ab".repeat(64),
        };
        let status = FrontendSyncAdminStatus {
            descriptor: FrontendSyncDescriptor::default(),
            admin_pubkey: "11".repeat(32),
            data_dir: "data".to_string(),
            max_members: 100,
            max_total_storage_mb: 100,
            max_user_storage_mb: 25,
            used_bytes: None,
            max_bytes: None,
            soft_warning: false,
            card_policy: FrontendMemberPolicyDto::default(),
            allow: vec![],
            total_member_count: 0,
            revocation: Some(notice.into()),
        };
        let v = serde_json::to_value(&status).unwrap();
        let rev = v.get("revocation").expect("revocation serialized");
        assert_eq!(
            rev.get("effectiveAt").and_then(|x| x.as_u64()),
            Some(1_700_100_000)
        );
        assert!(rev.get("effective_at").is_none());
        assert_eq!(
            rev.get("replacement")
                .and_then(|r| r.get("writeUrl"))
                .and_then(|x| x.as_str()),
            Some("https://new.example/sync.php")
        );
        assert!(rev
            .get("replacement")
            .and_then(|r| r.get("write_url"))
            .is_none());
    }
}
