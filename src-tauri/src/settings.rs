// src-tauri/src/settings.rs
use serde::{Deserialize, Serialize};

pub const SETTINGS_KEY: &str = "app_settings";

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
    pub bundle_retention_days: u64,
    // NEU: 0 = Immer fragen, >0 = Session Dauer in Sekunden
    pub session_timeout_seconds: u64,
    pub last_used_directory: Option<String>,
    #[serde(default = "default_privacy_default")]
    pub privacy_default: String,
    #[serde(default)]
    pub l2_node_urls: Option<Vec<String>>,
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
}

fn default_privacy_default() -> String {
    "ask".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            bundle_retention_days: 30,
            session_timeout_seconds: 600, // Standard: 10 Minuten
            last_used_directory: None,
            privacy_default: "ask".to_string(),
            l2_node_urls: None,
            active_network: default_active_network(),
            bootstrap_seed_url: None,
            custom_servers: Vec::new(),
            exclusive_custom_servers: false,
            cached_peers_mainnet: Vec::new(),
            cached_peers_testnet: Vec::new(),
            dev_mode_unlocked: false,
            regional_mesh_sources:Vec::new(),
        }
    }
}