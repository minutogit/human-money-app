use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReachPresetDto {
    pub preset: String,
    pub own_list_max_hops: u8,
    pub card_max_hops: u8,
    pub share_min_level: u8,
    pub media_auto_load_max_hops: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnchorServerInputDto {
    pub write_url: String,
    pub nominated_by: String, // "creator" | "guarantor"
    #[serde(default)]
    pub guarantor_index: Option<u8>,
    #[serde(default)]
    pub asn: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendL2LockStatusDto {
    pub status: String, // "Ok" | "Syncing" | "NoServers" | "Degraded"
    pub unresolved_intents_count: usize,
    pub pending_restore_reconciliation: bool,
    pub templates_count: usize,
    pub has_own_nominated_server: bool,
    pub stale_servers_count: usize,
    pub max_probe_age_secs: Option<u64>,
}
