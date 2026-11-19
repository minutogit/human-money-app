// src-tauri/src/models/backup.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Canonical IPC backup encryption mode selected by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupProtectionMode {
    MnemonicOnly,
    CustomPassword,
}

impl BackupProtectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MnemonicOnly => "mnemonic_only",
            Self::CustomPassword => "custom_password",
        }
    }
}

/// Portable single-file container for offline export/import.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortableBackupContainer {
    pub container_format: String,           // "humoco-app-backup-v1"
    pub created_at: String,                 // RFC3339
    pub manifest_hex: String,               // Hex-encoded EncryptedBackupManifest JSON bytes
    pub blobs_hex: HashMap<String, String>, // blob_name -> hex-encoded blob bytes
    #[serde(default)]
    pub custom_standards: HashMap<String, String>, // standard_id -> standard.toml content
}

/// Differential delta container for sync-server backup (`<pfx>-delta` slot).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackupDeltaContainer {
    pub container_format: String, // "humoco-app-backup-delta-v1"
    pub created_at: String,       // RFC3339
    pub base_manifest_sha256: String,
    pub manifest_hex: String,
    pub put_blobs_hex: HashMap<String, String>,
    #[serde(default)]
    pub delete_blobs: Vec<String>,
    #[serde(default)]
    pub custom_standards: HashMap<String, String>,
}

/// Information returned when probing an encrypted backup source without credentials.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendBackupProbeDto {
    pub source_kind: String, // "file" | "folder"
    pub protection_mode: String,
    pub supports_password_restore: bool,
    pub supports_mnemonic_restore: bool,
    pub blob_count: usize,
    pub custom_standard_count: usize,
}

/// Metadata describing an already existing local profile that shares the same `user_id`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalProfileCollisionDto {
    pub folder_name: String,
    pub profile_name: String,
    pub user_id: String,
    pub local_epoch: Option<u64>,
    pub local_tx_nonce: Option<u64>,
    pub is_locked_or_conflicted: bool,
}

/// Canonical resolved sync service target discovered from a sync URL/descriptor.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSyncTargetDto {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
    pub register_url: Option<String>,
}

/// Preview of the cryptographic identity derived from mnemonic + optional passphrase/prefix.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MnemonicIdentityPreviewDto {
    pub user_id: String,
    pub local_collision: Option<LocalProfileCollisionDto>,
}

/// Detailed metadata obtained after decrypting the manifest with credentials.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendBackupInspectionDto {
    pub profile_name: String,
    pub user_id: String,
    pub epoch: u64,
    pub tx_nonce: u64,
    pub seal_timestamp: String,
    pub blob_count: usize,
    pub custom_standard_count: usize,
    pub protection_mode: String,
    pub local_collision: Option<LocalProfileCollisionDto>,
}

/// Statistics and metadata from an export or folder synchronization run.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendBackupSummaryDto {
    pub total_blobs: usize,
    pub changed_blobs: usize,
    pub deleted_blobs: usize,
    pub upload_bytes: usize,
    pub timestamp: String,
    pub destination_path: String,
    pub custom_standard_count: usize,
}

/// Outcome of a staged restore, including local state and post-restore L2 reconciliation.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendRestoreOutcomeDto {
    pub folder_name: String,
    pub profile_name: String,
    pub active_voucher_queries_count: usize,
    pub reconciled_spent_count: usize,
    pub restored_standards_count: usize,
    pub l2_reconciliation_status: String, // "no_servers" | "offline" | "reconciled"
}

/// Status and metadata for automated background backup and synchronization.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAutoBackupStatusDto {
    pub status: String, // "ok" | "pending" | "warning" | "disconnected" | "notConfigured"
    pub last_backup_at: Option<String>,
    pub backup_dirty: bool,
    pub sync_folder: Option<String>,
    pub changed_blobs: Option<usize>,
    pub sync_backup_enabled: bool,
    pub last_sync_backup_at: Option<String>,
    pub has_authorized_sync_backup_target: bool,
}

/// Result of an event-driven or manual sync backup run.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendBackupSyncResult {
    pub uploaded_to: usize,
    pub authorized_targets: usize,
    pub padded_bytes: usize,
    pub timestamp: Option<String>,
    pub errors: Vec<String>,
    pub retry_after_secs: Option<u64>,
    #[serde(default)]
    pub skipped_wifi_only: bool,
}

/// Non-silent probe result when searching sync services for an encrypted backup.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncBackupProbeResultDto {
    pub status: String, // "found" | "not_found" | "unreachable"
    pub staged_path: Option<String>,
    pub matched_key_kind: Option<String>, // "stealth" | "public" | "grace" | "slot_id" | "own"
    pub source_url: Option<String>,
    pub inspection: Option<FrontendBackupInspectionDto>,
    pub user_id: String,
    pub local_collision: Option<LocalProfileCollisionDto>,
    pub resolved_sync_targets: Vec<ResolvedSyncTargetDto>,
    pub error: Option<String>,
}

/// Outcome of reconciling a restored wallet against online L2 gateways.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendRestoreReconcileResultDto {
    pub active_voucher_queries_count: usize,
    pub reconciled_spent_count: usize,
    pub l2_reconciliation_status: String, // "no_servers" | "offline" | "reconciled"
    pub pending_restore_l2_reconciliation: bool,
}
