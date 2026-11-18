// src-tauri/src/settings.rs
use serde::{Deserialize, Serialize};

pub const SETTINGS_KEY: &str = "app_settings";
pub const MAX_POLLED_SYNCS: usize = 10;
pub const SYNC_HEARTBEAT_SECS: u64 = 7 * 86_400;
pub const SYNC_BASE_MAX_AGE_SECS: u64 = 7 * 86_400;
pub const SYNC_MARKET_POLL_COOLDOWN_SECS: u64 = 1_200;
pub const SYNC_CARD_MAX_BYTES: usize = 524_288;
pub const SYNC_WOT_MAX_BYTES: usize = 131_072;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredAnchorServerMeta {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
    pub admin_pub: String,
    pub lock_pub: String,
    pub nominated_by: String,
    #[serde(default)]
    pub operator_domain: Option<String>,
    #[serde(default)]
    pub asn: Option<String>,
    pub info_fetched_at_secs: u64,
    pub max_retention_quarter: u16,
    pub pow_base_bits: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredAnchorTemplateMeta {
    pub anchor_id: String,
    pub label: String,
    pub threshold: u8,
    pub retain_until_quarter: u16,
    pub created_at_secs: u64,
    pub servers: Vec<StoredAnchorServerMeta>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NominatedLockServerMeta {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
    pub admin_pub: String,
    pub lock_pub: String,
    #[serde(default)]
    pub asn: Option<String>,
    pub probed_at_secs: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub session_timeout_seconds: u64,
    pub last_used_directory: Option<String>,
    #[serde(default = "default_privacy_default")]
    pub privacy_default: String,
    #[serde(default)]
    pub dev_mode_unlocked: bool,
    #[serde(default)]
    pub l2_anchor_templates_meta: Vec<StoredAnchorTemplateMeta>,
    #[serde(default)]
    pub own_nominated_lock_server: Option<NominatedLockServerMeta>,
    #[serde(default)]
    pub l2_probe_cache:
        std::collections::BTreeMap<String, crate::sync_client::FrontendLockServerProbeResult>,
    #[serde(default)]
    pub backup_sync_folder: Option<String>,
    #[serde(default)]
    pub backup_protection_mode: Option<String>,
    #[serde(default)]
    pub last_backup_at: Option<String>,
    #[serde(default)]
    pub enable_pos_mode: bool,
    #[serde(default)]
    pub pos_cashier_active: bool,
    #[serde(default = "default_balance_view_mode")]
    pub balance_view_mode: String,
    #[serde(default = "default_auto_backup_enabled")]
    pub auto_backup_enabled: bool,
    #[serde(default)]
    pub backup_dirty: bool,
    #[serde(default = "default_market_show_unknown")]
    pub market_show_unknown: bool,
    #[serde(default = "default_max_stored_cards")]
    pub max_stored_cards: usize,
    #[serde(default)]
    pub market_send_card_on_payment: bool,
    #[serde(default = "default_true")]
    pub market_sync_enabled: bool,
    #[serde(default = "default_share_min_level")]
    pub share_min_level: u8,
    #[serde(default)]
    pub market_poll_wifi_only: bool,
    #[serde(default)]
    pub market_selected_syncs: Vec<crate::models::FrontendSyncDescriptor>,
    #[serde(default)]
    pub public_syncs: Vec<StoredPublicSync>,
    #[serde(default = "default_true")]
    pub wot_share_via_sync: bool,
    #[serde(default = "default_true")]
    pub sync_backup_enabled: bool,
    #[serde(default)]
    pub last_sync_backup_at: Option<String>,
    #[serde(default)]
    pub pending_restore_l2_reconciliation: bool,
    #[serde(default = "default_true")]
    pub post_accept_from_contacts: bool,
    #[serde(default = "default_ui_mode")]
    pub ui_mode: String,
    #[serde(default = "default_media_auto_load_max_hops")]
    pub media_auto_load_max_hops: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredPublicSync {
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
    #[serde(default = "crate::models::default_sync_status")]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunset_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<crate::models::FrontendSyncReplacementTarget>,
}

impl crate::models::SyncRevocationFields for StoredPublicSync {
    fn status_mut(&mut self) -> &mut String {
        &mut self.status
    }

    fn sunset_at_mut(&mut self) -> &mut Option<u64> {
        &mut self.sunset_at
    }

    fn revocation_reason_mut(&mut self) -> &mut Option<String> {
        &mut self.revocation_reason
    }

    fn replacement_mut(&mut self) -> &mut Option<crate::models::FrontendSyncReplacementTarget> {
        &mut self.replacement
    }
}

impl Default for StoredPublicSync {
    fn default() -> Self {
        Self {
            write_url: String::new(),
            read_url: String::new(),
            drop_pub: String::new(),
            register_url: None,
            access_pubkey: String::new(),
            enabled: default_is_active(),
            authorized_channels: Vec::new(),
            status: crate::models::default_sync_status(),
            sunset_at: None,
            revocation_reason: None,
            replacement: None,
        }
    }
}

fn default_share_min_level() -> u8 {
    1
}

fn default_max_stored_cards() -> usize {
    2000
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

fn default_ui_mode() -> String {
    "simple".to_string()
}

fn default_is_active() -> bool {
    true
}

fn default_media_auto_load_max_hops() -> u8 {
    2
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            session_timeout_seconds: 600, // Standard: 10 Minuten
            last_used_directory: None,
            privacy_default: "ask".to_string(),
            dev_mode_unlocked: false,
            backup_sync_folder: None,
            backup_protection_mode: None,
            last_backup_at: None,
            enable_pos_mode: false,
            pos_cashier_active: false,
            balance_view_mode: default_balance_view_mode(),
            auto_backup_enabled: true,
            backup_dirty: false,
            market_show_unknown: true,
            max_stored_cards: default_max_stored_cards(),
            market_send_card_on_payment: false,
            market_sync_enabled: true,
            share_min_level: default_share_min_level(),
            market_poll_wifi_only: false,
            market_selected_syncs: Vec::new(),
            public_syncs: Vec::new(),
            wot_share_via_sync: true,
            sync_backup_enabled: true,
            last_sync_backup_at: None,
            pending_restore_l2_reconciliation: false,
            post_accept_from_contacts: true,
            ui_mode: default_ui_mode(),
            media_auto_load_max_hops: default_media_auto_load_max_hops(),
            l2_anchor_templates_meta: Vec::new(),
            own_nominated_lock_server: None,
            l2_probe_cache: std::collections::BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_settings_default_max_stored_cards() {
        let settings = AppSettings::default();
        assert_eq!(settings.max_stored_cards, 2000);
    }

    #[test]
    fn test_app_settings_deserialize_default() {
        let json = r#"{}"#;
        let settings: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.max_stored_cards, 2000);
    }

    #[test]
    fn test_app_settings_deserialize_custom_max_stored_cards() {
        let json = r#"{"maxStoredCards": 500}"#;
        let settings: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.max_stored_cards, 500);
    }

    #[test]
    fn test_app_settings_default_and_roundtrip_ui_mode() {
        let settings = AppSettings::default();
        assert_eq!(settings.ui_mode, "simple");

        let deserialized_missing: AppSettings = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(deserialized_missing.ui_mode, "simple");

        let expert = AppSettings {
            ui_mode: "expert".to_string(),
            ..Default::default()
        };
        let serialized = serde_json::to_string(&expert).unwrap();
        assert!(serialized.contains(r#""uiMode":"expert""#));
        let roundtrip: AppSettings = serde_json::from_str(&serialized).unwrap();
        assert_eq!(roundtrip.ui_mode, "expert");
    }

    #[test]
    fn test_app_settings_default_and_roundtrip_media_auto_load_max_hops() {
        let settings = AppSettings::default();
        assert_eq!(settings.media_auto_load_max_hops, 2);

        let deserialized_missing: AppSettings = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(deserialized_missing.media_auto_load_max_hops, 2);

        let custom = AppSettings {
            media_auto_load_max_hops: 1,
            ..Default::default()
        };
        let serialized = serde_json::to_string(&custom).unwrap();
        assert!(serialized.contains(r#""mediaAutoLoadMaxHops":1"#));
        let roundtrip: AppSettings = serde_json::from_str(&serialized).unwrap();
        assert_eq!(roundtrip.media_auto_load_max_hops, 1);
    }
}
