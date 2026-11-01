use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoucherTrustDto {
    pub score: f64,
    pub trust_percent: u8,
    pub is_own_voucher: bool,
    pub blocked_by_offender: bool,
    pub shortest_hops: Option<u8>,
    pub primary_bridge_did: Option<String>,
    pub primary_bridge_name: Option<String>,
    pub additional_paths_count: usize,
    pub via_guarantor: bool,
    pub best_path_names: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendWotSummaryDto {
    pub rated_contacts_count: usize,
    pub verified_humans_count: usize,
    pub close_confidants_count: usize,
    pub imported_peer_lists_count: usize,
    pub own_list_version: Option<u64>,
    pub own_list_max_hops: u8,
    pub provider_filters_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendWotProviderDto {
    pub issuer_did: String,
    pub name: String,
    pub version: u64,
    pub issued_at: u64,
    pub source_url: Option<String>,
    pub member_count: u32,
    pub trust_level: u8,
    pub imported_at: u64,
    pub is_stale: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendWotImportResultDto {
    pub sender_did: String,
    pub sender_profile: Option<super::profile::FrontendUserProfile>,
    pub imported: usize,
    pub updated: usize,
    pub merged: usize,
    pub skipped_unchanged: usize,
    pub rejected_spam: usize,
    pub is_known_contact: bool,
    pub existing_trust_level: Option<u8>,
    #[serde(default)]
    pub is_provider_filter: bool,
    pub provider_name: Option<String>,
    pub provider_member_count: Option<u32>,
}
