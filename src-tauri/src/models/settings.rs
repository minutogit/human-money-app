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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendLockServerItemDto {
    pub write_url: String,
    pub read_url: String,
    pub admin_pub: String,
    pub lock_pub: String,
    pub source: String, // "own" | "circle" | "public" | "cache"
    pub status: String, // "ok" | "unreachable" | "no_lock" | "unverified"
    pub trusted: bool,
    pub allow_pow_tickets: bool,
    pub max_retention_quarter: u16,
    pub probed_at_secs: u64,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendLockProtectionOverviewDto {
    pub status: String, // "ready" | "degraded" | "none" | "syncing"
    pub ready_trusted_count: usize,
    pub servers: Vec<FrontendLockServerItemDto>,
    pub has_offline_cached_anchor: bool,
    pub unresolved_intents_count: usize,
    pub pending_restore_reconciliation: bool,
}
