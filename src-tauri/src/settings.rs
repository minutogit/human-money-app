// src-tauri/src/settings.rs
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const UI_PREFERENCES_KEY: &str = "ui_preferences";
pub const SECURITY_POLICY_KEY: &str = "security_policy";
pub const INTERNAL_LIFECYCLE_STATE_KEY: &str = "internal_lifecycle_state";

pub const MAX_POLLED_SYNCS: usize = 10;
pub const SYNC_HEARTBEAT_SECS: u64 = 7 * 86_400;
pub const SYNC_BASE_MAX_AGE_SECS: u64 = 7 * 86_400;
pub const SYNC_MARKET_POLL_COOLDOWN_SECS: u64 = 1_200;
pub const SYNC_CARD_MAX_BYTES: usize = 524_288;
pub const SYNC_WOT_MAX_BYTES: usize = 131_072;

/// Distinguishes an omitted optional JSON key (`None`) from an explicit `null` (`Some(None)`).
fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
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
    #[serde(default = "default_true")]
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
            enabled: true,
            authorized_channels: Vec::new(),
            status: crate::models::default_sync_status(),
            sunset_at: None,
            revocation_reason: None,
            replacement: None,
        }
    }
}

/// UI and market/sync usability preferences editable under `DailyPay`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct UiPreferences {
    #[serde(default = "default_ui_mode")]
    pub ui_mode: String,
    #[serde(default)]
    pub last_used_directory: Option<String>,
    #[serde(default = "default_balance_view_mode")]
    pub balance_view_mode: String,
    #[serde(default = "default_true")]
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
    #[serde(default = "default_true")]
    pub wot_share_via_sync: bool,
    #[serde(default = "default_true")]
    pub post_accept_from_contacts: bool,
    #[serde(default = "default_media_auto_load_max_hops")]
    pub media_auto_load_max_hops: u8,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            ui_mode: default_ui_mode(),
            last_used_directory: None,
            balance_view_mode: default_balance_view_mode(),
            market_show_unknown: true,
            max_stored_cards: default_max_stored_cards(),
            market_send_card_on_payment: false,
            market_sync_enabled: true,
            share_min_level: default_share_min_level(),
            market_poll_wifi_only: false,
            wot_share_via_sync: true,
            post_accept_from_contacts: true,
            media_auto_load_max_hops: default_media_auto_load_max_hops(),
        }
    }
}

/// Security-critical policies and network endpoints editable only under `Full` access.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct SecurityPolicy {
    #[serde(default = "default_session_timeout_seconds")]
    pub session_timeout_seconds: u64,
    #[serde(default = "default_privacy_default")]
    pub privacy_default: String,
    #[serde(default)]
    pub dev_mode_unlocked: bool,
    #[serde(default = "default_true")]
    pub auto_backup_enabled: bool,
    #[serde(default = "default_true")]
    pub sync_backup_enabled: bool,
    #[serde(default)]
    pub backup_sync_folder: Option<String>,
    #[serde(default)]
    pub backup_protection_mode: Option<String>,
    #[serde(default)]
    pub lock_trust_by_admin_pub: BTreeMap<String, bool>,
    #[serde(default)]
    pub market_selected_syncs: Vec<crate::models::FrontendSyncDescriptor>,
    #[serde(default)]
    pub public_syncs: Vec<StoredPublicSync>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            session_timeout_seconds: default_session_timeout_seconds(),
            privacy_default: default_privacy_default(),
            dev_mode_unlocked: false,
            auto_backup_enabled: true,
            sync_backup_enabled: true,
            backup_sync_folder: None,
            backup_protection_mode: None,
            lock_trust_by_admin_pub: BTreeMap::new(),
            market_selected_syncs: Vec::new(),
            public_syncs: Vec::new(),
        }
    }
}

/// Returns whether a sync server identified by `admin_pub_hex` is trusted for Sync-Locks
/// (trusted by default unless explicitly opted out via `false`).
pub fn is_lock_admin_trusted(policy: &SecurityPolicy, admin_pub_hex: &str) -> bool {
    let key = admin_pub_hex.trim().to_ascii_lowercase();
    if key.is_empty() {
        return false;
    }
    policy
        .lock_trust_by_admin_pub
        .get(&key)
        .copied()
        .unwrap_or(true)
}

/// Warning surfaced in the UI when backup sync detects a stale local copy, fork,
/// server rollback, or unexpected backup data loss within channel TTL.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WalletSyncAlert {
    pub kind: String, // "stale_local_backup" | "fork_detected" | "server_rollback" | "unexpected_data_loss"
    pub mirror_write_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_epoch: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_tx_nonce: Option<u64>,
    pub local_epoch: u32,
    pub local_tx_nonce: u64,
    pub detected_at: String,
    #[serde(default)]
    pub has_signed_ack_proof: bool,
}

/// Backend-managed lifecycle flags and L2 probe/template metadata; never writable via patch commands.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct InternalLifecycleState {
    #[serde(default)]
    pub pending_restore_l2_reconciliation: bool,
    #[serde(default)]
    pub backup_dirty: bool,
    #[serde(default)]
    pub last_backup_at: Option<String>,
    #[serde(default)]
    pub last_sync_backup_at: Option<String>,
    #[serde(default)]
    pub l2_probe_cache: BTreeMap<String, crate::sync_client::FrontendLockServerProbeResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallet_sync_alert: Option<WalletSyncAlert>,
}

/// Public subset of `InternalLifecycleState` exposed to the React frontend via `AppConfigDto`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct FrontendLifecycleStateDto {
    #[serde(default)]
    pub pending_restore_l2_reconciliation: bool,
    #[serde(default)]
    pub backup_dirty: bool,
    #[serde(default)]
    pub last_backup_at: Option<String>,
    #[serde(default)]
    pub last_sync_backup_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallet_sync_alert: Option<WalletSyncAlert>,
}

impl From<InternalLifecycleState> for FrontendLifecycleStateDto {
    fn from(value: InternalLifecycleState) -> Self {
        Self {
            pending_restore_l2_reconciliation: value.pending_restore_l2_reconciliation,
            backup_dirty: value.backup_dirty,
            last_backup_at: value.last_backup_at,
            last_sync_backup_at: value.last_sync_backup_at,
            wallet_sync_alert: value.wallet_sync_alert,
        }
    }
}

/// Read-only composite view of all three stores exposed to the UI via `get_app_config`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigDto {
    pub ui_preferences: UiPreferences,
    pub security_policy: SecurityPolicy,
    pub lifecycle: FrontendLifecycleStateDto,
}

/// Partial update for `UiPreferences` (authorized with `RequiredAccess::DailyPay`).
#[derive(Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct UiPreferencesPatch {
    pub ui_mode: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub last_used_directory: Option<Option<String>>,
    pub balance_view_mode: Option<String>,
    pub market_show_unknown: Option<bool>,
    pub max_stored_cards: Option<usize>,
    pub market_send_card_on_payment: Option<bool>,
    pub market_sync_enabled: Option<bool>,
    pub share_min_level: Option<u8>,
    pub market_poll_wifi_only: Option<bool>,
    pub wot_share_via_sync: Option<bool>,
    pub post_accept_from_contacts: Option<bool>,
    pub media_auto_load_max_hops: Option<u8>,
}

impl UiPreferencesPatch {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(Some(ref dir)) = self.last_used_directory {
            let trimmed = dir.trim();
            if !trimmed.is_empty() && trimmed.chars().any(char::is_control) {
                return Err(
                    "Invalid last_used_directory value: contains control characters".to_string(),
                );
            }
        }
        Ok(())
    }

    pub fn apply_to(&self, target: &mut UiPreferences) {
        if let Some(ref mode) = self.ui_mode {
            target.ui_mode = if mode.trim().eq_ignore_ascii_case("expert") {
                "expert".to_string()
            } else {
                "simple".to_string()
            };
        }
        if let Some(ref dir_opt) = self.last_used_directory {
            target.last_used_directory = dir_opt
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
        }
        if let Some(ref view_mode) = self.balance_view_mode {
            target.balance_view_mode = if view_mode.trim().eq_ignore_ascii_case("net") {
                "net".to_string()
            } else {
                "spendable".to_string()
            };
        }
        if let Some(v) = self.market_show_unknown {
            target.market_show_unknown = v;
        }
        if let Some(v) = self.max_stored_cards {
            target.max_stored_cards = v.clamp(100, 10_000);
        }
        if let Some(v) = self.market_send_card_on_payment {
            target.market_send_card_on_payment = v;
        }
        if let Some(v) = self.market_sync_enabled {
            target.market_sync_enabled = v;
        }
        if let Some(v) = self.share_min_level {
            target.share_min_level = v.min(3);
        }
        if let Some(v) = self.market_poll_wifi_only {
            target.market_poll_wifi_only = v;
        }
        if let Some(v) = self.wot_share_via_sync {
            target.wot_share_via_sync = v;
        }
        if let Some(v) = self.post_accept_from_contacts {
            target.post_accept_from_contacts = v;
        }
        if let Some(v) = self.media_auto_load_max_hops {
            target.media_auto_load_max_hops = v.min(3);
        }
    }
}

/// Partial update for `SecurityPolicy` scalar/backup policy fields (`RequiredAccess::Full`).
/// Lock server nominations and sync endpoints use dedicated validated commands.
#[derive(Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SecurityPolicyPatch {
    pub session_timeout_seconds: Option<u64>,
    pub privacy_default: Option<String>,
    pub dev_mode_unlocked: Option<bool>,
    pub auto_backup_enabled: Option<bool>,
    pub sync_backup_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub backup_sync_folder: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub backup_protection_mode: Option<Option<String>>,
}

impl SecurityPolicyPatch {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(ref v) = self.privacy_default {
            let trimmed = v.trim();
            if !matches!(trimmed, "ask" | "stealth" | "public") {
                return Err(format!("Invalid privacy_default value: {}", v));
            }
        }
        if let Some(Some(ref v)) = self.backup_sync_folder {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                if trimmed.chars().any(char::is_control) {
                    return Err(
                        "Invalid backup_sync_folder: contains control characters".to_string()
                    );
                }
                let path = std::path::Path::new(trimmed);
                if !path.is_absolute() {
                    return Err(format!(
                        "Invalid backup_sync_folder (must be absolute): {}",
                        trimmed
                    ));
                }
                if path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(format!(
                        "Invalid backup_sync_folder (parent directory traversal forbidden): {}",
                        trimmed
                    ));
                }
            }
        }
        if let Some(Some(ref v)) = self.backup_protection_mode {
            let trimmed = v.trim();
            if !trimmed.is_empty() && !matches!(trimmed, "mnemonic_only" | "custom_password") {
                return Err(format!("Invalid backup_protection_mode value: {}", v));
            }
        }
        Ok(())
    }

    pub fn apply_to(&self, target: &mut SecurityPolicy) {
        if let Some(v) = self.session_timeout_seconds {
            target.session_timeout_seconds = v;
        }
        if let Some(ref v) = self.privacy_default {
            let trimmed = v.trim();
            if matches!(trimmed, "ask" | "stealth" | "public") {
                target.privacy_default = trimmed.to_string();
            }
        }
        if let Some(v) = self.dev_mode_unlocked {
            target.dev_mode_unlocked = v;
        }
        if let Some(v) = self.auto_backup_enabled {
            target.auto_backup_enabled = v;
        }
        if let Some(v) = self.sync_backup_enabled {
            target.sync_backup_enabled = v;
        }
        if let Some(ref v) = self.backup_sync_folder {
            target.backup_sync_folder = v
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
        }
        if let Some(ref v) = self.backup_protection_mode {
            target.backup_protection_mode = v
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .filter(|s| matches!(*s, "mnemonic_only" | "custom_password"))
                .map(str::to_string);
        }
    }
}

fn default_session_timeout_seconds() -> u64 {
    600
}

fn default_share_min_level() -> u8 {
    1
}

fn default_max_stored_cards() -> usize {
    2000
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

fn default_media_auto_load_max_hops() -> u8 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_preferences_defaults_and_roundtrip() {
        let prefs = UiPreferences::default();
        assert_eq!(prefs.max_stored_cards, 2000);
        assert_eq!(prefs.ui_mode, "simple");
        assert_eq!(prefs.media_auto_load_max_hops, 2);
        assert!(prefs.market_show_unknown);
        assert!(!prefs.market_send_card_on_payment);

        let from_empty: UiPreferences = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(from_empty, prefs);

        let custom: UiPreferences = serde_json::from_str(
            r#"{"maxStoredCards": 500, "uiMode": "expert", "mediaAutoLoadMaxHops": 1}"#,
        )
        .unwrap();
        assert_eq!(custom.max_stored_cards, 500);
        assert_eq!(custom.ui_mode, "expert");
        assert_eq!(custom.media_auto_load_max_hops, 1);

        let serialized = serde_json::to_string(&custom).unwrap();
        let roundtrip: UiPreferences = serde_json::from_str(&serialized).unwrap();
        assert_eq!(roundtrip, custom);
    }

    #[test]
    fn test_security_policy_defaults_and_patch_double_option() {
        let mut policy = SecurityPolicy {
            session_timeout_seconds: 900,
            backup_sync_folder: Some("/tmp/backup".to_string()),
            backup_protection_mode: Some("custom_password".to_string()),
            ..Default::default()
        };

        // Omitted fields must preserve existing values.
        let omitted_patch: SecurityPolicyPatch =
            serde_json::from_str(r#"{"sessionTimeoutSeconds": 300}"#).unwrap();
        assert!(omitted_patch.backup_sync_folder.is_none());
        omitted_patch.apply_to(&mut policy);
        assert_eq!(policy.session_timeout_seconds, 300);
        assert_eq!(policy.backup_sync_folder.as_deref(), Some("/tmp/backup"));

        // Explicit JSON null must clear nullable fields to None.
        let clear_patch: SecurityPolicyPatch =
            serde_json::from_str(r#"{"backupSyncFolder": null, "backupProtectionMode": null}"#)
                .unwrap();
        assert_eq!(clear_patch.backup_sync_folder, Some(None));
        assert_eq!(clear_patch.backup_protection_mode, Some(None));
        clear_patch.apply_to(&mut policy);
        assert_eq!(policy.backup_sync_folder, None);
        assert_eq!(policy.backup_protection_mode, None);
    }

    #[test]
    fn test_ui_preferences_patch_clamps_values() {
        let mut prefs = UiPreferences::default();
        let patch: UiPreferencesPatch = serde_json::from_str(
            r#"{"uiMode": "EXPERT", "shareMinLevel": 9, "mediaAutoLoadMaxHops": 7, "lastUsedDirectory": "/tmp/exports"}"#,
        )
        .unwrap();
        patch.apply_to(&mut prefs);
        assert_eq!(prefs.ui_mode, "expert");
        assert_eq!(prefs.share_min_level, 3);
        assert_eq!(prefs.media_auto_load_max_hops, 3);
        assert_eq!(prefs.last_used_directory.as_deref(), Some("/tmp/exports"));
    }

    #[test]
    fn s11_f1_f3_patch_deny_unknown_fields_and_validation() {
        let ui_unknown_err =
            serde_json::from_str::<UiPreferencesPatch>(r#"{"autoBackupEnabled":true}"#)
                .unwrap_err()
                .to_string();
        assert!(
            ui_unknown_err.contains("unknown field"),
            "expected deny_unknown_fields error, got: {ui_unknown_err}"
        );
        let pol_unknown_err =
            serde_json::from_str::<SecurityPolicyPatch>(r#"{"uiMode":"expert"}"#)
                .unwrap_err()
                .to_string();
        assert!(
            pol_unknown_err.contains("unknown field"),
            "expected deny_unknown_fields error, got: {pol_unknown_err}"
        );

        let mut prefs = UiPreferences::default();
        let clamp_low: UiPreferencesPatch =
            serde_json::from_str(r#"{"maxStoredCards":0}"#).unwrap();
        clamp_low.apply_to(&mut prefs);
        assert_eq!(prefs.max_stored_cards, 100);
        let clamp_high: UiPreferencesPatch =
            serde_json::from_str(r#"{"maxStoredCards":999999}"#).unwrap();
        clamp_high.apply_to(&mut prefs);
        assert_eq!(prefs.max_stored_cards, 10_000);

        let bad_dir_patch = UiPreferencesPatch {
            last_used_directory: Some(Some("/tmp/bad\ndir".to_string())),
            ..Default::default()
        };
        assert_eq!(
            bad_dir_patch.validate().unwrap_err(),
            "Invalid last_used_directory value: contains control characters"
        );

        for (bad_folder, expected_substr) in [
            ("relative/path", "must be absolute"),
            ("../escape", "must be absolute"),
            ("/tmp/../escape", "parent directory traversal forbidden"),
            ("/tmp/bad\0folder", "contains control characters"),
        ] {
            let p = SecurityPolicyPatch {
                backup_sync_folder: Some(Some(bad_folder.to_string())),
                ..Default::default()
            };
            let err = p.validate().unwrap_err();
            assert!(
                err.contains(expected_substr),
                "expected '{expected_substr}' for '{bad_folder}', got: {err}"
            );
        }

        let valid_folder = SecurityPolicyPatch {
            backup_sync_folder: Some(Some("/tmp/valid-backups".to_string())),
            ..Default::default()
        };
        assert_eq!(valid_folder.validate(), Ok(()));
    }

    #[test]
    fn test_lock_admin_trust_lifecycle_dto_and_patch_validation_apply_branches() {
        let mut policy = SecurityPolicy {
            lock_trust_by_admin_pub: BTreeMap::from([
                ("aabb".to_string(), false),
                ("ccdd".to_string(), true),
            ]),
            ..Default::default()
        };
        assert_eq!(policy.session_timeout_seconds, 600);
        assert_eq!(policy.privacy_default, "ask");
        assert!(policy.auto_backup_enabled);
        assert!(policy.sync_backup_enabled);

        assert!(!is_lock_admin_trusted(&policy, ""));
        assert!(!is_lock_admin_trusted(&policy, "   "));
        assert!(!is_lock_admin_trusted(&policy, "  AABB "));
        assert!(is_lock_admin_trusted(&policy, "CCDD"));
        assert!(is_lock_admin_trusted(&policy, "eeff"));

        let alert = WalletSyncAlert {
            kind: "fork_detected".to_string(),
            mirror_write_url: "https://sync.example.org/sync.php".to_string(),
            remote_epoch: Some(2),
            remote_tx_nonce: Some(9),
            local_epoch: 1,
            local_tx_nonce: 4,
            detected_at: "2026-11-25T10:00:00Z".to_string(),
            has_signed_ack_proof: true,
        };
        let lifecycle = InternalLifecycleState {
            pending_restore_l2_reconciliation: true,
            backup_dirty: true,
            last_backup_at: Some("2026-11-25T09:00:00Z".to_string()),
            last_sync_backup_at: Some("2026-11-25T09:30:00Z".to_string()),
            l2_probe_cache: BTreeMap::new(),
            wallet_sync_alert: Some(alert.clone()),
        };
        let dto = FrontendLifecycleStateDto::from(lifecycle);
        assert!(dto.pending_restore_l2_reconciliation);
        assert!(dto.backup_dirty);
        assert_eq!(dto.last_backup_at.as_deref(), Some("2026-11-25T09:00:00Z"));
        assert_eq!(
            dto.last_sync_backup_at.as_deref(),
            Some("2026-11-25T09:30:00Z")
        );
        assert_eq!(dto.wallet_sync_alert, Some(alert));

        let mut prefs = UiPreferences::default();
        assert_eq!(prefs.share_min_level, 1);
        assert_eq!(prefs.balance_view_mode, "spendable");

        let valid_dir_patch = UiPreferencesPatch {
            ui_mode: Some("unexpected".to_string()),
            last_used_directory: Some(Some("  /tmp/ok  ".to_string())),
            balance_view_mode: Some("NET".to_string()),
            market_show_unknown: Some(false),
            market_send_card_on_payment: Some(true),
            market_sync_enabled: Some(false),
            market_poll_wifi_only: Some(true),
            wot_share_via_sync: Some(false),
            post_accept_from_contacts: Some(false),
            ..Default::default()
        };
        assert_eq!(valid_dir_patch.validate(), Ok(()));
        valid_dir_patch.apply_to(&mut prefs);
        assert_eq!(prefs.ui_mode, "simple");
        assert_eq!(prefs.last_used_directory.as_deref(), Some("/tmp/ok"));
        assert_eq!(prefs.balance_view_mode, "net");
        assert!(!prefs.market_show_unknown);
        assert!(prefs.market_send_card_on_payment);
        assert!(!prefs.market_sync_enabled);
        assert!(prefs.market_poll_wifi_only);
        assert!(!prefs.wot_share_via_sync);
        assert!(!prefs.post_accept_from_contacts);

        let blank_dir_patch = UiPreferencesPatch {
            last_used_directory: Some(Some("   ".to_string())),
            balance_view_mode: Some("other".to_string()),
            ..Default::default()
        };
        assert_eq!(blank_dir_patch.validate(), Ok(()));
        blank_dir_patch.apply_to(&mut prefs);
        assert_eq!(prefs.last_used_directory, None);
        assert_eq!(prefs.balance_view_mode, "spendable");

        for privacy in ["ask", "stealth", "public"] {
            let p = SecurityPolicyPatch {
                privacy_default: Some(format!(" {privacy} ")),
                ..Default::default()
            };
            assert_eq!(p.validate(), Ok(()));
            p.apply_to(&mut policy);
            assert_eq!(policy.privacy_default, privacy);
        }
        assert_eq!(
            SecurityPolicyPatch {
                privacy_default: Some("invalid".to_string()),
                ..Default::default()
            }
            .validate()
            .unwrap_err(),
            "Invalid privacy_default value: invalid"
        );
        assert_eq!(
            SecurityPolicyPatch {
                backup_protection_mode: Some(Some("bogus".to_string())),
                ..Default::default()
            }
            .validate()
            .unwrap_err(),
            "Invalid backup_protection_mode value: bogus"
        );

        let apply_pol = SecurityPolicyPatch {
            dev_mode_unlocked: Some(true),
            auto_backup_enabled: Some(false),
            sync_backup_enabled: Some(false),
            backup_sync_folder: Some(Some("  /var/backups  ".to_string())),
            backup_protection_mode: Some(Some(" mnemonic_only ".to_string())),
            ..Default::default()
        };
        assert_eq!(apply_pol.validate(), Ok(()));
        apply_pol.apply_to(&mut policy);
        assert!(policy.dev_mode_unlocked);
        assert!(!policy.auto_backup_enabled);
        assert!(!policy.sync_backup_enabled);
        assert_eq!(policy.backup_sync_folder.as_deref(), Some("/var/backups"));
        assert_eq!(
            policy.backup_protection_mode.as_deref(),
            Some("mnemonic_only")
        );

        let blank_pol = SecurityPolicyPatch {
            backup_sync_folder: Some(Some("   ".to_string())),
            backup_protection_mode: Some(Some("   ".to_string())),
            ..Default::default()
        };
        assert_eq!(blank_pol.validate(), Ok(()));
        blank_pol.apply_to(&mut policy);
        assert_eq!(policy.backup_sync_folder, None);
        assert_eq!(policy.backup_protection_mode, None);
    }
}
