use serde::{Deserialize, Serialize};

use crate::models::{FrontendBackupSyncResult, FrontendPostSyncResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum GuardianAssignmentStatus {
    Pending,
    Confirmed { acked_at: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardianAssignmentRecord {
    pub user_id: String,
    pub display_name: String,
    pub share_index: u8,
    pub status: GuardianAssignmentStatus,
    pub assigned_at: u64,
    pub sealed_entry: humoco_market::GuardianVaultEntry,
}

fn default_rehearsal_m_kib() -> u32 {
    humoco_market::KDF_M_KIB_PRODUCTION
}

fn default_rehearsal_t_cost() -> u32 {
    humoco_market::KDF_T_COST_PRODUCTION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryOwnerState {
    pub rotation_epoch: u32,
    pub threshold_m: u8,
    pub question_ids: [u8; 3],
    pub guardians: Vec<GuardianAssignmentRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emergency_sheet_confirmed_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_verifiers: Option<[[u8; 32]; 3]>,
    #[serde(default)]
    pub rehearsal_stage: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_rehearsal_at: Option<u64>,
    #[serde(default = "default_rehearsal_m_kib")]
    pub rehearsal_kdf_m_kib: u32,
    #[serde(default = "default_rehearsal_t_cost")]
    pub rehearsal_kdf_t_cost: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedMnemonicKitDto {
    pub phrase: String,
    pub language_code: u8,
    pub has_bip39_passphrase: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_prefix: Option<String>,
    pub kit_uri: String,
    pub qr_svg: String,
    pub sync_urls: Vec<String>,
    pub guardian_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmEmergencySheetResultDto {
    pub confirmed_at: u64,
    pub backup_sync_result: Option<FrontendBackupSyncResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardianStatusDto {
    pub user_id: String,
    pub display_name: String,
    pub share_index: u8,
    pub status: GuardianAssignmentStatus,
    pub assigned_at: u64,
    pub last_contacted_at: Option<u64>,
    pub is_stale_90d: bool,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardianWardDto {
    pub owner_did: String,
    pub owner_display_name: String,
    pub rotation_epoch: u32,
    pub threshold_m: u8,
    pub total_n: u8,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryProtectionStatusDto {
    pub has_voucher: bool,
    pub confirmed_guardians_count: usize,
    pub required_threshold: u8,
    pub guardians: Vec<GuardianStatusDto>,
    pub emergency_sheet_confirmed_at: Option<u64>,
    pub has_identity_recovery: bool,
    pub sync_backup_count: usize,
    pub has_sync_backup: bool,
    pub has_local_backup: bool,
    pub has_voucher_backup: bool,
    pub guardian_wards: Vec<GuardianWardDto>,
    pub rehearsal_due: bool,
    pub question_ids: Option<[u8; 3]>,
    pub rehearsal_stage: u8,
    pub next_rehearsal_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RehearsalCheckResultDto {
    pub all_correct: bool,
    pub question_matches: [bool; 3],
    pub rehearsal_stage: u8,
    pub next_rehearsal_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupSocialRecoveryResultDto {
    pub rotation_epoch: u32,
    pub threshold_m: u8,
    pub guardians_count: usize,
    pub post_sync_result: FrontendPostSyncResult,
    pub backup_sync_result: Option<FrontendBackupSyncResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeSocialRecoveryResultDto {
    pub rotation_epoch: u32,
    pub revoked_guardians_count: usize,
    pub post_sync_result: FrontendPostSyncResult,
    pub backup_sync_result: Option<FrontendBackupSyncResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRecoverySessionDto {
    pub sas_code: String,
    pub created_at: u64,
    pub name_hint: Option<String>,
    pub request_uri: String,
    pub session_mode: String,
    pub target_folder_name: Option<String>,
    pub owner_did: Option<String>,
    pub owner_display_name: Option<String>,
    pub rotation_epoch: Option<u32>,
    pub threshold_m: Option<u8>,
    #[serde(default)]
    pub total_n: Option<u8>,
    pub collected_count: usize,
    #[serde(default)]
    pub collected_share_indices: Vec<u8>,
    pub question_ids: Option<[u8; 3]>,
    pub has_enough_shares: bool,
    pub discovered_sync_urls: Vec<String>,
    #[serde(default)]
    pub last_import_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRecoverySessionFile {
    pub sk_temp: [u8; 32],
    pub pk_temp: [u8; 32],
    pub sas_code: String,
    pub created_at: u64,
    pub name_hint: Option<String>,
    pub request_uri: String,
    pub session_mode: String,
    pub target_folder_name: Option<String>,
    pub replies: Vec<humoco_market::GuardianReleasePayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardianRecoveryRequestInspectionDto {
    pub sas_code: String,
    pub requested_at: u64,
    pub is_stale_48h: bool,
    pub name_hint: Option<String>,
    pub wards: Vec<GuardianWardDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveGuardianRecoveryResultDto {
    pub reply_uri: String,
    pub reply_qr_svg: String,
    pub sas_code: String,
    pub owner_display_name: String,
}

/// Decoded emergency kit URI payload for pre-filling the unified words/sheet UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectedEmergencyKitDto {
    pub phrase: String,
    pub word_count: usize,
    pub language: crate::models::MnemonicLanguage,
    pub has_bip39_passphrase: bool,
    pub user_prefix: Option<String>,
    pub sync_urls: Vec<String>,
    pub sync_descriptors_json: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCompletionResultDto {
    pub folder_name: String,
    pub profile_name: String,
    pub mode: String,
    pub outcome: String,
    pub backup_epoch: Option<u64>,
    pub backup_tx_nonce: Option<u64>,
    pub active_voucher_queries_count: usize,
    pub reconciled_spent_count: usize,
    pub restored_standards_count: usize,
    pub l2_reconciliation_status: String,
    pub pending_restore_l2_reconciliation: bool,
    pub local_collision: Option<crate::models::LocalProfileCollisionDto>,
    pub resolved_sync_targets: Vec<crate::models::ResolvedSyncTargetDto>,
}
