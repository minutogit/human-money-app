// src-tauri/src/settings.rs
use serde::{Deserialize, Serialize};

pub const SETTINGS_KEY: &str = "app_settings";
pub const MAX_POLLED_DROPS: usize = 10;
pub const DROP_HEARTBEAT_SECS: u64 = 7 * 86_400;
pub const DROP_CARD_MAX_BYTES: usize = 65_536;
pub const DROP_WOT_MAX_BYTES: usize = 131_072;

fn default_active_network() -> String {
    "mainnet".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct CustomServerDto {
    pub url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default)]
    pub peer_token: Option<String>,
    #[serde(default = "default_is_active")]
    pub is_active: bool,
}

fn default_is_active() -> bool {
    true
}

impl Default for CustomServerDto {
    fn default() -> Self {
        Self {
            url: String::new(),
            name: String::new(),
            auth_token: None,
            peer_token: None,
            is_active: true,
        }
    }
}

/// Independently pinned regional observation source, configured by the recipient.
#[derive(Serialize,Deserialize,Debug,Clone,Default)]
#[serde(default,rename_all="camelCase")]
pub struct RegionalSourceDto {
    pub url:String,
    pub public_key:String,
    pub bearer_token:Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    // NEU: 0 = Immer fragen, >0 = Session Dauer in Sekunden
    pub session_timeout_seconds: u64,
    pub last_used_directory: Option<String>,
    #[serde(default = "default_privacy_default")]
    pub privacy_default: String,
    // --- Netzwerk / PEX / Seed Felder ---
    #[serde(default = "default_active_network")]
    pub active_network: String,
    #[serde(default)]
    pub bootstrap_seed_url: Option<String>,
    #[serde(default)]
    pub custom_servers: Vec<CustomServerDto>,
    #[serde(default)]
    pub exclusive_custom_servers: bool,
    #[serde(default)]
    pub cached_peers_mainnet: Vec<String>,
    #[serde(default)]
    pub cached_peers_testnet: Vec<String>,
    #[serde(default)]
    pub dev_mode_unlocked: bool,
    pub regional_mesh_sources: Vec<RegionalSourceDto>,
    #[serde(default)]
    pub backup_sync_folder: Option<String>,
    #[serde(default)]
    pub backup_protection_mode: Option<String>,
    #[serde(default)]
    pub last_backup_at: Option<String>,
    #[serde(default)]
    pub enable_pos_mode: bool,
    #[serde(default = "default_balance_view_mode")]
    pub balance_view_mode: String,
    #[serde(default = "default_auto_backup_enabled")]
    pub auto_backup_enabled: bool,
    #[serde(default)]
    pub backup_dirty: bool,
    #[serde(default = "default_market_show_unknown")]
    pub market_show_unknown: bool,
    #[serde(default = "default_max_store_hops")]
    pub max_store_hops: u8,
    #[serde(default)]
    pub market_send_card_on_payment: bool,
    #[serde(default = "default_share_min_level")]
    pub share_min_level: u8,
    #[serde(default)]
    pub market_poll_wifi_only: bool,
    #[serde(default)]
    pub market_selected_drops: Vec<crate::models::FrontendDropDescriptor>,
    #[serde(default)]
    pub public_drops: Vec<StoredPublicDrop>,
    #[serde(default = "default_true")]
    pub wot_share_via_drop: bool,
    #[serde(default = "default_true")]
    pub drop_backup_enabled: bool,
    #[serde(default)]
    pub last_drop_backup_at: Option<String>,
    #[serde(default)]
    pub pending_restore_l2_reconciliation: bool,
    #[serde(default = "default_true")]
    pub post_accept_from_contacts: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredPublicDrop {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
    #[serde(default)]
    pub register_url: Option<String>,
    pub access_pubkey: String, // R_hex64
    #[serde(default = "default_is_active")]
    pub enabled: bool,
    #[serde(default)]
    pub authorized_channels: Vec<String>,
}

fn default_share_min_level() -> u8 {
    1
}

fn default_max_store_hops() -> u8 {
    2
}

fn default_market_show_unknown() -> bool {
    true
}

fn default_auto_backup_enabled() -> bool {
    true
}

fn default_balance_view_mode() -> String {
    "spendable".to_string()
}

fn default_privacy_default() -> String {
    "ask".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            session_timeout_seconds: 600, // Standard: 10 Minuten
            last_used_directory: None,
            privacy_default: "ask".to_string(),
            active_network: default_active_network(),
            bootstrap_seed_url: None,
            custom_servers: Vec::new(),
            exclusive_custom_servers: false,
            cached_peers_mainnet: Vec::new(),
            cached_peers_testnet: Vec::new(),
            dev_mode_unlocked: false,
            regional_mesh_sources:Vec::new(),
            backup_sync_folder: None,
            backup_protection_mode: None,
            last_backup_at: None,
            enable_pos_mode: false,
            balance_view_mode: default_balance_view_mode(),
            auto_backup_enabled: true,
            backup_dirty: false,
            market_show_unknown: true,
            max_store_hops: 2,
            market_send_card_on_payment: false,
            share_min_level: default_share_min_level(),
            market_poll_wifi_only: false,
            market_selected_drops: Vec::new(),
            public_drops: Vec::new(),
            wot_share_via_drop: true,
            drop_backup_enabled: true,
            last_drop_backup_at: None,
            pending_restore_l2_reconciliation: false,
            post_accept_from_contacts: true,
        }
    }
}