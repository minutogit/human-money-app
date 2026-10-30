// src-tauri/src/models/backup.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Portable single-file container for offline export/import.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortableBackupContainer {
    pub container_format: String, // "humoco-app-backup-v1"
    pub created_at: String,       // RFC3339
    pub manifest_hex: String,     // Hex-encoded EncryptedBackupManifest JSON bytes
    pub blobs_hex: HashMap<String, String>, // blob_name -> hex-encoded blob bytes
    #[serde(default)]
    pub custom_standards: HashMap<String, String>, // standard_id -> standard.toml content
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
