// src-tauri/src/state.rs
use crate::models::{FrontendAddressBook, FrontendContact, TransactionRecord};
use crate::session::{ActiveProfileCaches, SessionToken};
use crate::settings::{
    AppConfigDto, InternalLifecycleState, SecurityPolicy, UiPreferences,
    INTERNAL_LIFECYCLE_STATE_KEY, SECURITY_POLICY_KEY, UI_PREFERENCES_KEY,
};
use chrono::{Duration, Utc};
use human_money_core::app_service::{AppFacadeError, AppService};
use log::{error, info};
use std::sync::{Mutex, MutexGuard};

pub const TRANSACTION_HISTORY_KEY: &str = "transaction_history";
pub const OUTGOING_BUNDLES_KEY: &str = "outgoing_bundles";
pub const CONTACTS_DATA_NAME: &str = "address_book";
pub const WOT_DATA_NAME: &str = "wot_register";
pub const MARKET_CARD_STORE_KEY: &str = "market_card_store";
pub const MARKET_OWN_PROFILE_KEY: &str = "market_own_profile";
pub const MARKET_SYNC_ADMIN_KEY: &str = "market_sync_admin";
pub const MARKET_SYNC_KEY: &str = "market_sync_state";
pub const BACKUP_SYNC_STATE_KEY: &str = "backup_sync_state";
pub const KNOWN_LOCK_HEADS_KEY: &str = "known_lock_heads";
pub const MARKET_POST_STATE_KEY: &str = "market_post_state";
pub const APP_NOTICES_KEY: &str = "app_notices";
pub const MAX_APP_NOTICES: usize = 50;
pub const RECOVERY_OWNER_STATE_KEY: &str = "recovery_owner_state";
pub const RECOVERY_GUARDIAN_VAULT_KEY: &str = "recovery_guardian_vault";
pub const BUNDLE_RETENTION_DAYS: i64 = 90;
pub const ONE_SHOT_PIN_GRANT_PREFIX: &str = "hmc_grant_";
const ONE_SHOT_PIN_GRANT_TTL: std::time::Duration = std::time::Duration::from_secs(15);

pub fn own_media_storage_key(image_token: &str) -> String {
    format!("market_own_media_{}", image_token)
}

pub fn cached_media_storage_key(blob_id: &str) -> String {
    format!("market_media_cache_{}", blob_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredAccess {
    Unlocked,
    DailyPay,
    Full,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OutboxMessageClass {
    ValueTransfer,
    ClaimBearing,
    Control,
}

impl OutboxMessageClass {
    pub fn requires_outflow_guard(self) -> bool {
        matches!(self, Self::ValueTransfer | Self::ClaimBearing)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PostOutboxEntry {
    pub msg_id: String,
    pub recipient_did: String,
    pub tag16_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    pub message_class: OutboxMessageClass,
    pub entry: humoco_market::PostBoxEntry,
    pub created_at: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StoredPrecommitL2Evidence {
    pub quorum_certificates: Vec<(
        String,
        Vec<human_money_core::humoco_proto::types::QuorumCertificate>,
    )>,
    pub competing_entries: Vec<(human_money_core::models::layer2_api::LockEntry, u32)>,
    pub is_conflict: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StoredPendingTolerancePayment {
    pub msg_id: String,
    pub sender_did: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub created_at: u64,
    pub zone: String,
    pub error_message: String,
    pub tag16_hex: String,
    pub container_b64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l2_evidence: Option<StoredPrecommitL2Evidence>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredPostState {
    #[serde(default)]
    pub outbox_entries: Vec<PostOutboxEntry>,
    #[serde(default)]
    pub slot_etags_by_write_url: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub processed_msg_ids: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub pending_sign_requests: Vec<crate::models::FrontendPendingSignRequest>,
    #[serde(default)]
    pub pending_wot_handshakes: Vec<crate::models::FrontendPendingWotHandshake>,
    #[serde(default)]
    pub pending_wot_packages_by_msg_id: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub inquiries: Vec<crate::models::FrontendMarketInquiryItem>,
    #[serde(default)]
    pub unsupported_messages: Vec<crate::models::FrontendUnsupportedPostItem>,
    #[serde(default)]
    pub pending_tolerance_payments: Vec<StoredPendingTolerancePayment>,
    #[serde(default)]
    pub inquiry_rate_tracker: humoco_market::InquiryRateTracker,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StoredSyncAdmin {
    pub descriptor: humoco_market::DropDescriptor,
    #[serde(default)]
    pub sync_index: u32,
    pub data_dir: String,
    pub max_members: usize,
    pub max_total_storage_mb: usize,
    pub max_user_storage_mb: usize,
    #[serde(default)]
    pub register_url: Option<String>,
    pub policies: std::collections::BTreeMap<String, humoco_market::MemberPolicy>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub blocked_pubkeys: Vec<String>,
    pub last_members_hash: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<crate::sync_client::SyncRevocationNotice>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredSyncState {
    pub last_upload_at: Option<u64>,
    #[serde(default)]
    pub last_poll_at: Option<u64>,
    pub last_readers_hash: Option<String>,
    #[serde(default)]
    pub etags_by_read_url: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub last_modified_by_read_url: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub slot_etags: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub channel_max_bytes_by_write_url: std::collections::BTreeMap<String, usize>,
    #[serde(default)]
    pub card_base_uploaded_at_by_write_url: std::collections::BTreeMap<String, u64>,
    #[serde(default)]
    pub card_base_size_by_write_url: std::collections::BTreeMap<String, usize>,
    #[serde(default)]
    pub card_base_versions_by_write_url:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, u64>>,
    #[serde(default)]
    pub card_delta_active_by_write_url: std::collections::BTreeMap<String, bool>,
    #[serde(default)]
    pub card_last_uploaded_versions_by_write_url:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, u64>>,
    #[serde(default)]
    pub last_revocation_check_at: std::collections::BTreeMap<String, u64>,
    #[serde(default)]
    pub sync_sources_by_did: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub uploaded_media_blobs: std::collections::HashSet<String>,
    pub wot_tombstone_fp_by_write_url: std::collections::BTreeMap<String, String>,
}

pub(crate) fn merge_btree_map_3way<K: Ord + Clone, V: Eq + Clone>(
    baseline: &std::collections::BTreeMap<K, V>,
    working: &mut std::collections::BTreeMap<K, V>,
    latest: &std::collections::BTreeMap<K, V>,
) {
    let all_keys: std::collections::BTreeSet<K> = baseline
        .keys()
        .chain(working.keys())
        .chain(latest.keys())
        .cloned()
        .collect();
    for key in all_keys {
        let b = baseline.get(&key);
        let w = working.get(&key);
        if w == b {
            match latest.get(&key) {
                Some(v) => {
                    working.insert(key, v.clone());
                }
                None => {
                    working.remove(&key);
                }
            }
        }
    }
}

pub(crate) fn merge_sync_state_3way(
    baseline: &StoredSyncState,
    working: &mut StoredSyncState,
    latest: &StoredSyncState,
) {
    working.last_upload_at = match (working.last_upload_at, latest.last_upload_at) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    working.last_poll_at = match (working.last_poll_at, latest.last_poll_at) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    if working.last_readers_hash == baseline.last_readers_hash {
        working.last_readers_hash = latest.last_readers_hash.clone();
    }
    merge_btree_map_3way(
        &baseline.etags_by_read_url,
        &mut working.etags_by_read_url,
        &latest.etags_by_read_url,
    );
    merge_btree_map_3way(
        &baseline.last_modified_by_read_url,
        &mut working.last_modified_by_read_url,
        &latest.last_modified_by_read_url,
    );
    merge_btree_map_3way(
        &baseline.slot_etags,
        &mut working.slot_etags,
        &latest.slot_etags,
    );
    merge_btree_map_3way(
        &baseline.channel_max_bytes_by_write_url,
        &mut working.channel_max_bytes_by_write_url,
        &latest.channel_max_bytes_by_write_url,
    );
    merge_btree_map_3way(
        &baseline.card_base_uploaded_at_by_write_url,
        &mut working.card_base_uploaded_at_by_write_url,
        &latest.card_base_uploaded_at_by_write_url,
    );
    merge_btree_map_3way(
        &baseline.card_base_size_by_write_url,
        &mut working.card_base_size_by_write_url,
        &latest.card_base_size_by_write_url,
    );
    merge_btree_map_3way(
        &baseline.card_base_versions_by_write_url,
        &mut working.card_base_versions_by_write_url,
        &latest.card_base_versions_by_write_url,
    );
    merge_btree_map_3way(
        &baseline.card_delta_active_by_write_url,
        &mut working.card_delta_active_by_write_url,
        &latest.card_delta_active_by_write_url,
    );
    merge_btree_map_3way(
        &baseline.card_last_uploaded_versions_by_write_url,
        &mut working.card_last_uploaded_versions_by_write_url,
        &latest.card_last_uploaded_versions_by_write_url,
    );
    merge_btree_map_3way(
        &baseline.last_revocation_check_at,
        &mut working.last_revocation_check_at,
        &latest.last_revocation_check_at,
    );
    merge_btree_map_3way(
        &baseline.sync_sources_by_did,
        &mut working.sync_sources_by_did,
        &latest.sync_sources_by_did,
    );
    merge_btree_map_3way(
        &baseline.wot_tombstone_fp_by_write_url,
        &mut working.wot_tombstone_fp_by_write_url,
        &latest.wot_tombstone_fp_by_write_url,
    );
    let all_blobs: std::collections::HashSet<String> = baseline
        .uploaded_media_blobs
        .iter()
        .chain(working.uploaded_media_blobs.iter())
        .chain(latest.uploaded_media_blobs.iter())
        .cloned()
        .collect();
    for blob in all_blobs {
        let in_b = baseline.uploaded_media_blobs.contains(&blob);
        let in_w = working.uploaded_media_blobs.contains(&blob);
        if in_w == in_b {
            if latest.uploaded_media_blobs.contains(&blob) {
                working.uploaded_media_blobs.insert(blob);
            } else {
                working.uploaded_media_blobs.remove(&blob);
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupMirrorState {
    pub base_manifest_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_slot_etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_slot_etag: Option<String>,
    pub base_uploaded_at: u64,
    pub base_padded_bytes: usize,
    pub delta_active: bool,
    #[serde(default)]
    pub last_synced_epoch: u32,
    #[serde(default)]
    pub last_synced_tx_nonce: u64,
    #[serde(default)]
    pub last_synced_seal_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_slot_ack: Option<crate::sync_client::SlotAckReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_known_ttl_days: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anomaly: Option<crate::settings::WalletSyncAlert>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredBackupSyncState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_manifest_bytes: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_manifest_sha256: Option<String>,
    #[serde(default)]
    pub mirrors_by_write_url: std::collections::BTreeMap<String, BackupMirrorState>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownLockHeadsState {
    #[serde(default)]
    pub heads:
        std::collections::BTreeMap<String, human_money_core::models::layer2_api::LockReceipt>,
}

/// OS-level exclusive session lock for a single profile folder (`<session_locks>/<folder_name>.lock`).
///
/// Released automatically on `Drop` (logout, failed login rollback, or process termination).
pub struct ProfileSessionLock {
    folder_name: String,
    _file: std::fs::File,
}

/// Result of attempting to acquire a profile session lock prior to login/recovery.
pub enum PendingProfileLock {
    None,
    AlreadyHeld,
    Acquired(ProfileSessionLock),
}

fn validate_lock_folder_name(folder_name: &str) -> Result<(), crate::models::FrontendError> {
    if folder_name.is_empty() || folder_name.contains(['/', '\\']) {
        return Err(crate::models::FrontendError::from(
            "Invalid profile folder name".to_string(),
        ));
    }
    let mut components = std::path::Path::new(folder_name).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(_)), None) => Ok(()),
        _ => Err(crate::models::FrontendError::from(
            "Invalid profile folder name".to_string(),
        )),
    }
}

impl ProfileSessionLock {
    fn try_acquire(
        lock_dir: &std::path::Path,
        folder_name: &str,
    ) -> Result<Self, crate::models::FrontendError> {
        validate_lock_folder_name(folder_name)?;
        std::fs::create_dir_all(lock_dir).map_err(|e| {
            crate::models::FrontendError::from(format!(
                "Failed to create session lock directory: {}",
                e
            ))
        })?;
        let lock_path = lock_dir.join(format!("{folder_name}.lock"));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| {
                crate::models::FrontendError::from(format!(
                    "Failed to open profile session lock file: {}",
                    e
                ))
            })?;
        match file.try_lock() {
            Ok(()) => Ok(Self {
                folder_name: folder_name.to_string(),
                _file: file,
            }),
            Err(std::fs::TryLockError::WouldBlock) => Err(crate::models::FrontendError::with_code(
                "error.auth.profileAlreadyOpen",
                "This profile is already open in another app instance.",
            )),
            Err(std::fs::TryLockError::Error(e)) => Err(crate::models::FrontendError::from(
                format!("Failed to acquire profile session lock: {}", e),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncWakeRequest {
    #[serde(default)]
    pub seal: bool,
    #[serde(default)]
    pub local_backup: bool,
    #[serde(default)]
    pub sync_backup: bool,
    #[serde(default)]
    pub sync_backup_force: bool,
    #[serde(default)]
    pub post: bool,
    #[serde(default)]
    pub market: bool,
    #[serde(default)]
    pub market_force_upload: bool,
}

impl SyncWakeRequest {
    pub fn merge(&mut self, other: Self) {
        self.seal |= other.seal;
        self.local_backup |= other.local_backup;
        self.sync_backup |= other.sync_backup;
        self.sync_backup_force |= other.sync_backup_force;
        self.post |= other.post;
        self.market |= other.market;
        self.market_force_upload |= other.market_force_upload;
    }

    pub fn is_empty(&self) -> bool {
        !(self.seal
            || self.local_backup
            || self.sync_backup
            || self.sync_backup_force
            || self.post
            || self.market
            || self.market_force_upload)
    }
}

#[derive(Debug, Clone)]
pub struct SyncCoordinatorControl {
    pub is_online: bool,
    pub is_wifi: bool,
    pub is_visible: bool,
    pub pending_wake: SyncWakeRequest,
    pub cooldown_until: Option<std::time::Instant>,
}

impl Default for SyncCoordinatorControl {
    fn default() -> Self {
        Self {
            is_online: true,
            is_wifi: true,
            is_visible: true,
            pending_wake: SyncWakeRequest::default(),
            cooldown_until: None,
        }
    }
}

/// Ephemeral single-use server-side grant issued when unlocking with Quick-PIN under `timeout == 0`.
struct OneShotPinGrant {
    token: String,
    secret: zeroize::Zeroizing<String>,
    generation: u64,
    owner_did: String,
    expires_at: std::time::Instant,
}

/// Central application state shared across Tauri commands and background workers.
///
/// # Lock Ordering Invariant
/// Whenever multiple mutexes on `AppState` must be acquired simultaneously,
/// acquire them strictly in the following total order to prevent deadlocks:
/// `recovery_op_lock -> post_op_lock -> service -> profile_caches`.
/// `active_profile_lock`, `sync_control`, and `one_shot_pin_grant` are leaf mutexes never held while acquiring `service` or across `.await`.
///
/// Never hold any `std::sync::MutexGuard` across an `.await` point.
pub struct AppState {
    pub service: Mutex<AppService>,
    pub profile_caches: Mutex<Option<ActiveProfileCaches>>,
    pub session_generation: std::sync::atomic::AtomicU64,
    session_gen_tx: tokio::sync::watch::Sender<u64>,
    pub recovery_op_lock: tokio::sync::Mutex<()>,
    pub post_op_lock: tokio::sync::Mutex<()>,
    pub startup_launch_items: Mutex<Vec<String>>,
    pub sync_control: Mutex<SyncCoordinatorControl>,
    pub sync_coordinator_wake: std::sync::Arc<tokio::sync::Notify>,
    session_lock_dir: Option<std::path::PathBuf>,
    active_profile_lock: Mutex<Option<ProfileSessionLock>>,
    one_shot_pin_grant: Mutex<Option<OneShotPinGrant>>,
}

fn load_bytes_opt_inner(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
    is_local_sidecar: bool,
) -> Result<Option<Vec<u8>>, AppFacadeError> {
    let res = if is_local_sidecar {
        service.load_local_sidecar_data(key, password)
    } else {
        service.load_encrypted_data(key, password)
    };
    match res {
        Ok(bytes) => Ok(Some(bytes)),
        Err(AppFacadeError::Storage(human_money_core::StorageError::NotFound)) => Ok(None),
        Err(e) => Err(e),
    }
}

fn load_json_opt_inner<T: serde::de::DeserializeOwned>(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
    is_local_sidecar: bool,
) -> Result<Option<T>, AppFacadeError> {
    match load_bytes_opt_inner(service, key, password, is_local_sidecar)? {
        Some(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| AppFacadeError::Json(format!("Failed to parse {}: {}", key, e))),
        None => Ok(None),
    }
}

fn save_json_inner<T: serde::Serialize>(
    service: &mut AppService,
    key: &str,
    value: &T,
    password: Option<&str>,
    is_local_sidecar: bool,
) -> Result<(), AppFacadeError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| AppFacadeError::Json(format!("Failed to serialize {}: {}", key, e)))?;
    if is_local_sidecar {
        service.save_local_sidecar_data(key, &bytes, password)
    } else {
        service.save_encrypted_data(key, &bytes, password)
    }
}

/// Loads an encrypted raw byte block, returning `Ok(None)` only on `StorageError::NotFound`.
///
/// Propagates lock, session, authentication, and I/O errors without silent defaults.
pub fn load_encrypted_bytes_opt(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
) -> Result<Option<Vec<u8>>, AppFacadeError> {
    load_bytes_opt_inner(service, key, password, false)
}

/// Loads and deserializes an encrypted JSON block, returning `Ok(None)` only on `StorageError::NotFound`.
///
/// Propagates lock, session, authentication, I/O, and JSON parse errors without silent defaults.
pub fn load_encrypted_json_opt<T: serde::de::DeserializeOwned>(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
) -> Result<Option<T>, AppFacadeError> {
    load_json_opt_inner(service, key, password, false)
}

/// Loads and deserializes an encrypted JSON block, falling back to `T::default()` only on `StorageError::NotFound`.
///
/// Propagates lock, session, authentication, I/O, and JSON parse errors so corrupt or locked
/// storage is never silently treated as empty state.
pub fn load_encrypted_json_or_default<T: serde::de::DeserializeOwned + Default>(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
) -> Result<T, AppFacadeError> {
    Ok(load_encrypted_json_opt(service, key, password)?.unwrap_or_default())
}

/// Saves raw encrypted bytes under `key`.
pub fn save_encrypted_bytes(
    service: &mut AppService,
    key: &str,
    bytes: &[u8],
    password: Option<&str>,
) -> Result<(), AppFacadeError> {
    service.save_encrypted_data(key, bytes, password)
}

/// Loads device-local encrypted raw bytes (`.local_<key>.enc`), returning `Ok(None)` only on `StorageError::NotFound`.
pub fn load_local_sidecar_bytes_opt(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
) -> Result<Option<Vec<u8>>, AppFacadeError> {
    load_bytes_opt_inner(service, key, password, true)
}

/// Saves device-local encrypted raw bytes (`.local_<key>.enc`) without touching `WalletSeal`.
pub fn save_local_sidecar_bytes(
    service: &mut AppService,
    key: &str,
    bytes: &[u8],
    password: Option<&str>,
) -> Result<(), AppFacadeError> {
    service.save_local_sidecar_data(key, bytes, password)
}

/// Serializes `value` as JSON and saves it to the encrypted store under `key`.
pub fn save_encrypted_json<T: serde::Serialize>(
    service: &mut AppService,
    key: &str,
    value: &T,
    password: Option<&str>,
) -> Result<(), AppFacadeError> {
    save_json_inner(service, key, value, password, false)
}

/// Loads a device-local encrypted JSON sidecar (`.local_<key>.enc`), returning `Ok(None)` only on `StorageError::NotFound`.
pub fn load_local_sidecar_json_opt<T: serde::de::DeserializeOwned>(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
) -> Result<Option<T>, AppFacadeError> {
    load_json_opt_inner(service, key, password, true)
}

/// Loads a device-local encrypted JSON sidecar, falling back to `T::default()` only on `StorageError::NotFound`.
pub fn load_local_sidecar_json_or_default<T: serde::de::DeserializeOwned + Default>(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
) -> Result<T, AppFacadeError> {
    Ok(load_local_sidecar_json_opt(service, key, password)?.unwrap_or_default())
}

/// Loads canonical outgoing transfer bundles from encrypted storage (`OUTGOING_BUNDLES_KEY`).
pub fn load_outgoing_bundles(
    service: &mut AppService,
    password: Option<&str>,
) -> Result<crate::models::StoredOutgoingBundles, crate::models::FrontendError> {
    load_encrypted_json_or_default(service, OUTGOING_BUNDLES_KEY, password)
        .map_err(crate::models::FrontendError::from)
}

/// Returns exact canonical bytes for an outgoing `bundle_id` if still retained.
pub fn get_outgoing_bundle_bytes(
    service: &mut AppService,
    bundle_id: &str,
    password: Option<&str>,
) -> Result<Option<Vec<u8>>, crate::models::FrontendError> {
    let store = load_outgoing_bundles(service, password)?;
    Ok(store.bundles.get(bundle_id).map(|b| b.bundle_bytes.clone()))
}

/// Serialized companion batch produced for an atomic outgoing bundle + history commit.
pub struct OutgoingCompanionBatch {
    pub blobs: Vec<(String, Vec<u8>)>,
    pub history: Vec<TransactionRecord>,
    pub outgoing: crate::models::StoredOutgoingBundles,
}

/// Pure companion builder producing serialized `(OUTGOING_BUNDLES_KEY, TRANSACTION_HISTORY_KEY)` blobs
/// with `bundle_data` stripped from `transaction_history` to stay well below the `WalletTransaction` undo limit.
pub fn prepare_outgoing_history_and_bundle_companions(
    baseline_history: &[TransactionRecord],
    baseline_outgoing: &crate::models::StoredOutgoingBundles,
    entries: Vec<(TransactionRecord, Vec<u8>)>,
) -> Result<OutgoingCompanionBatch, crate::models::FrontendError> {
    let mut next_history = baseline_history.to_vec();
    let mut next_outgoing = baseline_outgoing.clone();

    for (mut record, bundle_bytes) in entries {
        if !record.bundle_id.is_empty() && !bundle_bytes.is_empty() {
            next_outgoing.bundles.insert(
                record.bundle_id.clone(),
                crate::models::StoredOutgoingBundle {
                    bundle_id: record.bundle_id.clone(),
                    recipient_id: record.recipient_id.clone(),
                    created_at: record.timestamp.clone(),
                    bundle_bytes,
                },
            );
        }
        record.bundle_data.clear();
        let duplicate = !record.bundle_id.is_empty()
            && next_history
                .iter()
                .any(|r| r.bundle_id == record.bundle_id && r.direction == record.direction);
        if !duplicate {
            next_history.push(record);
        }
    }

    let outgoing_bytes = serde_json::to_vec(&next_outgoing).map_err(|e| {
        crate::models::FrontendError::from(format!("Failed to serialize outgoing_bundles: {}", e))
    })?;
    let history_bytes = serde_json::to_vec(&next_history).map_err(|e| {
        crate::models::FrontendError::from(format!(
            "Failed to serialize transaction_history: {}",
            e
        ))
    })?;
    let blobs = vec![
        (OUTGOING_BUNDLES_KEY.to_string(), outgoing_bytes),
        (TRANSACTION_HISTORY_KEY.to_string(), history_bytes),
    ];
    Ok(OutgoingCompanionBatch {
        blobs,
        history: next_history,
        outgoing: next_outgoing,
    })
}

pub type CompanionBlobBatch = (Vec<(String, Vec<u8>)>, Vec<TransactionRecord>);

/// Prepares the encrypted `transaction_history` companion blob alongside the next in-memory
/// history list for atomic `CommitWithCompanionBlobs` receive transactions.
pub fn prepare_receive_history_companion(
    baseline_history: &[TransactionRecord],
    mut record: TransactionRecord,
) -> Result<CompanionBlobBatch, crate::models::FrontendError> {
    record.bundle_data.clear();
    let mut next_history = baseline_history.to_vec();
    let duplicate = !record.bundle_id.is_empty()
        && next_history
            .iter()
            .any(|r| r.bundle_id == record.bundle_id && r.direction == record.direction);
    if !duplicate {
        next_history.push(record);
    }
    let history_bytes = serde_json::to_vec(&next_history).map_err(|e| {
        crate::models::FrontendError::from(format!(
            "Failed to serialize transaction_history: {}",
            e
        ))
    })?;
    Ok((
        vec![(TRANSACTION_HISTORY_KEY.to_string(), history_bytes)],
        next_history,
    ))
}

/// Serializes `value` as JSON and saves it to a device-local encrypted sidecar (`.local_<key>.enc`).
/// Never triggers `WalletSeal` or backup dirtying.
pub fn save_local_sidecar_json<T: serde::Serialize>(
    service: &mut AppService,
    key: &str,
    value: &T,
    password: Option<&str>,
) -> Result<(), AppFacadeError> {
    save_json_inner(service, key, value, password, true)
}

/// Loads verified highest `LockReceipt`s recorded per `lock_pub` for `anchor_id` from the local sidecar.
///
/// Fails closed on decryption, parsing, key-format, or signature corruption; returns an empty map
/// ONLY when no sidecar file exists yet (`StorageError::NotFound`).
pub fn load_known_head_receipts_for_anchor(
    service: &mut AppService,
    anchor_id: &[u8; 32],
    password: Option<&str>,
) -> Result<
    std::collections::HashMap<
        human_money_core::models::layer2_api::LockPubKey,
        human_money_core::models::layer2_api::LockReceipt,
    >,
    crate::models::FrontendError,
> {
    let heads_state = load_local_sidecar_json_or_default::<KnownLockHeadsState>(
        service,
        KNOWN_LOCK_HEADS_KEY,
        password,
    )
    .map_err(crate::models::FrontendError::from)?;
    let anchor_hex = human_money_core::hex::encode(anchor_id);
    let pfx = format!("{anchor_hex}:");
    let mut out = std::collections::HashMap::new();
    for (k, receipt) in heads_state.heads {
        if k.starts_with(&pfx) {
            let expected_key = format!(
                "{}:{}",
                anchor_hex,
                human_money_core::hex::encode(receipt.lock_pub)
            );
            if k != expected_key || !receipt.verify_signature() {
                return Err(crate::models::FrontendError::from(format!(
                    "Corrupt anti-rollback head receipt entry '{}' for anchor {}",
                    k, anchor_hex
                )));
            }
            out.insert(receipt.lock_pub, receipt);
        }
    }
    Ok(out)
}

/// Records verified `LockReceipt`s (retaining the highest `write_seq` per `(anchor_id, lock_pub)`)
/// into the local sidecar without mutating `WalletSeal`.
pub fn record_known_head_receipts(
    service: &mut AppService,
    anchor_id: &[u8; 32],
    receipts: &[human_money_core::models::layer2_api::LockReceipt],
    password: Option<&str>,
) -> Result<(), crate::models::FrontendError> {
    if receipts.is_empty() {
        return Ok(());
    }
    let mut heads_state = load_local_sidecar_json_or_default::<KnownLockHeadsState>(
        service,
        KNOWN_LOCK_HEADS_KEY,
        password,
    )
    .map_err(crate::models::FrontendError::from)?;
    let anchor_hex = human_money_core::hex::encode(anchor_id);
    let mut changed = false;
    for r in receipts {
        if !r.verify_signature() {
            continue;
        }
        let key = format!(
            "{}:{}",
            anchor_hex,
            human_money_core::hex::encode(r.lock_pub)
        );
        let replace = heads_state
            .heads
            .get(&key)
            .is_none_or(|prev| r.write_seq > prev.write_seq);
        if replace {
            heads_state.heads.insert(key, r.clone());
            changed = true;
        }
    }
    if changed {
        save_local_sidecar_json(service, KNOWN_LOCK_HEADS_KEY, &heads_state, password)
            .map_err(crate::models::FrontendError::from)?;
    }
    Ok(())
}

fn endpoints_overlap(a_read: &str, a_write: &str, b_read: &str, b_write: &str) -> bool {
    let ar = crate::services::common::normalize_sync_url(a_read);
    let aw = crate::services::common::normalize_sync_url(a_write);
    let br = crate::services::common::normalize_sync_url(b_read);
    let bw = crate::services::common::normalize_sync_url(b_write);
    (!ar.is_empty() && (ar == br || ar == bw)) || (!aw.is_empty() && (aw == br || aw == bw))
}

impl AppState {
    pub fn new(service: AppService) -> Self {
        let (initial_gen, initial_caches) =
            match service.with_wallet(|w| w.get_user_id().to_string()) {
                Ok(owner_did) if service.is_wallet_unlocked() => {
                    (1u64, Some(ActiveProfileCaches::new(1, owner_did)))
                }
                _ => (0u64, None),
            };
        let (session_gen_tx, _) = tokio::sync::watch::channel(initial_gen);
        Self {
            service: Mutex::new(service),
            profile_caches: Mutex::new(initial_caches),
            session_generation: std::sync::atomic::AtomicU64::new(initial_gen),
            session_gen_tx,
            recovery_op_lock: tokio::sync::Mutex::new(()),
            post_op_lock: tokio::sync::Mutex::new(()),
            startup_launch_items: Mutex::new(Vec::new()),
            sync_control: Mutex::new(SyncCoordinatorControl::default()),
            sync_coordinator_wake: std::sync::Arc::new(tokio::sync::Notify::new()),
            session_lock_dir: None,
            active_profile_lock: Mutex::new(None),
            one_shot_pin_grant: Mutex::new(None),
        }
    }

    pub fn update_network_status(&self, is_online: bool, is_wifi: bool, is_visible: bool) {
        let should_notify = {
            let mut guard = self.sync_control.lock().unwrap();
            let became_online = !guard.is_online && is_online;
            let became_wifi = !guard.is_wifi && is_wifi;
            let became_visible = !guard.is_visible && is_visible;
            guard.is_online = is_online;
            guard.is_wifi = is_wifi;
            guard.is_visible = is_visible;
            became_online || became_wifi || became_visible
        };
        if should_notify {
            self.sync_coordinator_wake.notify_one();
        }
    }

    pub fn wake_sync_coordinator(&self, req: SyncWakeRequest) {
        {
            let mut guard = self.sync_control.lock().unwrap();
            guard.pending_wake.merge(req);
        }
        self.sync_coordinator_wake.notify_one();
    }

    pub fn notify_sync_coordinator(&self) {
        self.sync_coordinator_wake.notify_one();
    }

    pub fn take_sync_coordinator_snapshot(&self) -> (SyncCoordinatorControl, SyncWakeRequest) {
        let mut guard = self.sync_control.lock().unwrap();
        let wake = std::mem::take(&mut guard.pending_wake);
        (guard.clone(), wake)
    }

    pub fn record_sync_cooldown(&self, retry_after_secs: u64) {
        let clamped = retry_after_secs.clamp(5, 3600);
        let target = std::time::Instant::now() + std::time::Duration::from_secs(clamped);
        let mut guard = self.sync_control.lock().unwrap();
        guard.cooldown_until = Some(guard.cooldown_until.map_or(target, |prev| prev.max(target)));
    }

    pub fn clear_sync_cooldown(&self) {
        let mut guard = self.sync_control.lock().unwrap();
        guard.cooldown_until = None;
        guard.pending_wake = SyncWakeRequest::default();
    }

    pub fn current_session_generation(&self) -> u64 {
        self.session_generation
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn is_generation_current(&self, generation: u64) -> bool {
        self.current_session_generation() == generation
    }

    pub fn subscribe_session_generation(&self) -> tokio::sync::watch::Receiver<u64> {
        self.session_gen_tx.subscribe()
    }

    pub fn session_token_for_service(
        &self,
        service: &AppService,
    ) -> Result<SessionToken, crate::models::FrontendError> {
        if !service.is_wallet_unlocked() {
            return Err(crate::models::FrontendError::from(
                AppFacadeError::WalletLocked,
            ));
        }
        let owner_did = service
            .with_wallet(|w| w.get_user_id().to_string())
            .map_err(crate::models::FrontendError::from)?;
        Ok(SessionToken {
            generation: self.current_session_generation(),
            owner_did,
        })
    }

    pub fn begin_session_op(&self) -> Result<SessionToken, crate::models::FrontendError> {
        let service = self.lock_service()?;
        self.session_token_for_service(&service)
    }

    pub fn authorize_and_begin_session(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<SessionToken, crate::models::FrontendError> {
        let service = self.authorize_and_lock(required, password)?;
        self.session_token_for_service(&service)
    }

    /// Synchronizes host session caches, one-shot grants, and generation when Core
    /// intentionally locks itself (e.g. post-mutation failure in `TransactionOrchestrator`).
    pub fn sync_if_core_locked(&self, service: &AppService) {
        if !service.is_wallet_unlocked() {
            let has_host_state = self.profile_caches.lock().unwrap().is_some()
                || self.one_shot_pin_grant.lock().unwrap().is_some();
            if has_host_state {
                self.clear_all_caches();
            }
        }
    }

    pub fn is_session_current(&self, token: &SessionToken, service: &AppService) -> bool {
        if self.current_session_generation() != token.generation || !service.is_wallet_unlocked() {
            return false;
        }
        service
            .with_wallet(|w| w.get_user_id() == token.owner_did)
            .unwrap_or(false)
    }

    pub fn with_session<F, R>(
        &self,
        token: &SessionToken,
        f: F,
    ) -> Result<Option<R>, crate::models::FrontendError>
    where
        F: FnOnce(&mut AppService) -> Result<R, crate::models::FrontendError>,
    {
        let mut service = self.lock_service()?;
        self.sync_if_core_locked(&service);
        if !self.is_session_current(token, &service) {
            return Ok(None);
        }
        let res = f(&mut service);
        self.sync_if_core_locked(&service);
        res.map(Some)
    }

    pub fn with_session_required<F, R>(
        &self,
        token: &SessionToken,
        superseded_msg: &str,
        f: F,
    ) -> Result<R, crate::models::FrontendError>
    where
        F: FnOnce(&mut AppService) -> Result<R, crate::models::FrontendError>,
    {
        self.with_session(token, f)?
            .ok_or_else(|| crate::models::FrontendError::from(superseded_msg.to_string()))
    }

    pub fn is_media_negatively_cached(
        &self,
        session: &SessionToken,
        blob_id: &str,
        read_url: &str,
        now_secs: u64,
        ttl_secs: u64,
    ) -> bool {
        if self.current_session_generation() != session.generation {
            return false;
        }
        let key = (
            blob_id.to_string(),
            read_url.trim_end_matches('/').to_string(),
        );
        let guard = self.profile_caches.lock().unwrap();
        let Some(ref caches) = *guard else {
            return false;
        };
        if caches.generation != session.generation || caches.owner_did != session.owner_did {
            return false;
        }
        caches
            .media_negative_cache
            .get(&key)
            .is_some_and(|&ts| now_secs.saturating_sub(ts) < ttl_secs)
    }

    pub fn record_media_negative_cache(
        &self,
        session: &SessionToken,
        blob_id: String,
        read_url: String,
        now_secs: u64,
    ) {
        if self.current_session_generation() != session.generation {
            return;
        }
        let key = (blob_id, read_url.trim_end_matches('/').to_string());
        let mut guard = self.profile_caches.lock().unwrap();
        let needs_init = guard
            .as_ref()
            .is_none_or(|c| c.generation != session.generation || c.owner_did != session.owner_did);
        if needs_init {
            *guard = Some(ActiveProfileCaches::new(
                session.generation,
                session.owner_did.clone(),
            ));
        }
        if let Some(ref mut caches) = *guard {
            if caches.media_negative_cache.len() >= 1024 {
                caches.media_negative_cache.clear();
            }
            caches.media_negative_cache.insert(key, now_secs);
        }
    }

    pub fn with_session_lock_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.session_lock_dir = Some(dir);
        self
    }

    pub fn acquire_profile_lock(
        &self,
        folder_name: &str,
    ) -> Result<PendingProfileLock, crate::models::FrontendError> {
        let Some(ref lock_dir) = self.session_lock_dir else {
            return Ok(PendingProfileLock::None);
        };
        {
            let guard = self.active_profile_lock.lock().unwrap();
            if let Some(ref existing) = *guard {
                if existing.folder_name == folder_name {
                    return Ok(PendingProfileLock::AlreadyHeld);
                }
            }
        }
        let lock = ProfileSessionLock::try_acquire(lock_dir, folder_name)?;
        Ok(PendingProfileLock::Acquired(lock))
    }

    pub fn commit_profile_lock(&self, pending: PendingProfileLock) {
        if let PendingProfileLock::Acquired(lock) = pending {
            *self.active_profile_lock.lock().unwrap() = Some(lock);
        }
    }

    pub fn release_profile_lock(&self) {
        *self.active_profile_lock.lock().unwrap() = None;
    }

    /// Releases the held `ProfileSessionLock` guard iff it belongs to `folder_name`.
    /// Never unlinks the underlying `.lock` file path so live `flock` inodes stay intact.
    pub fn release_profile_lock_if_matching(&self, folder_name: &str) {
        let mut guard = self.active_profile_lock.lock().unwrap();
        if guard
            .as_ref()
            .is_some_and(|existing| existing.folder_name == folder_name)
        {
            *guard = None;
        }
    }

    // 1. Central helper methods to access the locked service
    pub fn service(&self) -> MutexGuard<'_, AppService> {
        self.service.lock().unwrap()
    }

    pub fn lock_service(&self) -> Result<MutexGuard<'_, AppService>, crate::models::FrontendError> {
        self.service.lock().map_err(|_| {
            crate::models::FrontendError::from("Wallet service unavailable".to_string())
        })
    }

    pub fn clear_one_shot_pin_grant(&self) {
        *self.one_shot_pin_grant.lock().unwrap() = None;
    }

    pub fn issue_one_shot_pin_grant(
        &self,
        service: &AppService,
        secret: String,
    ) -> Result<String, crate::models::FrontendError> {
        if !service.is_wallet_unlocked() {
            return Err(crate::models::FrontendError::from(
                AppFacadeError::WalletLocked,
            ));
        }
        let owner_did = service
            .with_wallet(|w| w.get_user_id().to_string())
            .map_err(crate::models::FrontendError::from)?;
        let generation = self.current_session_generation();
        let mut rnd = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut rnd);
        let token = format!("{}{}", ONE_SHOT_PIN_GRANT_PREFIX, hex::encode(rnd));
        let grant = OneShotPinGrant {
            token: token.clone(),
            secret: zeroize::Zeroizing::new(secret),
            generation,
            owner_did,
            expires_at: std::time::Instant::now() + ONE_SHOT_PIN_GRANT_TTL,
        };
        *self.one_shot_pin_grant.lock().unwrap() = Some(grant);
        Ok(token)
    }

    fn consume_one_shot_pin_grant(
        &self,
        service: &AppService,
        token: &str,
    ) -> Result<zeroize::Zeroizing<String>, crate::models::FrontendError> {
        let taken = self.one_shot_pin_grant.lock().unwrap().take();
        let Some(grant) = taken else {
            return Err(crate::models::FrontendError::from(
                human_money_core::Error::App(human_money_core::error::AppError::SessionExpired {
                    reason: "One-shot PIN grant missing or already used.".to_string(),
                }),
            ));
        };
        let owner_matches = service
            .with_wallet(|w| w.get_user_id() == grant.owner_did)
            .unwrap_or(false);
        if grant.token != token
            || std::time::Instant::now() > grant.expires_at
            || self.current_session_generation() != grant.generation
            || !service.is_wallet_unlocked()
            || !owner_matches
        {
            return Err(crate::models::FrontendError::from(
                human_money_core::Error::App(human_money_core::error::AppError::SessionExpired {
                    reason: "One-shot PIN grant expired or bound to a different session."
                        .to_string(),
                }),
            ));
        }
        Ok(grant.secret)
    }

    /// Enforces `required` access and resolves any single-use `hmc_grant_*` PIN grant into its
    /// underlying zeroizing secret for `DailyPay` operations.
    pub fn authorize_and_lock_secret(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<
        (
            MutexGuard<'_, AppService>,
            Option<zeroize::Zeroizing<String>>,
        ),
        crate::models::FrontendError,
    > {
        let service = self.lock_service()?;
        self.sync_if_core_locked(&service);
        if let Some(raw) = password {
            if raw.starts_with(ONE_SHOT_PIN_GRANT_PREFIX) {
                match required {
                    RequiredAccess::Full => {
                        return Err(crate::models::FrontendError::from(
                            human_money_core::Error::App(
                                human_money_core::error::AppError::InsufficientAccessMode {
                                    required: human_money_core::AccessMode::Full
                                        .as_str()
                                        .to_string(),
                                    current: human_money_core::AccessMode::DailyPay
                                        .as_str()
                                        .to_string(),
                                },
                            ),
                        ));
                    }
                    RequiredAccess::DailyPay => {
                        let secret = self.consume_one_shot_pin_grant(&service, raw)?;
                        service
                            .require_access_mode(
                                human_money_core::AccessMode::DailyPay,
                                Some(secret.as_str()),
                            )
                            .map_err(crate::models::FrontendError::from)?;
                        return Ok((service, Some(secret)));
                    }
                    RequiredAccess::Unlocked => {
                        return Err(crate::models::FrontendError::from(
                            human_money_core::Error::App(
                                human_money_core::error::AppError::SessionExpired {
                                    reason: "One-shot PIN grant cannot authorize Unlocked access."
                                        .to_string(),
                                },
                            ),
                        ));
                    }
                }
            }
        }

        match required {
            RequiredAccess::Unlocked => {
                if !service.is_wallet_unlocked() {
                    return Err(crate::models::FrontendError::from(
                        AppFacadeError::WalletLocked,
                    ));
                }
            }
            RequiredAccess::DailyPay => {
                service
                    .require_access_mode(human_money_core::AccessMode::DailyPay, password)
                    .map_err(crate::models::FrontendError::from)?;
            }
            RequiredAccess::Full => {
                service
                    .require_access_mode(human_money_core::AccessMode::Full, password)
                    .map_err(crate::models::FrontendError::from)?;
            }
        }
        Ok((
            service,
            password.map(|p| zeroize::Zeroizing::new(p.to_string())),
        ))
    }

    pub fn authorize_and_begin_session_secret(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<(SessionToken, Option<zeroize::Zeroizing<String>>), crate::models::FrontendError>
    {
        let (service, resolved_secret) = self.authorize_and_lock_secret(required, password)?;
        let session = self.session_token_for_service(&service)?;
        Ok((session, resolved_secret))
    }

    /// Enforces `required` access at the Tauri boundary and returns the locked `AppService`.
    pub fn authorize_and_lock(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<MutexGuard<'_, AppService>, crate::models::FrontendError> {
        let service = self.lock_service()?;
        self.sync_if_core_locked(&service);
        if password.is_some_and(|p| p.starts_with(ONE_SHOT_PIN_GRANT_PREFIX)) {
            return match required {
                RequiredAccess::Full => Err(crate::models::FrontendError::from(
                    human_money_core::Error::App(
                        human_money_core::error::AppError::InsufficientAccessMode {
                            required: human_money_core::AccessMode::Full.as_str().to_string(),
                            current: human_money_core::AccessMode::DailyPay.as_str().to_string(),
                        },
                    ),
                )),
                RequiredAccess::DailyPay | RequiredAccess::Unlocked => Err(
                    crate::models::FrontendError::from(human_money_core::Error::App(
                        human_money_core::error::AppError::SessionExpired {
                            reason: "One-shot PIN grant must be consumed via secret guard."
                                .to_string(),
                        },
                    )),
                ),
            };
        }
        match required {
            RequiredAccess::Unlocked => {
                if !service.is_wallet_unlocked() {
                    return Err(crate::models::FrontendError::from(
                        AppFacadeError::WalletLocked,
                    ));
                }
            }
            RequiredAccess::DailyPay => {
                service
                    .require_access_mode(human_money_core::AccessMode::DailyPay, password)
                    .map_err(crate::models::FrontendError::from)?;
            }
            RequiredAccess::Full => {
                service
                    .require_access_mode(human_money_core::AccessMode::Full, password)
                    .map_err(crate::models::FrontendError::from)?;
            }
        }
        Ok(service)
    }

    /// Enforces `required` access without holding `AppService` mutex across `.await` points.
    pub fn authorize(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<(), crate::models::FrontendError> {
        let _service = self.authorize_and_lock(required, password)?;
        Ok(())
    }

    // 2. Settings & Cache Helpers (Unified ActiveProfileCaches under single mutex)
    fn get_cached_store<T: Clone>(
        &self,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        name: &str,
    ) -> Result<T, AppFacadeError> {
        let mut guard = self.profile_caches.lock().unwrap();
        guard
            .as_mut()
            .and_then(|c| field(c).clone())
            .ok_or_else(|| AppFacadeError::Generic(format!("{} not loaded", name)))
    }

    pub(crate) fn write_cache_slot_for_service<T>(
        &self,
        service: &AppService,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        value: Option<T>,
    ) {
        let current_gen = self.current_session_generation();
        let owner_did = service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap_or_default();
        let mut guard = self.profile_caches.lock().unwrap();
        let needs_reset = guard.as_ref().is_none_or(|c| {
            c.generation != current_gen || (!owner_did.is_empty() && c.owner_did != owner_did)
        });
        if needs_reset {
            *guard = Some(ActiveProfileCaches::new(current_gen, owner_did));
        }
        if let Some(ref mut caches) = *guard {
            *field(caches) = value;
        }
    }

    pub fn cache_contacts(&self) -> Option<FrontendAddressBook> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.contacts.clone())
    }

    pub fn set_cached_contacts(&self, value: Option<FrontendAddressBook>) {
        self.set_cache_slot_direct(|c| &mut c.contacts, value);
    }

    pub fn set_cached_contacts_for_service(
        &self,
        service: &AppService,
        value: Option<FrontendAddressBook>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.contacts, value);
    }

    pub fn with_cached_contacts_mut<R>(
        &self,
        f: impl FnOnce(&mut Option<FrontendAddressBook>) -> R,
    ) -> R {
        let mut guard = self.profile_caches.lock().unwrap();
        if let Some(ref mut caches) = *guard {
            f(&mut caches.contacts)
        } else {
            let mut none = None;
            f(&mut none)
        }
    }

    pub fn cache_events(&self) -> Option<Vec<human_money_core::models::wallet_event::WalletEvent>> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.events.clone())
    }

    pub fn set_cached_events(
        &self,
        value: Option<Vec<human_money_core::models::wallet_event::WalletEvent>>,
    ) {
        self.set_cache_slot_direct(|c| &mut c.events, value);
    }

    pub fn cache_wot(&self) -> Option<humoco_wot::TrustRegister> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.wot.clone())
    }

    pub fn set_cached_wot(&self, value: Option<humoco_wot::TrustRegister>) {
        self.set_cache_slot_direct(|c| &mut c.wot, value);
    }

    pub fn cache_ui_preferences(&self) -> Option<UiPreferences> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.ui_preferences.clone())
    }

    pub fn set_cached_ui_preferences(&self, value: Option<UiPreferences>) {
        self.set_cache_slot_direct(|c| &mut c.ui_preferences, value);
    }

    pub fn cache_security_policy(&self) -> Option<SecurityPolicy> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.security_policy.clone())
    }

    pub fn set_cached_security_policy(&self, value: Option<SecurityPolicy>) {
        self.set_cache_slot_direct(|c| &mut c.security_policy, value);
    }

    pub fn cache_internal_lifecycle(&self) -> Option<InternalLifecycleState> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.internal_lifecycle.clone())
    }

    pub fn set_cached_internal_lifecycle(&self, value: Option<InternalLifecycleState>) {
        self.set_cache_slot_direct(|c| &mut c.internal_lifecycle, value);
    }

    pub fn cache_history(&self) -> Option<Vec<TransactionRecord>> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.history.clone())
    }

    pub fn set_cached_history(&self, value: Option<Vec<TransactionRecord>>) {
        self.set_cache_slot_direct(|c| &mut c.history, value);
    }

    pub fn cache_own_market_profile(&self) -> Option<crate::models::StoredOwnMarketProfile> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.own_market_profile.clone())
    }

    pub fn set_cached_own_market_profile(
        &self,
        value: Option<crate::models::StoredOwnMarketProfile>,
    ) {
        self.set_cache_slot_direct(|c| &mut c.own_market_profile, value);
    }

    pub fn set_cached_own_market_profile_for_service(
        &self,
        service: &AppService,
        value: Option<crate::models::StoredOwnMarketProfile>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.own_market_profile, value);
    }

    pub fn cache_recovery_owner(
        &self,
    ) -> Option<Option<crate::models::recovery::RecoveryOwnerState>> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.recovery_owner.clone())
    }

    pub fn set_cached_recovery_owner(
        &self,
        value: Option<Option<crate::models::recovery::RecoveryOwnerState>>,
    ) {
        self.set_cache_slot_direct(|c| &mut c.recovery_owner, value);
    }

    pub fn set_cached_recovery_owner_for_service(
        &self,
        service: &AppService,
        value: Option<Option<crate::models::recovery::RecoveryOwnerState>>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.recovery_owner, value);
    }

    pub fn cache_guardian_vault(&self) -> Option<humoco_market::GuardianVaultStore> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.guardian_vault.clone())
    }

    pub fn set_cached_guardian_vault(&self, value: Option<humoco_market::GuardianVaultStore>) {
        self.set_cache_slot_direct(|c| &mut c.guardian_vault, value);
    }

    pub fn set_cached_guardian_vault_for_service(
        &self,
        service: &AppService,
        value: Option<humoco_market::GuardianVaultStore>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.guardian_vault, value);
    }

    pub fn cache_app_notices(&self) -> Option<crate::models::StoredAppNotices> {
        self.profile_caches
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.app_notices.clone())
    }

    pub fn set_cached_app_notices(&self, value: Option<crate::models::StoredAppNotices>) {
        self.set_cache_slot_direct(|c| &mut c.app_notices, value);
    }

    fn set_cache_slot_direct<T>(
        &self,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        value: Option<T>,
    ) {
        let mut guard = self.profile_caches.lock().unwrap();
        if value.is_some() && guard.is_none() {
            *guard = Some(ActiveProfileCaches::new(
                self.current_session_generation(),
                String::new(),
            ));
        }
        if let Some(ref mut caches) = *guard {
            *field(caches) = value;
        }
    }

    fn load_store_inner<T>(
        &self,
        service: &mut AppService,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        password: Option<&str>,
        is_local_sidecar: bool,
    ) -> Result<T, AppFacadeError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        let loaded_opt = if is_local_sidecar {
            load_local_sidecar_json_opt::<T>(service, key, password)?
        } else {
            load_encrypted_json_opt::<T>(service, key, password)?
        };
        let value: T = match loaded_opt {
            Some(loaded) => loaded,
            None => {
                info!("No {} file found, creating default.", key);
                let default_value = T::default();
                let save_res = if is_local_sidecar {
                    save_local_sidecar_json(service, key, &default_value, password)
                } else {
                    save_encrypted_json(service, key, &default_value, password)
                };
                save_res.map_err(|e| {
                    AppFacadeError::Generic(format!("Failed to save default {}: {}", key, e))
                })?;
                default_value
            }
        };
        self.write_cache_slot_for_service(service, field, Some(value.clone()));
        Ok(value)
    }

    fn load_store<T>(
        &self,
        service: &mut AppService,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        password: Option<&str>,
    ) -> Result<T, AppFacadeError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        self.load_store_inner(service, field, key, password, false)
    }

    fn load_local_store<T>(
        &self,
        service: &mut AppService,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        password: Option<&str>,
    ) -> Result<T, AppFacadeError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        self.load_store_inner(service, field, key, password, true)
    }

    fn get_or_load_store<T>(
        &self,
        service: &mut AppService,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        name: &str,
        password: Option<&str>,
    ) -> Result<T, AppFacadeError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        if let Ok(cached) = self.get_cached_store(field, name) {
            return Ok(cached);
        }
        self.load_store(service, field, key, password)
    }

    fn get_or_load_local_store<T>(
        &self,
        service: &mut AppService,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        name: &str,
        password: Option<&str>,
    ) -> Result<T, AppFacadeError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        if let Ok(cached) = self.get_cached_store(field, name) {
            return Ok(cached);
        }
        self.load_local_store(service, field, key, password)
    }

    fn get_or_load_store_locked<T>(
        &self,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        name: &str,
        password: Option<&str>,
    ) -> Result<T, crate::models::FrontendError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        if let Ok(cached) = self.get_cached_store(field, name) {
            return Ok(cached);
        }
        let mut service = self.lock_service()?;
        self.load_store(&mut service, field, key, password)
            .map_err(crate::models::FrontendError::from)
    }

    fn get_or_load_local_store_locked<T>(
        &self,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        name: &str,
        password: Option<&str>,
    ) -> Result<T, crate::models::FrontendError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        if let Ok(cached) = self.get_cached_store(field, name) {
            return Ok(cached);
        }
        let mut service = self.lock_service()?;
        self.load_local_store(&mut service, field, key, password)
            .map_err(crate::models::FrontendError::from)
    }

    fn save_store<T>(
        &self,
        service: &mut AppService,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        value: T,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError>
    where
        T: serde::Serialize + Clone,
    {
        save_encrypted_json(service, key, &value, password)?;
        self.write_cache_slot_for_service(service, field, Some(value));
        Ok(())
    }

    fn save_local_store<T>(
        &self,
        service: &mut AppService,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        value: T,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError>
    where
        T: serde::Serialize + Clone,
    {
        save_local_sidecar_json(service, key, &value, password)?;
        self.write_cache_slot_for_service(service, field, Some(value));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn mutate_store_for_owner_inner<T>(
        &self,
        expected_owner_did: &str,
        expected_generation: Option<u64>,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        name: &str,
        password: Option<&str>,
        is_local_sidecar: bool,
        f: impl FnOnce(&mut T) -> bool,
    ) -> Result<Option<T>, crate::models::FrontendError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        let mut service = self.lock_service()?;
        if !service.is_wallet_unlocked() {
            return Ok(None);
        }
        if let Some(expected_gen) = expected_generation {
            if self.current_session_generation() != expected_gen {
                return Ok(None);
            }
        }
        let Ok(current_did) = service.with_wallet(|w| w.get_user_id().to_string()) else {
            return Ok(None);
        };
        if current_did != expected_owner_did {
            return Ok(None);
        }
        let mut value = if is_local_sidecar {
            self.get_or_load_local_store(&mut service, field, key, name, password)
        } else {
            self.get_or_load_store(&mut service, field, key, name, password)
        }
        .map_err(crate::models::FrontendError::from)?;
        if f(&mut value) {
            if is_local_sidecar {
                self.save_local_store(&mut service, field, key, value.clone(), password)
            } else {
                self.save_store(&mut service, field, key, value.clone(), password)
            }
            .map_err(crate::models::FrontendError::from)?;
        }
        Ok(Some(value))
    }

    fn mutate_store_for_owner<T>(
        &self,
        expected_owner_did: &str,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        name: &str,
        password: Option<&str>,
        f: impl FnOnce(&mut T) -> bool,
    ) -> Result<Option<T>, crate::models::FrontendError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        self.mutate_store_for_owner_inner(
            expected_owner_did,
            None,
            field,
            key,
            name,
            password,
            false,
            f,
        )
    }

    fn mutate_local_store_for_owner<T>(
        &self,
        expected_owner_did: &str,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        name: &str,
        password: Option<&str>,
        f: impl FnOnce(&mut T) -> bool,
    ) -> Result<Option<T>, crate::models::FrontendError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone,
    {
        self.mutate_store_for_owner_inner(
            expected_owner_did,
            None,
            field,
            key,
            name,
            password,
            true,
            f,
        )
    }

    pub fn get_cached_ui_preferences(&self) -> Result<UiPreferences, AppFacadeError> {
        self.get_cached_store(|c| &mut c.ui_preferences, "UiPreferences")
    }

    pub fn get_or_load_ui_preferences(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<UiPreferences, AppFacadeError> {
        self.get_or_load_store(
            service,
            |c| &mut c.ui_preferences,
            UI_PREFERENCES_KEY,
            "UiPreferences",
            password,
        )
    }

    pub fn get_or_load_ui_preferences_locked(
        &self,
        password: Option<&str>,
    ) -> Result<UiPreferences, crate::models::FrontendError> {
        self.get_or_load_store_locked(
            |c| &mut c.ui_preferences,
            UI_PREFERENCES_KEY,
            "UiPreferences",
            password,
        )
    }

    pub fn load_ui_preferences(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<UiPreferences, AppFacadeError> {
        self.load_store(
            service,
            |c| &mut c.ui_preferences,
            UI_PREFERENCES_KEY,
            password,
        )
    }

    pub fn save_ui_preferences(
        &self,
        service: &mut AppService,
        prefs: UiPreferences,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        let prev_min = match self
            .cache_ui_preferences()
            .as_ref()
            .map(|p| p.share_min_level)
        {
            Some(v) => Some(v),
            None => {
                load_encrypted_json_opt::<UiPreferences>(service, UI_PREFERENCES_KEY, password)?
                    .map(|p| p.share_min_level)
            }
        };
        let new_min = prefs.share_min_level;
        self.save_store(
            service,
            |c| &mut c.ui_preferences,
            UI_PREFERENCES_KEY,
            prefs,
            password,
        )?;
        if prev_min.is_some_and(|prev| new_min > prev) {
            crate::services::market_service::bump_own_card_version_for_eviction(
                self, service, password,
            );
        }
        Ok(())
    }

    pub fn mutate_ui_preferences(
        &self,
        service: &mut AppService,
        password: Option<&str>,
        f: impl FnOnce(&mut UiPreferences),
    ) -> Result<UiPreferences, AppFacadeError> {
        let mut prefs = self.get_or_load_ui_preferences(service, password)?;
        f(&mut prefs);
        self.save_ui_preferences(service, prefs.clone(), password)?;
        Ok(prefs)
    }

    pub fn get_cached_security_policy(&self) -> Result<SecurityPolicy, AppFacadeError> {
        self.get_cached_store(|c| &mut c.security_policy, "SecurityPolicy")
    }

    pub fn get_or_load_security_policy(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<SecurityPolicy, AppFacadeError> {
        self.get_or_load_store(
            service,
            |c| &mut c.security_policy,
            SECURITY_POLICY_KEY,
            "SecurityPolicy",
            password,
        )
    }

    pub fn get_or_load_security_policy_locked(
        &self,
        password: Option<&str>,
    ) -> Result<SecurityPolicy, crate::models::FrontendError> {
        self.get_or_load_store_locked(
            |c| &mut c.security_policy,
            SECURITY_POLICY_KEY,
            "SecurityPolicy",
            password,
        )
    }

    pub fn load_security_policy(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<SecurityPolicy, AppFacadeError> {
        self.load_store(
            service,
            |c| &mut c.security_policy,
            SECURITY_POLICY_KEY,
            password,
        )
    }

    pub fn save_security_policy(
        &self,
        service: &mut AppService,
        policy: SecurityPolicy,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        self.save_store(
            service,
            |c| &mut c.security_policy,
            SECURITY_POLICY_KEY,
            policy,
            password,
        )
    }

    pub fn mutate_security_policy(
        &self,
        service: &mut AppService,
        password: Option<&str>,
        f: impl FnOnce(&mut SecurityPolicy),
    ) -> Result<SecurityPolicy, AppFacadeError> {
        let mut policy = self.get_or_load_security_policy(service, password)?;
        f(&mut policy);
        self.save_security_policy(service, policy.clone(), password)?;
        Ok(policy)
    }

    pub fn mutate_security_policy_for_owner(
        &self,
        expected_owner_did: &str,
        password: Option<&str>,
        f: impl FnOnce(&mut SecurityPolicy) -> bool,
    ) -> Result<Option<SecurityPolicy>, crate::models::FrontendError> {
        self.mutate_store_for_owner(
            expected_owner_did,
            |c| &mut c.security_policy,
            SECURITY_POLICY_KEY,
            "SecurityPolicy",
            password,
            f,
        )
    }

    pub fn mutate_security_policy_for_session(
        &self,
        token: &SessionToken,
        password: Option<&str>,
        f: impl FnOnce(&mut SecurityPolicy) -> bool,
    ) -> Result<Option<SecurityPolicy>, crate::models::FrontendError> {
        self.mutate_store_for_owner_inner(
            &token.owner_did,
            Some(token.generation),
            |c| &mut c.security_policy,
            SECURITY_POLICY_KEY,
            "SecurityPolicy",
            password,
            false,
            f,
        )
    }

    pub fn get_cached_internal_lifecycle(&self) -> Result<InternalLifecycleState, AppFacadeError> {
        self.get_cached_store(|c| &mut c.internal_lifecycle, "InternalLifecycleState")
    }

    pub fn get_or_load_internal_lifecycle(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<InternalLifecycleState, AppFacadeError> {
        self.get_or_load_local_store(
            service,
            |c| &mut c.internal_lifecycle,
            INTERNAL_LIFECYCLE_STATE_KEY,
            "InternalLifecycleState",
            password,
        )
    }

    pub fn get_or_load_internal_lifecycle_locked(
        &self,
        password: Option<&str>,
    ) -> Result<InternalLifecycleState, crate::models::FrontendError> {
        self.get_or_load_local_store_locked(
            |c| &mut c.internal_lifecycle,
            INTERNAL_LIFECYCLE_STATE_KEY,
            "InternalLifecycleState",
            password,
        )
    }

    pub fn load_internal_lifecycle(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<InternalLifecycleState, AppFacadeError> {
        self.load_local_store(
            service,
            |c| &mut c.internal_lifecycle,
            INTERNAL_LIFECYCLE_STATE_KEY,
            password,
        )
    }

    pub fn save_internal_lifecycle(
        &self,
        service: &mut AppService,
        lifecycle: InternalLifecycleState,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        self.save_local_store(
            service,
            |c| &mut c.internal_lifecycle,
            INTERNAL_LIFECYCLE_STATE_KEY,
            lifecycle,
            password,
        )
    }

    pub fn mutate_internal_lifecycle(
        &self,
        service: &mut AppService,
        password: Option<&str>,
        f: impl FnOnce(&mut InternalLifecycleState),
    ) -> Result<InternalLifecycleState, AppFacadeError> {
        let mut lifecycle = self.get_or_load_internal_lifecycle(service, password)?;
        f(&mut lifecycle);
        self.save_internal_lifecycle(service, lifecycle.clone(), password)?;
        Ok(lifecycle)
    }

    pub fn mutate_internal_lifecycle_for_owner(
        &self,
        expected_owner_did: &str,
        password: Option<&str>,
        f: impl FnOnce(&mut InternalLifecycleState) -> bool,
    ) -> Result<Option<InternalLifecycleState>, crate::models::FrontendError> {
        self.mutate_local_store_for_owner(
            expected_owner_did,
            |c| &mut c.internal_lifecycle,
            INTERNAL_LIFECYCLE_STATE_KEY,
            "InternalLifecycleState",
            password,
            f,
        )
    }

    pub fn mutate_internal_lifecycle_for_session(
        &self,
        token: &SessionToken,
        password: Option<&str>,
        f: impl FnOnce(&mut InternalLifecycleState) -> bool,
    ) -> Result<Option<InternalLifecycleState>, crate::models::FrontendError> {
        self.mutate_store_for_owner_inner(
            &token.owner_did,
            Some(token.generation),
            |c| &mut c.internal_lifecycle,
            INTERNAL_LIFECYCLE_STATE_KEY,
            "InternalLifecycleState",
            password,
            true,
            f,
        )
    }

    pub fn get_cached_app_config(&self) -> Result<AppConfigDto, AppFacadeError> {
        let guard = self.profile_caches.lock().unwrap();
        let caches = guard
            .as_ref()
            .ok_or_else(|| AppFacadeError::Generic("AppConfig caches not loaded".to_string()))?;
        Ok(AppConfigDto {
            ui_preferences: caches
                .ui_preferences
                .clone()
                .ok_or_else(|| AppFacadeError::Generic("UiPreferences not loaded".to_string()))?,
            security_policy: caches
                .security_policy
                .clone()
                .ok_or_else(|| AppFacadeError::Generic("SecurityPolicy not loaded".to_string()))?,
            lifecycle: caches
                .internal_lifecycle
                .clone()
                .map(Into::into)
                .ok_or_else(|| {
                    AppFacadeError::Generic("InternalLifecycleState not loaded".to_string())
                })?,
        })
    }

    // 3. Contacts Helpers
    pub fn get_cached_contacts(&self) -> Result<Vec<FrontendContact>, AppFacadeError> {
        let guard = self.profile_caches.lock().unwrap();
        let Some(caches) = guard.as_ref() else {
            return Err(AppFacadeError::Generic(
                "Contacts cache not initialized".to_string(),
            ));
        };
        if let Some(book) = caches.contacts.as_ref() {
            let mut contacts: Vec<FrontendContact> = book.contacts.values().cloned().collect();
            for c in contacts.iter_mut() {
                enrich_contact_with_wot(c, caches.wot.as_ref());
            }
            contacts.sort_by(|a, b| {
                let name_a = a.profile.first_name.as_deref().unwrap_or("");
                let name_b = b.profile.first_name.as_deref().unwrap_or("");
                name_a.to_lowercase().cmp(&name_b.to_lowercase())
            });
            Ok(contacts)
        } else {
            Err(AppFacadeError::Generic(
                "Contacts cache not initialized".to_string(),
            ))
        }
    }

    pub fn load_contacts(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<Vec<FrontendContact>, AppFacadeError> {
        info!("Fetching all contacts from address book from disk...");
        let address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password).map_err(|e| {
                error!("Failed to load address book: {:?}", e);
                e
            })?;

        self.set_cached_contacts_for_service(service, Some(address_book.clone()));

        let wot_opt = self.cache_wot();
        let mut contacts: Vec<FrontendContact> = address_book.contacts.into_values().collect();
        for c in contacts.iter_mut() {
            enrich_contact_with_wot(c, wot_opt.as_ref());
            c.syncs.truncate(2);
        }
        contacts.sort_by(|a, b| {
            let name_a = a.profile.first_name.as_deref().unwrap_or("");
            let name_b = b.profile.first_name.as_deref().unwrap_or("");
            name_a.to_lowercase().cmp(&name_b.to_lowercase())
        });
        Ok(contacts)
    }

    pub fn save_contact(
        &self,
        service: &mut AppService,
        contact: FrontendContact,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        self.save_contact_with_options(service, contact, password, false)
    }

    pub fn save_contact_with_options(
        &self,
        service: &mut AppService,
        mut contact: FrontendContact,
        password: Option<&str>,
        preserve_manual_card: bool,
    ) -> Result<(), AppFacadeError> {
        let validated_did = human_money_core::ValidatedUserId::parse(&contact.did)
            .map_err(|e| AppFacadeError::Generic(format!("Invalid contact DID: {}", e)))?;
        contact.did = validated_did.into_string();
        let mut address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)?;

        let can_did = canonicalize_did(&contact.did)
            .map_err(|e| AppFacadeError::Generic(format!("Invalid contact DID: {}", e)))?;

        let own_ids: Option<(String, String)> = service
            .with_wallet(|w| {
                let uid = w.get_user_id().trim().to_string();
                let can = canonicalize_did(&uid).unwrap_or_else(|_| uid.clone());
                (uid, can)
            })
            .ok();

        if own_ids
            .as_ref()
            .is_some_and(|(own_uid, _)| contact.did == *own_uid)
        {
            return Err(AppFacadeError::Generic(
                "Cannot add your own active profile as a contact".to_string(),
            ));
        }

        let is_own_root = own_ids
            .as_ref()
            .is_some_and(|(_, own_can)| *own_can == can_did);

        if is_own_root && contact.trust_level.is_some() {
            return Err(AppFacadeError::Generic(
                "Self trust forbidden: cannot assign a trust level to your own subaccount"
                    .to_string(),
            ));
        }

        // Keep distinct prefixes separate for both own subaccounts and external contacts;
        // each prefix can carry its own sync endpoints while sharing root WoT trust.
        let keys_to_remove: Vec<String> = address_book
            .contacts
            .keys()
            .filter(|k| *k != &contact.did && k.trim() == contact.did)
            .cloned()
            .collect();

        let is_existing =
            address_book.contacts.contains_key(&contact.did) || !keys_to_remove.is_empty();

        let mut old_contact_syncs: Vec<crate::models::FrontendSyncDescriptor> = address_book
            .contacts
            .get(&contact.did)
            .map(|c| c.syncs.clone())
            .unwrap_or_default();

        let mut existing_profile: Option<crate::models::FrontendUserProfile> = address_book
            .contacts
            .get(&contact.did)
            .map(|c| c.profile.clone());

        if let Some(old) = address_book.contacts.get(&contact.did) {
            if contact.added_at.is_empty() {
                contact.added_at = old.added_at.clone();
            }
            if !contact.has_peer_list && old.has_peer_list {
                contact.has_peer_list = true;
                contact.peer_list_updated_at =
                    contact.peer_list_updated_at.or(old.peer_list_updated_at);
            }
        }

        for k in keys_to_remove {
            if let Some(old) = address_book.contacts.remove(&k) {
                if old_contact_syncs.is_empty() {
                    old_contact_syncs = old.syncs;
                }
                if contact.added_at.is_empty() {
                    contact.added_at = old.added_at;
                }
                if !contact.has_peer_list && old.has_peer_list {
                    contact.has_peer_list = true;
                    contact.peer_list_updated_at =
                        contact.peer_list_updated_at.or(old.peer_list_updated_at);
                }
                if existing_profile.is_none() {
                    existing_profile = Some(old.profile);
                }
            }
        }

        if let Some(ref old_prof) = existing_profile {
            if contact.profile.protocol_version.is_none() {
                contact.profile.protocol_version = old_prof.protocol_version.clone();
            }
            if contact.profile.id.is_none() {
                contact.profile.id = old_prof.id.clone();
            }
            if contact.profile.community.is_none() {
                contact.profile.community = old_prof.community.clone();
            }
            if contact.profile.address.is_none() {
                contact.profile.address = old_prof.address.clone();
            }
            if contact.profile.gender.is_none() {
                contact.profile.gender = old_prof.gender.clone();
            }
            if contact.profile.email.is_none() {
                contact.profile.email = old_prof.email.clone();
            }
            if contact.profile.phone.is_none() {
                contact.profile.phone = old_prof.phone.clone();
            }
            if contact.profile.coordinates.is_none() {
                contact.profile.coordinates = old_prof.coordinates.clone();
            }
            if contact.profile.url.is_none() {
                contact.profile.url = old_prof.url.clone();
            }
            if contact.profile.service_offer.is_none() {
                contact.profile.service_offer = old_prof.service_offer.clone();
            }
            if contact.profile.needs.is_none() {
                contact.profile.needs = old_prof.needs.clone();
            }
            if contact.profile.picture_url.is_none() {
                contact.profile.picture_url = old_prof.picture_url.clone();
            }
        }

        // Authoritative sync endpoints:
        // For existing contacts, contact.syncs passed from caller/UI is authoritative.
        // We do NOT revive old sync endpoints.
        // For new contacts, if contact.syncs is empty, populate from CardStore if a card exists.
        if is_existing {
            contact.syncs = merge_contact_syncs(&[], &contact.syncs);
            contact.syncs.truncate(2);
        } else {
            let card_syncs: Vec<crate::models::FrontendSyncDescriptor> =
                match load_encrypted_json_or_default::<humoco_market::CardStore>(
                    service,
                    MARKET_CARD_STORE_KEY,
                    password,
                ) {
                    Ok(store) => store
                        .cards
                        .get(&contact.did)
                        .map(|sc| {
                            sc.card
                                .payload
                                .drops
                                .iter()
                                .cloned()
                                .map(Into::into)
                                .collect()
                        })
                        .unwrap_or_default(),
                    Err(e) => {
                        log::warn!("Failed to load CardStore during save_contact: {}", e);
                        Vec::new()
                    }
                };

            let mut all_syncs = Vec::new();
            all_syncs.extend(contact.syncs);
            all_syncs.extend(card_syncs);
            contact.syncs = merge_contact_syncs(&[], &all_syncs);
            contact.syncs.truncate(2);
        }

        // Synchronize TrustRegister if wallet is unlocked (skip for own subaccounts: self-trust is forbidden)
        if is_own_root {
            contact.trust_level = None;
            contact.desired_hops = None;
        } else if service.with_wallet(|_| ()).is_ok() {
            let mut reg = self.get_or_load_wot_register(service, password)?;
            let now_secs = Utc::now().timestamp() as u64;
            let prev_level = reg.own_contacts.get(&can_did).map(|e| e.level).unwrap_or(0);
            if !is_existing && contact.trust_level.is_none() {
                if let Some(existing_entry) = reg.own_contacts.get(&can_did) {
                    contact.trust_level = Some(existing_entry.level);
                    contact.desired_hops = Some(existing_entry.desired_hops);
                }
            }
            let new_eff_level = contact.trust_level.unwrap_or(0);

            if let Some(level) = contact.trust_level {
                if level > 3 {
                    return Err(AppFacadeError::Generic(format!(
                        "Invalid trust level: {}",
                        level
                    )));
                }
                let hops = if level == 0 {
                    0
                } else {
                    reg.policy.own_list_max_hops
                };
                reg.set_contact_trust(&can_did, level, hops, now_secs)
                    .map_err(|e| AppFacadeError::Generic(e.to_string()))?;
                if level == 0 {
                    self.prune_local_data_on_trust_withdrawal(
                        service,
                        &mut reg,
                        Some(&can_did),
                        password,
                        preserve_manual_card,
                    );
                } else {
                    reg.prune_unreachable_lists();
                }
                self.ensure_wot_list_signed(service, &mut reg)?;
                self.save_wot_register(service, &reg, password)?;
                contact.trust_level = Some(level);
                contact.desired_hops = Some(hops);
            } else {
                let removed = reg
                    .remove_contact_trust(&can_did)
                    .map_err(|e| AppFacadeError::Generic(e.to_string()))?;
                if removed {
                    self.prune_local_data_on_trust_withdrawal(
                        service,
                        &mut reg,
                        Some(&can_did),
                        password,
                        preserve_manual_card,
                    );
                    if reg.own_list_dirty {
                        self.ensure_wot_list_signed(service, &mut reg)?;
                    }
                    self.save_wot_register(service, &reg, password)?;
                }
            }

            if new_eff_level < prev_level {
                crate::services::market_service::bump_own_card_version_for_eviction(
                    self, service, password,
                );
            }
        }

        let removed_endpoints: Vec<crate::models::FrontendSyncDescriptor> = old_contact_syncs
            .into_iter()
            .filter(|old_ep| {
                !contact.syncs.iter().any(|new_ep| {
                    endpoints_overlap(
                        &old_ep.read_url,
                        &old_ep.write_url,
                        &new_ep.read_url,
                        &new_ep.write_url,
                    )
                })
            })
            .collect();

        if !is_own_root {
            for sibling in address_book.contacts.values_mut() {
                if canonicalize_did(&sibling.did).ok().as_deref() == Some(can_did.as_str()) {
                    sibling.trust_level = contact.trust_level;
                    sibling.desired_hops = contact.desired_hops;
                }
            }
        }

        address_book.contacts.insert(contact.did.clone(), contact);

        save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password)
            .map_err(|e| AppFacadeError::Generic(format!("Failed to save address book: {}", e)))?;

        self.set_cached_contacts_for_service(service, Some(address_book.clone()));
        self.prune_orphaned_market_selected_syncs(
            service,
            &address_book,
            &removed_endpoints,
            password,
        );
        Ok(())
    }

    pub fn delete_contact(
        &self,
        service: &mut AppService,
        did: &str,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        self.delete_contact_with_options(service, did, password, false)
    }

    pub fn delete_contact_with_options(
        &self,
        service: &mut AppService,
        did: &str,
        password: Option<&str>,
        preserve_manual_card: bool,
    ) -> Result<(), AppFacadeError> {
        let did = did.trim();
        let mut address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)?;

        let can_target = canonicalize_did(did).ok();
        let own_can_did = service
            .with_wallet(|w| canonicalize_did(w.get_user_id()).ok())
            .ok()
            .flatten();
        let is_own_root = can_target.is_some() && own_can_did == can_target;

        let keys_to_remove: Vec<String> = address_book
            .contacts
            .keys()
            .filter(|k| k.trim() == did)
            .cloned()
            .collect();

        let mut removed_endpoints = Vec::new();
        let mut changed = false;
        for k in keys_to_remove {
            if let Some(removed_contact) = address_book.contacts.remove(&k) {
                removed_endpoints.extend(removed_contact.syncs);
                changed = true;
            }
        }

        if changed {
            save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password).map_err(
                |e| AppFacadeError::Generic(format!("Failed to save address book: {}", e)),
            )?;

            self.set_cached_contacts_for_service(service, Some(address_book.clone()));
            self.prune_orphaned_market_selected_syncs(
                service,
                &address_book,
                &removed_endpoints,
                password,
            );
        }

        let has_remaining_sibling = can_target.as_ref().is_some_and(|ct| {
            address_book
                .contacts
                .keys()
                .any(|k| canonicalize_did(k).ok().as_ref() == Some(ct))
        });

        if !is_own_root && !has_remaining_sibling && service.with_wallet(|_| ()).is_ok() {
            if let Ok(mut reg) = self.get_or_load_wot_register(service, password) {
                let target_key = can_target.as_deref().unwrap_or(did);
                if let Ok(removed) = reg.remove_contact_trust(target_key) {
                    if removed {
                        self.prune_local_data_on_trust_withdrawal(
                            service,
                            &mut reg,
                            Some(target_key),
                            password,
                            preserve_manual_card,
                        );
                        if reg.own_list_dirty {
                            self.ensure_wot_list_signed(service, &mut reg)?;
                        }
                        self.save_wot_register(service, &reg, password)?;
                    }
                }
            }
        }

        Ok(())
    }

    // 4. History & Outgoing Bundle Helpers
    pub fn get_cached_history(&self) -> Result<Vec<TransactionRecord>, AppFacadeError> {
        self.get_cached_store(|c| &mut c.history, "History cache")
    }

    /// Returns cached transaction history with `"sent"` records hydrated from `outgoing_bundles`
    /// so UI retry/download paths receive exact bytes without storing large payloads inside `tx_history`.
    pub fn get_hydrated_history(&self) -> Result<Vec<TransactionRecord>, AppFacadeError> {
        let (mut history, cached_outgoing) = {
            let guard = self
                .profile_caches
                .lock()
                .map_err(|_| AppFacadeError::Generic("History cache lock poisoned".into()))?;
            let caches = guard
                .as_ref()
                .ok_or_else(|| AppFacadeError::Generic("History cache not initialized".into()))?;
            let history = caches
                .history
                .clone()
                .ok_or_else(|| AppFacadeError::Generic("History cache not initialized".into()))?;
            (history, caches.outgoing_bundles.clone())
        };

        let outgoing = match cached_outgoing {
            Some(o) => o,
            None => {
                let mut service = self.lock_service().map_err(|e| {
                    AppFacadeError::Generic(format!("Service lock unavailable: {}", e.message))
                })?;
                load_outgoing_bundles(&mut service, None)
                    .map_err(|e| AppFacadeError::Generic(e.message))?
            }
        };

        for record in &mut history {
            if record.direction == "sent"
                && record.bundle_data.is_empty()
                && !record.bundle_id.is_empty()
            {
                if let Some(entry) = outgoing.bundles.get(&record.bundle_id) {
                    record.bundle_data = entry.bundle_bytes.clone();
                }
            }
        }
        Ok(history)
    }

    pub fn load_history_from_disk(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<Vec<TransactionRecord>, AppFacadeError> {
        let records: Vec<TransactionRecord> =
            load_encrypted_json_or_default(service, TRANSACTION_HISTORY_KEY, password).map_err(
                |e| {
                    error!("Failed to load transaction history: {}", e);
                    e
                },
            )?;
        self.write_cache_slot_for_service(service, |c| &mut c.history, Some(records.clone()));
        Ok(records)
    }

    pub fn save_history(
        &self,
        service: &mut AppService,
        mut history: Vec<TransactionRecord>,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        // Keep `transaction_history` metadata-only on disk; outgoing bundle bytes live in `OUTGOING_BUNDLES_KEY`.
        for record in &mut history {
            record.bundle_data.clear();
        }
        self.save_store(
            service,
            |c| &mut c.history,
            TRANSACTION_HISTORY_KEY,
            history,
            password,
        )
    }

    pub fn append_to_history(
        &self,
        service: &mut AppService,
        record: TransactionRecord,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        let mut history = self.load_history_from_disk(service, password)?;
        if !record.bundle_id.is_empty()
            && history
                .iter()
                .any(|r| r.bundle_id == record.bundle_id && r.direction == record.direction)
        {
            return Ok(());
        }
        history.push(record);
        self.save_history(service, history, password)
    }

    /// Atomically persists outgoing `(TransactionRecord, bundle_bytes)` pairs across
    /// `(OUTGOING_BUNDLES_KEY, TRANSACTION_HISTORY_KEY)` in a single `WalletTransaction` batch.
    pub fn record_outgoing_transactions_with_batch(
        &self,
        service: &mut AppService,
        entries: Vec<(TransactionRecord, Vec<u8>)>,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        if entries.is_empty() {
            return Ok(());
        }
        let baseline_history = self.load_history_from_disk(service, password)?;
        let baseline_outgoing = load_outgoing_bundles(service, password)
            .map_err(|e| AppFacadeError::Generic(e.message))?;
        let batch = prepare_outgoing_history_and_bundle_companions(
            &baseline_history,
            &baseline_outgoing,
            entries,
        )
        .map_err(|e| AppFacadeError::Json(e.message))?;
        let refs: Vec<(&str, &[u8])> = batch
            .blobs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        service.save_encrypted_batch(&refs, password)?;
        self.write_cache_slot_for_service(service, |c| &mut c.history, Some(batch.history));
        self.write_cache_slot_for_service(
            service,
            |c| &mut c.outgoing_bundles,
            Some(batch.outgoing),
        );
        Ok(())
    }

    // 5. Events Helpers
    pub fn refresh_events_cache(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        info!("Refreshing events cache...");
        let events = service.get_event_history(0, 50, password)?;
        self.write_cache_slot_for_service(service, |c| &mut c.events, Some(events));
        Ok(())
    }

    // 6. Logout Helpers
    pub fn clear_all_caches(&self) {
        info!("Clearing all caches on logout...");
        let mut guard = self.profile_caches.lock().unwrap();
        let next = self
            .session_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .saturating_add(1);
        let _ = self.session_gen_tx.send(next);
        *guard = None;
        self.clear_one_shot_pin_grant();
    }

    /// Central session initializer — called after Login, Create Profile, Recovery, and Handover.
    /// Loads all caches (Settings, History, Events, Contacts, WoT) and starts the session timer.
    pub fn initialize_profile_session(
        &self,
        service: &mut AppService,
        password: &str,
    ) -> Result<(), AppFacadeError> {
        self.initialize_profile_session_inner(service, password, false)
    }

    /// Central session initializer for secondary (PIN / Cashier) logins.
    pub fn initialize_profile_session_with_secondary(
        &self,
        service: &mut AppService,
        secondary_secret: &str,
    ) -> Result<(), AppFacadeError> {
        self.initialize_profile_session_inner(service, secondary_secret, true)
    }

    fn initialize_profile_session_inner(
        &self,
        service: &mut AppService,
        credential: &str,
        is_secondary: bool,
    ) -> Result<(), AppFacadeError> {
        let res = self.initialize_profile_session_fallible(service, credential, is_secondary);
        if let Err(err) = res {
            service.logout();
            self.clear_all_caches();
            self.clear_sync_cooldown();
            return Err(err);
        }
        Ok(())
    }

    fn initialize_profile_session_fallible(
        &self,
        service: &mut AppService,
        credential: &str,
        is_secondary: bool,
    ) -> Result<(), AppFacadeError> {
        info!("Initializing profile session and member caches...");
        let owner_did = service.with_wallet(|w| w.get_user_id().to_string())?;
        {
            let mut guard = self.profile_caches.lock().unwrap();
            let next = self
                .session_generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                .saturating_add(1);
            let _ = self.session_gen_tx.send(next);
            *guard = Some(ActiveProfileCaches::new(next, owner_did));
        }
        self.clear_one_shot_pin_grant();

        // 1. Load or create split configuration stores
        let _ui_prefs = self.load_ui_preferences(service, Some(credential))?;
        let security_policy = self.load_security_policy(service, Some(credential))?;
        let _lifecycle = self.load_internal_lifecycle(service, Some(credential))?;

        // 2. Start session timer if configured
        if security_policy.session_timeout_seconds > 0 {
            if is_secondary {
                service.unlock_secondary_session(
                    credential,
                    security_policy.session_timeout_seconds,
                )?;
            } else {
                service.unlock_session(credential, security_policy.session_timeout_seconds)?;
            }
        }

        // 3. Load transaction history + canonical outgoing bundles and prune expired retained bundles
        let mut history = self.load_history_from_disk(service, Some(credential))?;
        let mut outgoing = load_outgoing_bundles(service, Some(credential))
            .map_err(|e| AppFacadeError::Generic(e.message))?;
        let retention_duration = Duration::days(BUNDLE_RETENTION_DAYS);
        let now = Utc::now();

        let mut history_changed = false;
        for record in &mut history {
            if !record.bundle_data.is_empty() {
                record.bundle_data.clear();
                history_changed = true;
            }
        }
        let prev_outgoing_len = outgoing.bundles.len();
        outgoing.bundles.retain(|_, entry| {
            entry
                .created_at
                .parse::<chrono::DateTime<Utc>>()
                .map(|ts| now.signed_duration_since(ts) <= retention_duration)
                .unwrap_or(true)
        });
        let outgoing_changed = outgoing.bundles.len() != prev_outgoing_len;

        if history_changed || outgoing_changed {
            info!("Saving pruned outgoing bundles / transaction history back to disk...");
            let outgoing_bytes = serde_json::to_vec(&outgoing).map_err(|e| {
                AppFacadeError::Json(format!("Failed to serialize outgoing_bundles: {}", e))
            })?;
            let history_bytes = serde_json::to_vec(&history).map_err(|e| {
                AppFacadeError::Json(format!("Failed to serialize transaction_history: {}", e))
            })?;
            service.save_encrypted_batch(
                &[
                    (OUTGOING_BUNDLES_KEY, &outgoing_bytes),
                    (TRANSACTION_HISTORY_KEY, &history_bytes),
                ],
                Some(credential),
            )?;
            self.write_cache_slot_for_service(service, |c| &mut c.history, Some(history));
        }
        self.write_cache_slot_for_service(service, |c| &mut c.outgoing_bundles, Some(outgoing));

        // 4. Pre-cache the last 50 wallet events
        self.refresh_events_cache(service, Some(credential))?;

        // 5. Load the address book
        if let Err(e) = self.load_contacts(service, Some(credential)) {
            log::warn!("Failed to pre-load contacts during session init: {}", e);
        }

        // 6. Load the Web of Trust register
        self.load_wot_register(service, Some(credential))?;

        // 7. Pre-cache Social Recovery owner state and guardian vault (supports timeout = 0)
        match load_encrypted_json_opt::<crate::models::recovery::RecoveryOwnerState>(
            service,
            RECOVERY_OWNER_STATE_KEY,
            Some(credential),
        ) {
            Ok(owner_opt) => {
                self.write_cache_slot_for_service(
                    service,
                    |c| &mut c.recovery_owner,
                    Some(owner_opt),
                );
            }
            Err(e) => {
                log::warn!("Failed to pre-load recovery owner state: {}", e);
            }
        }
        match load_encrypted_json_or_default::<humoco_market::GuardianVaultStore>(
            service,
            RECOVERY_GUARDIAN_VAULT_KEY,
            Some(credential),
        ) {
            Ok(vault) => {
                self.write_cache_slot_for_service(service, |c| &mut c.guardian_vault, Some(vault));
            }
            Err(e) => {
                log::warn!("Failed to pre-load guardian vault: {}", e);
            }
        }

        // 8. Pre-cache own market profile (supports timeout = 0)
        match load_encrypted_json_or_default::<crate::models::StoredOwnMarketProfile>(
            service,
            MARKET_OWN_PROFILE_KEY,
            Some(credential),
        ) {
            Ok(own_profile) => {
                self.write_cache_slot_for_service(
                    service,
                    |c| &mut c.own_market_profile,
                    Some(own_profile),
                );
            }
            Err(e) => {
                log::warn!("Failed to pre-load own market profile: {}", e);
            }
        }

        // 9. Pre-cache app notices (supports timeout = 0)
        match load_encrypted_json_or_default::<crate::models::StoredAppNotices>(
            service,
            APP_NOTICES_KEY,
            Some(credential),
        ) {
            Ok(notices) => {
                self.write_cache_slot_for_service(service, |c| &mut c.app_notices, Some(notices));
            }
            Err(e) => {
                log::warn!("Failed to pre-load app notices: {}", e);
            }
        }

        Ok(())
    }

    // 7. Web of Trust (WoT) Helpers
    pub fn prune_local_data_on_trust_withdrawal(
        &self,
        service: &mut AppService,
        reg: &mut humoco_wot::TrustRegister,
        revoked_did: Option<&str>,
        password: Option<&str>,
        preserve_manual_card: bool,
    ) {
        let can_revoked = revoked_did.and_then(|d| canonicalize_did(d).ok());
        if let Some(ref can) = can_revoked {
            reg.raw_list_store.remove(can);
            reg.invalidate_graph_cache();
        }
        reg.prune_unreachable_lists();
        reg.clear_drop_sent_log(None);

        match load_encrypted_json_or_default::<humoco_market::CardStore>(
            service,
            MARKET_CARD_STORE_KEY,
            password,
        ) {
            Ok(mut store) => {
                let before = store.cards.len();
                if !preserve_manual_card {
                    if let Some(ref can) = can_revoked {
                        store.cards.retain(|_, sc| {
                            canonicalize_did(&sc.card.payload.did).ok().as_deref()
                                != Some(can.as_str())
                        });
                    }
                }
                store.cards.retain(|_, sc| {
                    if sc.hops < 2 {
                        return true;
                    }
                    if preserve_manual_card
                        && can_revoked.as_deref().is_some_and(|rev| {
                            canonicalize_did(&sc.card.payload.did).ok().as_deref() == Some(rev)
                        })
                    {
                        return true;
                    }
                    reg.evaluate_did(&sc.card.payload.did, None)
                        .is_ok_and(|ev| ev.shortest_hops.is_some())
                });
                if store.cards.len() != before {
                    let _ = save_encrypted_json(service, MARKET_CARD_STORE_KEY, &store, password);
                }
            }
            Err(e) => {
                log::warn!(
                    "Failed to load CardStore during trust withdrawal prune: {}",
                    e
                );
            }
        }
    }

    pub fn ensure_wot_list_signed(
        &self,
        service: &mut AppService,
        reg: &mut humoco_wot::TrustRegister,
    ) -> Result<bool, AppFacadeError> {
        let now_secs = Utc::now().timestamp() as u64;
        service.with_wallet_and_identity(|_wallet, identity| {
            let signer = DidWotSigner::from_identity(identity);
            reg.ensure_own_list_signed(&signer, now_secs).map_err(|e| {
                AppFacadeError::Generic(format!("Failed to sign WoT trust list: {}", e))
            })
        })?
    }

    pub fn load_wot_register(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<humoco_wot::TrustRegister, AppFacadeError> {
        info!("Loading WoT register from disk...");
        let own_user_id = service.with_wallet(|w| w.get_user_id().to_string())?;
        let own_can_did = canonicalize_did(&own_user_id)
            .map_err(|e| AppFacadeError::Generic(format!("Invalid own wallet DID: {}", e)))?;
        let mut reg = match load_encrypted_json_opt::<humoco_wot::TrustRegister>(
            service,
            WOT_DATA_NAME,
            password,
        )? {
            Some(loaded) => loaded,
            None => {
                info!("No WoT register found, creating new one");
                humoco_wot::TrustRegister::new(&own_can_did, humoco_wot::WotPolicy::default())
                    .map_err(|e| {
                        AppFacadeError::Generic(format!(
                            "Failed to initialize TrustRegister: {}",
                            e
                        ))
                    })?
            }
        };

        reg.policy.own_list_max_hops = reg.policy.own_list_max_hops.clamp(1, 3);
        reg.policy.max_eval_depth = reg.policy.max_eval_depth.max(5);
        reg.policy.max_import_depth = reg.policy.max_import_depth.max(5);
        for entry in reg.own_contacts.values_mut() {
            if entry.level >= 1 && entry.desired_hops != reg.policy.own_list_max_hops {
                entry.desired_hops = reg.policy.own_list_max_hops;
                reg.own_list_dirty = true;
            }
        }

        self.write_cache_slot_for_service(service, |c| &mut c.wot, Some(reg.clone()));
        Ok(reg)
    }

    pub fn get_or_load_wot_register(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<humoco_wot::TrustRegister, AppFacadeError> {
        if let Some(reg) = self.cache_wot() {
            return Ok(reg);
        }
        self.load_wot_register(service, password)
    }

    pub fn save_wot_register(
        &self,
        service: &mut AppService,
        reg: &humoco_wot::TrustRegister,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        save_encrypted_json(service, WOT_DATA_NAME, reg, password)
            .map_err(|e| AppFacadeError::Generic(format!("Failed to save TrustRegister: {}", e)))?;
        self.write_cache_slot_for_service(service, |c| &mut c.wot, Some(reg.clone()));
        Ok(())
    }

    pub fn evaluate_voucher_trust_internal(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        creator_id: &str,
        guarantor_ids: &[&str],
    ) -> crate::models::FrontendVoucherTrustDto {
        use crate::models::FrontendVoucherTrustDto;
        use human_money_core::models::conflict::TrustStatus;

        let own_did_can = canonicalize_did(wallet.get_user_id()).ok();
        let creator_can = canonicalize_did(creator_id).ok();

        if let (Some(own), Some(creator)) = (&own_did_can, &creator_can) {
            if own == creator {
                return FrontendVoucherTrustDto {
                    score: 1.0,
                    trust_percent: 100,
                    is_own_voucher: true,
                    shortest_hops: Some(0),
                    ..Default::default()
                };
            }
        }

        let is_offender = matches!(
            wallet.check_reputation(creator_id),
            TrustStatus::KnownOffender(_)
        );
        if is_offender {
            return FrontendVoucherTrustDto {
                score: 0.0,
                trust_percent: 0,
                is_own_voucher: false,
                blocked_by_offender: true,
                ..Default::default()
            };
        }

        let valid_guarantors: Vec<&str> = guarantor_ids
            .iter()
            .copied()
            .filter(|gid| !matches!(wallet.check_reputation(gid), TrustStatus::KnownOffender(_)))
            .collect();

        let guarantor_canonical_set: std::collections::HashSet<String> = valid_guarantors
            .iter()
            .filter_map(|gid| canonicalize_did(gid).ok())
            .collect();

        let (eval, provider_filter_names, contacts_cache) = {
            let caches_guard = self.profile_caches.lock().unwrap();
            let reg = match caches_guard.as_ref().and_then(|c| c.wot.as_ref()) {
                Some(r) => r,
                None => return FrontendVoucherTrustDto::default(),
            };

            let creator_canonical = creator_can.as_deref().unwrap_or(creator_id);
            let valid_guarantor_canonicals: Vec<String> = valid_guarantors
                .iter()
                .map(|gid| canonicalize_did(gid).unwrap_or_else(|_| gid.to_string()))
                .collect();
            let valid_guarantor_refs: Vec<&str> = valid_guarantor_canonicals
                .iter()
                .map(|s| s.as_str())
                .collect();

            let now_secs = Utc::now().timestamp() as u64;
            let eval = match reg.evaluate_voucher(
                creator_canonical,
                &valid_guarantor_refs,
                false,
                Some(now_secs),
            ) {
                Ok(e) => e,
                Err(_) => return FrontendVoucherTrustDto::default(),
            };

            let mut provider_filter_names = std::collections::HashMap::new();
            if let Some(best_path) = eval.paths.first() {
                for hop in &best_path.hops {
                    if let Ok(can) = canonicalize_did(hop) {
                        if let Some(pf) = reg.provider_filters.get(&can) {
                            provider_filter_names.insert(can, pf.payload.name.clone());
                        }
                    }
                }
            }
            let contacts_cache = caches_guard.as_ref().and_then(|c| c.contacts.clone());
            (eval, provider_filter_names, contacts_cache)
        };

        let score = eval.score;
        let trust_percent = (score * 100.0).round().clamp(0.0, 100.0) as u8;
        let shortest_hops = eval.shortest_hops;
        let additional_paths_count = eval.paths.len().saturating_sub(1);

        let contacts_ref = contacts_cache.as_ref();

        let mut primary_bridge_did = None;
        let mut primary_bridge_name = None;
        let mut via_guarantor = false;
        let mut best_path_names = Vec::new();

        let resolve_name = |did: &str| -> Option<String> {
            lookup_contact_name(contacts_ref, did).or_else(|| {
                canonicalize_did(did)
                    .ok()
                    .and_then(|can| provider_filter_names.get(&can))
                    .cloned()
            })
        };

        if let Some(best_path) = eval.paths.first() {
            let hops = &best_path.hops;
            if hops.len() >= 2 {
                let hop1 = &hops[1];
                primary_bridge_did = Some(hop1.clone());
                primary_bridge_name = resolve_name(hop1);

                if hops.len() >= 3 {
                    let penultimate = &hops[hops.len() - 2];
                    if let Ok(pen_can) = canonicalize_did(penultimate) {
                        if guarantor_canonical_set.contains(&pen_can) {
                            via_guarantor = true;
                        }
                    }
                }
            }

            if hops.len() > 2 {
                for hop in &hops[1..hops.len() - 1] {
                    let name = resolve_name(hop).unwrap_or_else(|| truncate_did(hop));
                    best_path_names.push(name);
                }
            }
        }

        FrontendVoucherTrustDto {
            score,
            trust_percent,
            is_own_voucher: false,
            blocked_by_offender: false,
            shortest_hops,
            primary_bridge_did,
            primary_bridge_name,
            additional_paths_count,
            via_guarantor,
            best_path_names,
        }
    }

    pub fn evaluate_voucher_trust(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        creator_id: &str,
        signatures: &[human_money_core::models::voucher::VoucherSignature],
    ) -> crate::models::FrontendVoucherTrustDto {
        let guarantor_ids: Vec<&str> = signatures
            .iter()
            .filter(|s| s.role.eq_ignore_ascii_case("guarantor"))
            .map(|s| s.signer_id.as_str())
            .collect();
        self.evaluate_voucher_trust_internal(wallet, creator_id, &guarantor_ids)
    }

    pub fn evaluate_voucher_trust_preview(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        creator_id: &str,
        guarantor_ids: &[String],
    ) -> crate::models::FrontendVoucherTrustDto {
        let refs: Vec<&str> = guarantor_ids.iter().map(|s| s.as_str()).collect();
        self.evaluate_voucher_trust_internal(wallet, creator_id, &refs)
    }

    pub fn try_import_piggybacked_wot(
        &self,
        service: &mut AppService,
        wot_sync_json: &str,
        bundle_sender_id: &str,
        password: Option<&str>,
    ) -> Option<usize> {
        let pkg: humoco_wot::WotSyncPackage = match serde_json::from_str(wot_sync_json) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Malformed piggybacked WoT sync package: {}", e);
                return None;
            }
        };

        let direct_sender = canonicalize_did(bundle_sender_id).ok();
        let now_secs = Utc::now().timestamp() as u64;

        let mut reg = match self.get_or_load_wot_register(service, password) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Could not load WoT register for piggybacked import: {}", e);
                return None;
            }
        };

        let stats = match reg.import_sync_package(&pkg, direct_sender.as_deref(), now_secs) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to import piggybacked WoT package: {}", e);
                return None;
            }
        };

        if reg.own_list_dirty {
            let _ = self.ensure_wot_list_signed(service, &mut reg);
        }

        if let Err(e) = self.save_wot_register(service, &reg, password) {
            log::warn!(
                "Failed to save WoT register after piggybacked import: {}",
                e
            );
        }

        Some(stats.imported + stats.updated + stats.merged)
    }

    pub fn try_import_piggybacked_contact_card(
        &self,
        service: &mut AppService,
        contact_card_json: Option<&str>,
        password: Option<&str>,
    ) {
        let json = match contact_card_json {
            Some(j) if !j.trim().is_empty() => j.trim(),
            _ => return,
        };

        let env: humoco_market::CardEnvelope = match serde_json::from_str(json) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Malformed piggybacked CardEnvelope: {}", e);
                return;
            }
        };

        let ui_prefs = match self.get_or_load_ui_preferences(service, password) {
            Ok(p) => p,
            Err(e) => {
                log::warn!(
                    "Failed to load ui_preferences for piggybacked contact card: {}",
                    e
                );
                return;
            }
        };
        let max_stored_cards = ui_prefs.max_stored_cards.clamp(100, 10_000);
        let now = Utc::now().timestamp().max(0) as u64;

        let mut store: humoco_market::CardStore =
            match load_encrypted_json_or_default(service, MARKET_CARD_STORE_KEY, password) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!(
                        "Failed to load CardStore for piggybacked contact card: {}",
                        e
                    );
                    return;
                }
            };

        let outcome = service.with_wallet(|wallet| {
            let is_offender = |did: &str| -> bool {
                use human_money_core::models::conflict::TrustStatus;
                matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_))
            };
            let own_coords = wallet
                .profile
                .coordinates
                .as_deref()
                .and_then(humoco_market::parse_coordinates_pair);
            humoco_market::accept_envelope(
                &mut store,
                &env,
                now,
                max_stored_cards,
                own_coords,
                &is_offender,
            )
        });

        match outcome {
            Ok(humoco_market::AcceptOutcome::Accepted { did, replaced }) => {
                log::info!("Accepted piggybacked contact card (replaced={})", replaced);
                if let Err(e) =
                    save_encrypted_json(service, MARKET_CARD_STORE_KEY, &store, password)
                {
                    log::warn!(
                        "Failed to save CardStore after accepting piggybacked card: {}",
                        e
                    );
                }
                if let Some(stored) = store.cards.get(&did) {
                    let card_syncs: Vec<crate::models::FrontendSyncDescriptor> = stored
                        .card
                        .payload
                        .drops
                        .iter()
                        .cloned()
                        .map(Into::into)
                        .collect();
                    let card_profile = humoco_market::extract_public_profile(&stored.card);
                    if let Err(e) = self.update_contact_from_card(
                        service,
                        &did,
                        &card_syncs,
                        Some(&card_profile),
                        password,
                    ) {
                        log::warn!(
                            "Failed to persist piggybacked contact update for {}: {}",
                            did,
                            e.message
                        );
                    }
                }
            }
            Ok(other) => {
                log::debug!("Piggybacked contact card outcome: {:?}", other);
            }
            Err(e) => {
                log::warn!(
                    "Error verifying reputation for piggybacked contact card: {}",
                    e
                );
            }
        }
    }

    pub fn update_contact_from_card(
        &self,
        service: &mut AppService,
        card_did: &str,
        card_syncs: &[crate::models::FrontendSyncDescriptor],
        card_profile: Option<&human_money_core::models::profile::PublicProfile>,
        password: Option<&str>,
    ) -> Result<bool, crate::models::FrontendError> {
        let trimmed_card_did = card_did.trim();
        if (card_syncs.is_empty() && card_profile.is_none()) || trimmed_card_did.is_empty() {
            return Ok(false);
        }

        let mut address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)
                .map_err(crate::models::FrontendError::from)?;

        if apply_contact_update_from_card(
            &mut address_book,
            trimmed_card_did,
            card_syncs,
            card_profile,
        ) {
            save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password)
                .map_err(crate::models::FrontendError::from)?;
            self.set_cached_contacts_for_service(service, Some(address_book));
            return Ok(true);
        }
        Ok(false)
    }

    pub fn record_contact_sync_health_batch(
        &self,
        service: &mut AppService,
        results: &[(String, bool)],
        now_secs: u64,
        password: Option<&str>,
    ) -> bool {
        if results.is_empty() {
            return false;
        }

        let mut address_book: FrontendAddressBook =
            match load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password) {
                Ok(ab) => ab,
                Err(e) => {
                    log::warn!(
                        "Failed to load address book in record_contact_sync_health_batch: {}",
                        e
                    );
                    return false;
                }
            };

        let mut policy_opt = match self.get_or_load_security_policy(service, password) {
            Ok(p) => Some(p),
            Err(e) => {
                log::warn!(
                    "Failed to load security policy during record_contact_sync_health_batch: {}",
                    e
                );
                None
            }
        };

        let (updated, policy_updated) = apply_contact_sync_health_batch(
            &mut address_book,
            policy_opt.as_mut(),
            results,
            now_secs,
        );

        if updated {
            let _ = save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password);
            self.set_cached_contacts_for_service(service, Some(address_book));
        }

        if let (true, Some(policy)) = (policy_updated, policy_opt) {
            let _ = self.save_security_policy(service, policy, password);
        }
        updated
    }

    pub fn prune_orphaned_market_selected_syncs(
        &self,
        service: &mut AppService,
        address_book: &FrontendAddressBook,
        removed_endpoints: &[crate::models::FrontendSyncDescriptor],
        password: Option<&str>,
    ) {
        if removed_endpoints.is_empty() {
            return;
        }

        let mut policy = match self.get_or_load_security_policy(service, password) {
            Ok(p) => p,
            Err(_) => return,
        };

        if policy.market_selected_syncs.is_empty() {
            return;
        }

        let admin_opt: Option<StoredSyncAdmin> = match load_encrypted_json_opt::<
            Option<StoredSyncAdmin>,
        >(
            service, MARKET_SYNC_ADMIN_KEY, password
        ) {
            Ok(opt) => opt.flatten(),
            Err(e) => {
                log::warn!(
                    "Failed to load {} during prune_orphaned_market_selected_syncs: {}",
                    MARKET_SYNC_ADMIN_KEY,
                    e
                );
                return;
            }
        };

        let initial_count = policy.market_selected_syncs.len();
        policy.market_selected_syncs.retain(|sel| {
            let sr = crate::services::common::normalize_sync_url(&sel.read_url);
            let sw = crate::services::common::normalize_sync_url(&sel.write_url);
            if sr.is_empty() && sw.is_empty() {
                return false;
            }

            let was_removed = removed_endpoints
                .iter()
                .any(|rem| endpoints_overlap(&rem.read_url, &rem.write_url, &sr, &sw));

            if !was_removed {
                return true;
            }

            // 1. Matches StoredSyncAdmin?
            let matches_admin = admin_opt.as_ref().is_some_and(|a| {
                endpoints_overlap(&a.descriptor.read_url, &a.descriptor.write_url, &sr, &sw)
            });

            if matches_admin {
                return true;
            }

            // 2. Matches any remaining contact's syncs?
            let matches_contact = address_book.contacts.values().any(|c| {
                c.syncs
                    .iter()
                    .any(|ep| endpoints_overlap(&ep.read_url, &ep.write_url, &sr, &sw))
            });

            if matches_contact {
                return true;
            }

            // 3. Matches any policy.public_syncs?
            policy
                .public_syncs
                .iter()
                .any(|ps| endpoints_overlap(&ps.read_url, &ps.write_url, &sr, &sw))
        });

        if policy.market_selected_syncs.len() != initial_count {
            let updated_syncs = policy.market_selected_syncs.clone();
            let _ = self.save_security_policy(service, policy, password);
            crate::services::market_service::sync_own_card_drops_on_settings_change(
                self,
                service,
                &updated_syncs,
                password,
            );
        }
    }

    pub fn update_contact_sync_health(
        &self,
        service: &mut AppService,
        endpoint_url: &str,
        success: bool,
        now_secs: u64,
        password: Option<&str>,
    ) -> bool {
        self.record_contact_sync_health_batch(
            service,
            &[(endpoint_url.to_string(), success)],
            now_secs,
            password,
        )
    }
}

pub fn canonicalize_did(input: &str) -> Result<String, humoco_wot::WotError> {
    let validated = human_money_core::ValidatedUserId::parse(input)
        .map_err(|e| humoco_wot::WotError::InvalidDid(format!("{input}: {e}")))?;
    humoco_wot::canonicalize_did(validated.canonical_did())
}

pub struct DidWotSigner {
    pub did: String,
    pub signing_key: ed25519_dalek::SigningKey,
}

impl DidWotSigner {
    pub fn from_identity(identity: &human_money_core::models::profile::UserIdentity) -> Self {
        let did = canonicalize_did(&identity.user_id).unwrap_or_else(|_| {
            identity
                .user_id
                .split_once('@')
                .map(|(_, d)| d.to_string())
                .unwrap_or_else(|| identity.user_id.clone())
        });
        Self {
            did,
            signing_key: identity.signing_key.clone(),
        }
    }
}

impl humoco_wot::WotSigner for DidWotSigner {
    fn did(&self) -> &str {
        &self.did
    }

    fn sign(&self, message: &[u8]) -> [u8; 64] {
        human_money_core::services::crypto::sign_ed25519(&self.signing_key, message).to_bytes()
    }
}

fn enrich_contact_with_wot(
    contact: &mut FrontendContact,
    wot_opt: Option<&humoco_wot::TrustRegister>,
) {
    if let Some(reg) = wot_opt {
        if let Ok(can_did) = canonicalize_did(&contact.did) {
            if reg.self_did == can_did {
                contact.trust_level = None;
                contact.desired_hops = None;
                contact.has_peer_list = false;
                contact.peer_list_updated_at = None;
                return;
            }
            contact.trust_level = reg.own_contacts.get(&can_did).map(|e| e.level);
            contact.desired_hops = reg.own_contacts.get(&can_did).map(|e| e.desired_hops);
            contact.has_peer_list = reg.raw_list_store.contains_key(&can_did);
            contact.peer_list_updated_at = reg.raw_list_store.get(&can_did).map(|l| l.iat);
        }
    }
}

fn format_contact_display_name(c: &FrontendContact) -> Option<String> {
    let first = c.profile.first_name.as_deref().unwrap_or("").trim();
    let last = c.profile.last_name.as_deref().unwrap_or("").trim();
    let name = match (first.is_empty(), last.is_empty()) {
        (false, false) => format!("{} {}", first, last),
        (false, true) => first.to_string(),
        (true, false) => last.to_string(),
        (true, true) => c
            .profile
            .organization
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string(),
    };
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn lookup_contact_name(
    contacts_map: Option<&FrontendAddressBook>,
    target_did: &str,
) -> Option<String> {
    let book = contacts_map?;
    let trimmed_target = target_did.trim();
    if let Some((_, exact_contact)) = book
        .contacts
        .iter()
        .find(|(k, _)| k.trim() == trimmed_target)
    {
        if let Some(name) = format_contact_display_name(exact_contact) {
            return Some(name);
        }
    }
    let can_target = canonicalize_did(trimmed_target).ok()?;
    for c in book.contacts.values() {
        if canonicalize_did(&c.did).ok().as_ref() == Some(&can_target) {
            if let Some(name) = format_contact_display_name(c) {
                return Some(name);
            }
        }
    }
    None
}

fn truncate_did(did: &str) -> String {
    if did.len() > 16 {
        format!("{}…{}", &did[..8], &did[did.len() - 4..])
    } else {
        did.to_string()
    }
}

pub fn sync_endpoint_key(d: &crate::models::FrontendSyncDescriptor) -> String {
    let w = crate::services::common::normalize_sync_url(&d.write_url);
    if !w.is_empty() {
        w
    } else {
        crate::services::common::normalize_sync_url(&d.read_url)
    }
}

/// Applies disclosed business card profile fields to an existing address-book contact.
/// Always updates `picture_url` when present on the card, and populates missing names
/// without overwriting custom names already set by the user.
pub fn apply_card_profile_to_contact(
    contact: &mut FrontendContact,
    card_profile: &human_money_core::models::profile::PublicProfile,
) -> bool {
    let mut changed = false;

    if let Some(ref pic) = card_profile.picture_url {
        let trimmed = pic.trim();
        if !trimmed.is_empty() && contact.profile.picture_url.as_deref() != Some(trimmed) {
            contact.profile.picture_url = Some(trimmed.to_string());
            changed = true;
        }
    }

    let has_existing_name = contact
        .profile
        .first_name
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
        || contact
            .profile
            .last_name
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
        || contact
            .profile
            .organization
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());

    if !has_existing_name {
        if let Some(ref first) = card_profile.first_name {
            if !first.trim().is_empty() {
                contact.profile.first_name = Some(first.trim().to_string());
                changed = true;
            }
        }
        if let Some(ref last) = card_profile.last_name {
            if !last.trim().is_empty() {
                contact.profile.last_name = Some(last.trim().to_string());
                changed = true;
            }
        }
        if let Some(ref org) = card_profile.organization {
            if !org.trim().is_empty() {
                contact.profile.organization = Some(org.trim().to_string());
                changed = true;
            }
        }
    }

    changed
}

impl AppState {
    pub fn load_app_notices(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> crate::models::StoredAppNotices {
        match load_encrypted_json_or_default::<crate::models::StoredAppNotices>(
            service,
            APP_NOTICES_KEY,
            password,
        ) {
            Ok(loaded) => {
                self.write_cache_slot_for_service(
                    service,
                    |c| &mut c.app_notices,
                    Some(loaded.clone()),
                );
                loaded
            }
            Err(
                AppFacadeError::SessionNotActive(_)
                | AppFacadeError::SessionExpired(_)
                | AppFacadeError::App(
                    human_money_core::error::AppError::SessionNotActive { .. }
                    | human_money_core::error::AppError::SessionExpired { .. },
                ),
            ) if password.is_none() => {
                if let Some(cached) = self.cache_app_notices() {
                    return cached;
                }
                Default::default()
            }
            Err(e) => {
                log::warn!("Failed to load {}: {}", APP_NOTICES_KEY, e);
                if password.is_none() {
                    if let Some(cached) = self.cache_app_notices() {
                        return cached;
                    }
                }
                Default::default()
            }
        }
    }

    pub fn save_app_notices(
        &self,
        service: &mut AppService,
        notices: &crate::models::StoredAppNotices,
        password: Option<&str>,
    ) -> Result<(), crate::models::FrontendError> {
        save_encrypted_json(service, APP_NOTICES_KEY, notices, password)
            .map_err(crate::models::FrontendError::from)?;
        self.write_cache_slot_for_service(service, |c| &mut c.app_notices, Some(notices.clone()));
        Ok(())
    }

    pub fn append_app_notices_with_service(
        &self,
        service: &mut AppService,
        last_sync_at: Option<u64>,
        new_items: Vec<crate::models::FrontendAppNotice>,
        password: Option<&str>,
    ) {
        if last_sync_at.is_none() && new_items.is_empty() {
            return;
        }
        let mut store: crate::models::StoredAppNotices =
            match load_encrypted_json_or_default(service, APP_NOTICES_KEY, password) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("Failed to load app notices before append: {}", e);
                    return;
                }
            };
        apply_app_notices_update(&mut store, last_sync_at, new_items);
        if let Err(e) = self.save_app_notices(service, &store, password) {
            log::warn!("Failed to persist app notices: {:?}", e);
        }
    }

    /// Updates `last_sync_at` and prepends new notices, truncating to `MAX_APP_NOTICES` (50).
    pub fn append_app_notices(
        &self,
        last_sync_at: Option<u64>,
        new_items: Vec<crate::models::FrontendAppNotice>,
        password: Option<&str>,
    ) {
        if last_sync_at.is_none() && new_items.is_empty() {
            return;
        }
        let mut service = self.service.lock().unwrap();
        self.append_app_notices_with_service(&mut service, last_sync_at, new_items, password);
    }
}

pub(crate) fn apply_app_notices_update(
    store: &mut crate::models::StoredAppNotices,
    last_sync_at: Option<u64>,
    mut new_items: Vec<crate::models::FrontendAppNotice>,
) {
    if let Some(ts) = last_sync_at {
        store.last_sync_at = Some(store.last_sync_at.map_or(ts, |prev| prev.max(ts)));
    }
    if !new_items.is_empty() {
        new_items.extend(std::mem::take(&mut store.notices));
        new_items.truncate(MAX_APP_NOTICES);
        store.notices = new_items;
    }
}

pub(crate) fn apply_contact_update_from_card(
    address_book: &mut FrontendAddressBook,
    card_did: &str,
    card_syncs: &[crate::models::FrontendSyncDescriptor],
    card_profile: Option<&human_money_core::models::profile::PublicProfile>,
) -> bool {
    let trimmed_card_did = card_did.trim();
    if (card_syncs.is_empty() && card_profile.is_none()) || trimmed_card_did.is_empty() {
        return false;
    }
    let mut updated = false;
    for (k, contact) in address_book.contacts.iter_mut() {
        if k.trim() == trimmed_card_did {
            if !card_syncs.is_empty() {
                let merged = merge_contact_syncs(&contact.syncs, card_syncs);
                if contact.syncs != merged {
                    contact.syncs = merged;
                    updated = true;
                }
            }
            if let Some(prof) = card_profile {
                if apply_card_profile_to_contact(contact, prof) {
                    updated = true;
                }
            }
        }
    }
    updated
}

pub(crate) fn apply_contact_sync_health_batch(
    address_book: &mut FrontendAddressBook,
    mut policy_opt: Option<&mut SecurityPolicy>,
    results: &[(String, bool)],
    now_secs: u64,
) -> (bool, bool) {
    if results.is_empty() {
        return (false, false);
    }
    let mut updated = false;
    for (url, success) in results {
        let norm_target = crate::services::common::normalize_sync_url(url);
        if norm_target.is_empty() {
            continue;
        }
        for contact in address_book.contacts.values_mut() {
            for sync_ep in &mut contact.syncs {
                let norm_read = crate::services::common::normalize_sync_url(&sync_ep.read_url);
                let norm_write = crate::services::common::normalize_sync_url(&sync_ep.write_url);
                if norm_read == norm_target || norm_write == norm_target {
                    if sync_ep.status == "deprecating"
                        && sync_ep.sunset_at.is_some_and(|eff| now_secs >= eff)
                    {
                        sync_ep.status = "revoked".to_string();
                    }
                    let is_revocation_state =
                        sync_ep.status == "deprecating" || sync_ep.status == "revoked";
                    if *success {
                        sync_ep.last_seen_at = Some(now_secs);
                        sync_ep.fail_count = 0;
                        if !is_revocation_state {
                            if sync_ep.status == "active" {
                                // remains active
                            } else if sync_ep.status == "unreachable" {
                                let is_selected = policy_opt
                                    .as_deref()
                                    .map(|s| {
                                        s.market_selected_syncs.iter().any(|sel| {
                                            endpoints_overlap(
                                                &sel.read_url,
                                                &sel.write_url,
                                                &norm_read,
                                                &norm_write,
                                            )
                                        })
                                    })
                                    .unwrap_or(false);
                                if is_selected {
                                    sync_ep.status = "active".to_string();
                                } else {
                                    sync_ep.status = "unverified".to_string();
                                }
                            } else if sync_ep.status.is_empty() {
                                sync_ep.status = "unverified".to_string();
                            }
                        }
                    } else {
                        sync_ep.last_error_at = Some(now_secs);
                        sync_ep.fail_count = sync_ep.fail_count.saturating_add(1);
                        if !is_revocation_state
                            && (sync_ep.status != "pending_auth" || sync_ep.fail_count >= 3)
                        {
                            sync_ep.status = "unreachable".to_string();
                        }
                    }
                    updated = true;
                }
            }
        }
    }

    let mut policy_updated = false;
    if let Some(ref mut policy) = policy_opt {
        for (url, success) in results {
            let norm_target = crate::services::common::normalize_sync_url(url);
            if norm_target.is_empty() {
                continue;
            }
            for sel in &mut policy.market_selected_syncs {
                let norm_read = crate::services::common::normalize_sync_url(&sel.read_url);
                let norm_write = crate::services::common::normalize_sync_url(&sel.write_url);
                if norm_read == norm_target || norm_write == norm_target {
                    if sel.status == "deprecating"
                        && sel.sunset_at.is_some_and(|eff| now_secs >= eff)
                    {
                        sel.status = "revoked".to_string();
                    }
                    let is_revocation_state =
                        sel.status == "deprecating" || sel.status == "revoked";
                    if *success {
                        sel.last_seen_at = Some(now_secs);
                        sel.fail_count = 0;
                        if !is_revocation_state {
                            sel.status = "active".to_string();
                        }
                    } else {
                        sel.last_error_at = Some(now_secs);
                        sel.fail_count = sel.fail_count.saturating_add(1);
                        if !is_revocation_state {
                            sel.status = "unreachable".to_string();
                        }
                    }
                    policy_updated = true;
                }
            }
        }
    }
    (updated, policy_updated)
}

pub fn merge_contact_syncs(
    existing: &[crate::models::FrontendSyncDescriptor],
    card_syncs: &[crate::models::FrontendSyncDescriptor],
) -> Vec<crate::models::FrontendSyncDescriptor> {
    let mut result: Vec<crate::models::FrontendSyncDescriptor> = Vec::with_capacity(2);

    let merge_entry = |res: &mut Vec<crate::models::FrontendSyncDescriptor>,
                       d: &crate::models::FrontendSyncDescriptor| {
        let w = crate::services::common::normalize_sync_url(&d.write_url);
        let r = crate::services::common::normalize_sync_url(&d.read_url);
        if w.is_empty() && r.is_empty() {
            return;
        }

        if let Some(target) = res.iter_mut().find(|existing_entry| {
            let ew = crate::services::common::normalize_sync_url(&existing_entry.write_url);
            let er = crate::services::common::normalize_sync_url(&existing_entry.read_url);
            (!w.is_empty() && !ew.is_empty() && w == ew)
                || (!r.is_empty() && !er.is_empty() && r == er)
        }) {
            if target.read_url.is_empty() && !d.read_url.is_empty() {
                target.read_url = d.read_url.clone();
            }
            if target.drop_pub.is_empty() && !d.drop_pub.is_empty() {
                target.drop_pub = d.drop_pub.clone();
            }
            if target.write_url.is_empty() && !d.write_url.is_empty() {
                target.write_url = d.write_url.clone();
            }
            if target.slot_id.is_none() && d.slot_id.is_some() {
                target.slot_id = d.slot_id.clone();
            }
            if target.status == "unverified" && d.status != "unverified" {
                target.status = d.status.clone();
                target.last_probed_at = d.last_probed_at;
                target.last_seen_at = d.last_seen_at;
                target.last_error_at = d.last_error_at;
                target.fail_count = d.fail_count;
            }
        } else if res.len() < 2 {
            res.push(d.clone());
        }
    };

    // Newest card sync endpoints take precedence, preserving priority order
    for d in card_syncs {
        merge_entry(&mut result, d);
    }
    for d in existing {
        merge_entry(&mut result, d);
    }

    result
}

pub use crate::models::SyncRevocationFields;

pub fn apply_revocation_notice<T: SyncRevocationFields>(
    ep: &mut T,
    notice: &crate::sync_client::SyncRevocationNotice,
    now_secs: u64,
) {
    *ep.status_mut() = if now_secs >= notice.effective_at {
        "revoked".to_string()
    } else {
        "deprecating".to_string()
    };
    *ep.sunset_at_mut() = Some(notice.effective_at);
    *ep.revocation_reason_mut() = notice.reason.clone();
    *ep.replacement_mut() = notice.replacement.clone().map(Into::into);
}

pub fn clear_revocation<T: SyncRevocationFields>(ep: &mut T, fallback_status: &str) -> bool {
    let is_rev_status = {
        let s = ep.status_mut().as_str();
        s == "deprecating" || s == "revoked"
    };
    if ep.sunset_at_mut().is_some()
        || ep.revocation_reason_mut().is_some()
        || ep.replacement_mut().is_some()
        || is_rev_status
    {
        if is_rev_status {
            *ep.status_mut() = fallback_status.to_string();
        }
        *ep.sunset_at_mut() = None;
        *ep.revocation_reason_mut() = None;
        *ep.replacement_mut() = None;
        true
    } else {
        false
    }
}

pub fn apply_revocation_notice_to_address_book(
    address_book: &mut crate::models::FrontendAddressBook,
    old_read_url: &str,
    notice: &crate::sync_client::SyncRevocationNotice,
    now_secs: u64,
) -> bool {
    let old_norm = crate::services::common::normalize_sync_url(old_read_url);
    let notice_pub = notice.drop_pub.trim().to_lowercase();
    let is_revoked_now = now_secs >= notice.effective_at;
    let mut updated = false;

    for contact in address_book.contacts.values_mut() {
        let syncs_len = contact.syncs.len();
        let existing_endpoints: Vec<(String, String)> = contact
            .syncs
            .iter()
            .map(|o| {
                (
                    crate::services::common::normalize_sync_url(&o.read_url),
                    crate::services::common::normalize_sync_url(&o.write_url),
                )
            })
            .collect();
        let mut idx_to_replace: Option<(usize, crate::models::FrontendSyncDescriptor)> = None;
        let mut idx_to_append: Option<crate::models::FrontendSyncDescriptor> = None;

        for (idx, ep) in contact.syncs.iter_mut().enumerate() {
            if crate::services::common::normalize_sync_url(&ep.read_url) != old_norm {
                continue;
            }
            if !ep.drop_pub.trim().eq_ignore_ascii_case(&notice_pub) {
                continue;
            }

            let prev_status = ep.status.clone();
            let prev_sunset = ep.sunset_at;
            let prev_reason = ep.revocation_reason.clone();
            let prev_rep = ep.replacement.clone();

            apply_revocation_notice(ep, notice, now_secs);
            if ep.status != prev_status
                || ep.sunset_at != prev_sunset
                || ep.revocation_reason != prev_reason
                || ep.replacement != prev_rep
            {
                updated = true;
            }

            if let Some(ref rep) = notice.replacement {
                let rep_r = crate::services::common::normalize_sync_url(&rep.read_url);
                let rep_w = crate::services::common::normalize_sync_url(&rep.write_url);
                let ep_r = crate::services::common::normalize_sync_url(&ep.read_url);
                let ep_w = crate::services::common::normalize_sync_url(&ep.write_url);

                // Anti-loop: ignore self-redirects or targets already present on this contact
                let is_self_redirect = rep_r == ep_r || (!rep_w.is_empty() && rep_w == ep_w);
                let already_present = existing_endpoints
                    .iter()
                    .any(|(or, ow)| *or == rep_r || (!rep_w.is_empty() && *ow == rep_w));

                if !is_self_redirect && !already_present {
                    let new_desc = crate::models::FrontendSyncDescriptor {
                        write_url: rep.write_url.clone(),
                        read_url: rep.read_url.clone(),
                        slot_id: None, // Force index resolution on new server until contact's new SdCard arrives
                        drop_pub: rep.drop_pub.clone(),
                        status: "unverified".to_string(),
                        ..Default::default()
                    };
                    if is_revoked_now {
                        idx_to_replace = Some((idx, new_desc));
                    } else if syncs_len < 2 {
                        idx_to_append = Some(new_desc);
                    }
                }
            }
        }

        if let Some((idx, new_desc)) = idx_to_replace {
            contact.syncs[idx] = new_desc;
            updated = true;
        } else if let Some(new_desc) = idx_to_append {
            if contact.syncs.len() < 2 {
                contact.syncs.push(new_desc);
                updated = true;
            }
        }
    }

    updated
}

#[cfg(test)]
mod tests {
    use crate::models::{FrontendContact, FrontendSyncDescriptor, FrontendUserProfile};
    use chrono::Utc;

    #[test]
    fn test_save_contact_authoritative_syncs_deletion() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_state_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::test_support::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::test_support::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let sync1 = FrontendSyncDescriptor {
            read_url: "https://sync1.example.org/sync.php".to_string(),
            write_url: "https://sync1.example.org/sync.php".to_string(),
            drop_pub: "pubkey1".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let sync2 = FrontendSyncDescriptor {
            read_url: "https://sync2.example.org/sync.php".to_string(),
            write_url: "https://sync2.example.org/sync.php".to_string(),
            drop_pub: "pubkey2".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        // 1. Initial contact with 2 sync endpoints
        let initial_contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![sync1.clone(), sync2.clone()],
        };

        state
            .save_contact(&mut service, initial_contact, Some(password))
            .unwrap();

        let loaded = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = loaded.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs.len(), 2);

        // 2. Update contact with sync2 removed (only sync1 remains)
        let updated_one_sync = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: String::new(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![sync1.clone()], // sync2 removed!
        };

        state
            .save_contact(&mut service, updated_one_sync, Some(password))
            .unwrap();

        // 3. Verify sync2 is deleted and NOT resurrected from old syncs
        let loaded2 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob2 = loaded2.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob2.syncs.len(), 1, "Deleted sync2 must not be resurrected");
        assert_eq!(bob2.syncs[0].read_url, "https://sync1.example.org/sync.php");

        // 4. Update contact with empty syncs (all syncs deleted)
        let updated_zero_syncs = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: String::new(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![], // All removed!
        };

        state
            .save_contact(&mut service, updated_zero_syncs, Some(password))
            .unwrap();

        // 5. Verify all syncs deleted
        let loaded3 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob3 = loaded3.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(
            bob3.syncs.len(),
            0,
            "All syncs must be removed authoritatively"
        );
    }

    #[test]
    fn test_save_contact_allows_clearing_last_tag() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_tag_clear_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let state = super::AppState::new(alice_service);
        let bob_did = "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2";
        let mut service = state.service.lock().unwrap();

        let with_tags = crate::models::FrontendContact {
            did: bob_did.to_string(),
            profile: crate::models::FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec!["friend".to_string(), "neighbor".to_string()],
            added_at: String::new(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        state
            .save_contact(&mut service, with_tags, Some(password))
            .unwrap();

        let cleared_tags = crate::models::FrontendContact {
            did: bob_did.to_string(),
            profile: crate::models::FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: String::new(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        state
            .save_contact(&mut service, cleared_tags, Some(password))
            .unwrap();

        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert!(
            bob.tags.is_empty(),
            "Clearing the last tag must persist empty tags instead of restoring old tags"
        );
    }

    #[test]
    fn test_record_contact_sync_health_batch() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_health_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::test_support::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::test_support::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let sync_desc = FrontendSyncDescriptor {
            read_url: "https://sync-endpoint.org/sync.php".to_string(),
            write_url: "https://sync-endpoint.org/sync.php".to_string(),
            drop_pub: "pub123".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        let contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![sync_desc],
        };
        state
            .save_contact(&mut service, contact, Some(password))
            .unwrap();

        // 1. Success update preserves "unverified" (does NOT overwrite with "active")
        let updated = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            true,
            1000,
            Some(password),
        );
        assert!(updated);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_seen_at, Some(1000));
        assert_eq!(bob.syncs[0].fail_count, 0);
        assert_eq!(
            bob.syncs[0].status, "unverified",
            "unverified status must not be overwritten with active on static read success"
        );

        // 1b. Test that "pending_auth" is also preserved on read success
        let mut bob_pending = bob.clone();
        bob_pending.syncs[0].status = "pending_auth".to_string();
        state
            .save_contact(&mut service, bob_pending, Some(password))
            .unwrap();
        let updated_pending = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            true,
            1050,
            Some(password),
        );
        assert!(updated_pending);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(
            bob.syncs[0].status, "pending_auth",
            "pending_auth status must not be overwritten with active on static read success"
        );

        // 2. Transient failure on pending_auth increments fail_count without immediately flipping to unreachable
        let updated2 = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            false,
            1100,
            Some(password),
        );
        assert!(updated2);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_error_at, Some(1100));
        assert_eq!(bob.syncs[0].fail_count, 1);
        assert_eq!(bob.syncs[0].status, "pending_auth");

        // 3. Second failure still keeps pending_auth; third consecutive failure transitions to unreachable
        let updated3 = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            false,
            1200,
            Some(password),
        );
        assert!(updated3);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_error_at, Some(1200));
        assert_eq!(bob.syncs[0].fail_count, 2);
        assert_eq!(bob.syncs[0].status, "pending_auth");

        let updated3b = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            false,
            1250,
            Some(password),
        );
        assert!(updated3b);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_error_at, Some(1250));
        assert_eq!(bob.syncs[0].fail_count, 3);
        assert_eq!(bob.syncs[0].status, "unreachable");

        // 4. Recovery success resets fail_count; since not in market_selected_syncs, transitions to "unverified"
        let updated4 = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            true,
            1300,
            Some(password),
        );
        assert!(updated4);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_seen_at, Some(1300));
        assert_eq!(bob.syncs[0].fail_count, 0);
        assert_eq!(
            bob.syncs[0].status, "unverified",
            "unreachable contact sync transitions to unverified on read success when not in market_selected_syncs"
        );

        // 5. If sync is in market_selected_syncs, unreachable transitions to "active" on recovery
        state
            .mutate_security_policy(&mut service, Some(password), |policy| {
                policy.market_selected_syncs = vec![FrontendSyncDescriptor {
                    read_url: "https://sync-endpoint.org/sync.php".to_string(),
                    write_url: "https://sync-endpoint.org/sync.php".to_string(),
                    drop_pub: "pub123".to_string(),
                    slot_id: None,
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                }];
            })
            .unwrap();

        // Mark contact as unreachable again
        let _ = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            false,
            1400,
            Some(password),
        );
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(
            contacts.iter().find(|c| c.did == bob_did).unwrap().syncs[0].status,
            "unreachable"
        );

        // Now recover: should transition to "active" because it's in market_selected_syncs
        let _ = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            true,
            1500,
            Some(password),
        );
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(
            contacts.iter().find(|c| c.did == bob_did).unwrap().syncs[0].status,
            "active"
        );
    }

    #[test]
    fn test_prune_orphaned_market_selected_syncs() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_prune_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::test_support::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::test_support::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let bob_sync = FrontendSyncDescriptor {
            read_url: "https://bob-sync.org/sync.php".to_string(),
            write_url: "https://bob-sync.org/sync.php".to_string(),
            drop_pub: "pub_bob".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        let contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![bob_sync.clone()],
        };
        state
            .save_contact(&mut service, contact.clone(), Some(password))
            .unwrap();

        // Put Bob's sync into security_policy.market_selected_syncs
        let mut policy = state
            .load_security_policy(&mut service, Some(password))
            .unwrap();
        policy.market_selected_syncs = vec![bob_sync];
        state
            .save_security_policy(&mut service, policy, Some(password))
            .unwrap();

        // 1. Updating contact to remove sync should prune it from security_policy.market_selected_syncs
        let mut updated_contact = contact.clone();
        updated_contact.syncs = vec![];
        state
            .save_contact(&mut service, updated_contact, Some(password))
            .unwrap();

        let s_after = state
            .load_security_policy(&mut service, Some(password))
            .unwrap();
        assert_eq!(
            s_after.market_selected_syncs.len(),
            0,
            "Orphaned contact sync must be pruned from market_selected_syncs on save_contact"
        );

        // 2. Add contact back with sync, add to market_selected_syncs, then delete_contact
        state
            .save_contact(&mut service, contact, Some(password))
            .unwrap();
        let mut policy2 = state
            .load_security_policy(&mut service, Some(password))
            .unwrap();
        policy2.market_selected_syncs = vec![FrontendSyncDescriptor {
            read_url: "https://bob-sync.org/sync.php".to_string(),
            write_url: "https://bob-sync.org/sync.php".to_string(),
            drop_pub: "pub_bob".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        }];
        state
            .save_security_policy(&mut service, policy2, Some(password))
            .unwrap();

        state
            .delete_contact(&mut service, &bob_did, Some(password))
            .unwrap();
        let s_after_del = state
            .load_security_policy(&mut service, Some(password))
            .unwrap();
        assert_eq!(
            s_after_del.market_selected_syncs.len(),
            0,
            "Orphaned contact sync must be pruned from market_selected_syncs on delete_contact"
        );
    }

    #[test]
    fn test_merge_contact_syncs_unprobed_distinct_write_urls() {
        let ep1 = FrontendSyncDescriptor {
            write_url: "https://sync-a.example.com/sync.php".to_string(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let ep2 = FrontendSyncDescriptor {
            write_url: "https://sync-b.example.com/sync.php".to_string(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        // Merge two unprobed syncs with distinct write URLs: neither should be dropped
        let merged =
            super::merge_contact_syncs(std::slice::from_ref(&ep1), std::slice::from_ref(&ep2));
        assert_eq!(merged.len(), 2, "Both distinct unprobed syncs must be kept");
        assert_eq!(merged[0].write_url, "https://sync-b.example.com/sync.php");
        assert_eq!(merged[1].write_url, "https://sync-a.example.com/sync.php");

        // Merge populated card sync into an existing unprobed sync with same write_url
        let populated_card = FrontendSyncDescriptor {
            write_url: "https://sync-a.example.com/sync.php/".to_string(),
            read_url: "https://sync-a.example.com/read".to_string(),
            slot_id: Some("slot123".to_string()),
            drop_pub: "pubkey_a".to_string(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let merged2 = super::merge_contact_syncs(&[ep1], &[populated_card]);
        assert_eq!(
            merged2.len(),
            1,
            "Matching write_url should be merged into single entry"
        );
        assert_eq!(merged2[0].read_url, "https://sync-a.example.com/read");
        assert_eq!(merged2[0].drop_pub, "pubkey_a");
        assert_eq!(merged2[0].slot_id, Some("slot123".to_string()));
    }

    #[test]
    fn test_apply_revocation_notice_to_address_book_grace_and_inplace_replace() {
        use std::collections::HashMap;
        let old_pub = "aa".repeat(32);
        let new_pub = "bb".repeat(32);
        let other_pub = "cc".repeat(32);

        let mut contacts = HashMap::new();
        contacts.insert(
            "did:key:zAlice".to_string(),
            FrontendContact {
                did: "did:key:zAlice".to_string(),
                profile: FrontendUserProfile::default(),
                tags: vec![],
                added_at: "100".to_string(),
                notes: None,
                trust_level: None,
                desired_hops: None,
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: "https://old.example.org/sync.php".to_string(),
                    read_url: "https://old.example.org/d-old".to_string(),
                    slot_id: Some("oldslot".to_string()),
                    drop_pub: old_pub.clone(),
                    status: "active".to_string(),
                    ..Default::default()
                }],
            },
        );
        contacts.insert(
            "did:key:zBob".to_string(),
            FrontendContact {
                did: "did:key:zBob".to_string(),
                profile: FrontendUserProfile::default(),
                tags: vec![],
                added_at: "100".to_string(),
                notes: None,
                trust_level: None,
                desired_hops: None,
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![
                    FrontendSyncDescriptor {
                        write_url: "https://old.example.org/sync.php".to_string(),
                        read_url: "https://old.example.org/d-old".to_string(),
                        slot_id: Some("bobold".to_string()),
                        drop_pub: old_pub.clone(),
                        status: "active".to_string(),
                        ..Default::default()
                    },
                    FrontendSyncDescriptor {
                        write_url: "https://keep.example.org/sync.php".to_string(),
                        read_url: "https://keep.example.org/d-keep".to_string(),
                        slot_id: Some("keep".to_string()),
                        drop_pub: other_pub.clone(),
                        status: "active".to_string(),
                        ..Default::default()
                    },
                ],
            },
        );

        let mut ab = crate::models::FrontendAddressBook { contacts };
        let notice = crate::sync_client::SyncRevocationNotice {
            drop_pub: old_pub.clone(),
            issued_at: 1000,
            effective_at: 2000,
            reason: Some("Server-Umzug".to_string()),
            replacement: Some(crate::sync_client::SyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: "https://new.example.org/d-new".to_string(),
                drop_pub: new_pub.clone(),
            }),
            signature: "00".repeat(64),
        };

        // Wrong drop_pub -> no changes (anti-hijacking)
        let mut wrong_notice = notice.clone();
        wrong_notice.drop_pub = "ff".repeat(32);
        assert!(!super::apply_revocation_notice_to_address_book(
            &mut ab,
            "https://old.example.org/d-old",
            &wrong_notice,
            1500
        ));

        // During grace period (now = 1500 < 2000):
        // Alice (1 sync) gets replacement appended with slot_id = None
        // Bob (2 syncs) keeps 2 entries, old marked deprecating with replacement metadata
        assert!(super::apply_revocation_notice_to_address_book(
            &mut ab,
            "https://old.example.org/d-old",
            &notice,
            1500
        ));
        let alice = &ab.contacts["did:key:zAlice"];
        assert_eq!(alice.syncs.len(), 2);
        assert_eq!(alice.syncs[0].status, "deprecating");
        assert_eq!(alice.syncs[1].read_url, "https://new.example.org/d-new");
        assert_eq!(alice.syncs[1].slot_id, None);

        let bob = &ab.contacts["did:key:zBob"];
        assert_eq!(bob.syncs.len(), 2);
        assert_eq!(bob.syncs[0].status, "deprecating");
        assert!(bob.syncs[0].replacement.is_some());

        // After effective_at (now = 2500 >= 2000):
        // Bob's expired entry is replaced in-place with new server (slot_id = None)
        assert!(super::apply_revocation_notice_to_address_book(
            &mut ab,
            "https://old.example.org/d-old",
            &notice,
            2500
        ));
        let bob_after = &ab.contacts["did:key:zBob"];
        assert_eq!(bob_after.syncs.len(), 2);
        assert_eq!(bob_after.syncs[0].read_url, "https://new.example.org/d-new");
        assert_eq!(bob_after.syncs[0].slot_id, None);
        assert_eq!(
            bob_after.syncs[1].read_url,
            "https://keep.example.org/d-keep"
        );
    }

    #[test]
    fn test_record_contact_sync_health_preserves_deprecating_and_revoked() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_health_rev_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::test_support::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::test_support::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let dep_ep = FrontendSyncDescriptor {
            write_url: "https://dep.example.org/sync.php".to_string(),
            read_url: "https://dep.example.org/d-dep".to_string(),
            drop_pub: "aa".repeat(32),
            status: "deprecating".to_string(),
            sunset_at: Some(2000),
            revocation_reason: Some("Umzug".to_string()),
            ..Default::default()
        };
        let rev_ep = FrontendSyncDescriptor {
            write_url: "https://rev.example.org/sync.php".to_string(),
            read_url: "https://rev.example.org/d-rev".to_string(),
            drop_pub: "bb".repeat(32),
            status: "revoked".to_string(),
            sunset_at: Some(1500),
            ..Default::default()
        };

        let contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![dep_ep.clone(), rev_ep.clone()],
        };
        state
            .save_contact(&mut service, contact, Some(password))
            .unwrap();

        state
            .mutate_security_policy(&mut service, Some(password), |policy| {
                policy.market_selected_syncs = vec![dep_ep.clone(), rev_ep.clone()];
            })
            .unwrap();

        // 1. Before sunset (now = 1800 < 2000): success=true keeps deprecating, failure=false keeps revoked
        state.record_contact_sync_health_batch(
            &mut service,
            &[
                ("https://dep.example.org/d-dep".to_string(), true),
                ("https://rev.example.org/d-rev".to_string(), false),
            ],
            1800,
            Some(password),
        );

        let contacts_1 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob_1 = contacts_1.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob_1.syncs[0].status, "deprecating");
        assert_eq!(bob_1.syncs[0].last_seen_at, Some(1800));
        assert_eq!(bob_1.syncs[1].status, "revoked");
        assert_eq!(bob_1.syncs[1].fail_count, 1);

        let policy_1 = state
            .load_security_policy(&mut service, Some(password))
            .unwrap();
        assert_eq!(policy_1.market_selected_syncs[0].status, "deprecating");
        assert_eq!(policy_1.market_selected_syncs[1].status, "revoked");

        // 2. Failure on deprecating before sunset preserves deprecating (not "unreachable")
        state.record_contact_sync_health_batch(
            &mut service,
            &[("https://dep.example.org/d-dep".to_string(), false)],
            1900,
            Some(password),
        );
        let contacts_2 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob_2 = contacts_2.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob_2.syncs[0].status, "deprecating");
        assert_eq!(bob_2.syncs[0].fail_count, 1);

        // 3. Past sunset (now = 2100 >= 2000): deprecating is promoted to revoked even on success/fail
        state.record_contact_sync_health_batch(
            &mut service,
            &[("https://dep.example.org/d-dep".to_string(), false)],
            2100,
            Some(password),
        );
        let contacts_3 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob_3 = contacts_3.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob_3.syncs[0].status, "revoked");

        let policy_3 = state
            .load_security_policy(&mut service, Some(password))
            .unwrap();
        assert_eq!(policy_3.market_selected_syncs[0].status, "revoked");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_and_delete_own_subaccounts_and_reject_self_profile_and_self_trust() {
        let temp_dir = std::env::temp_dir().join(format!(
            "humoco_subaccount_contact_test_{}",
            uuid::Uuid::new_v4()
        ));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let (own_uid, own_vk) = alice_service
            .with_wallet_and_identity(|w, id| {
                (w.get_user_id().to_string(), id.signing_key.verifying_key())
            })
            .unwrap();
        let laden_uid = human_money_core::crypto::create_user_id(&own_vk, Some("laden")).unwrap();
        let privat_uid = human_money_core::crypto::create_user_id(&own_vk, Some("privat")).unwrap();

        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        // 1. Exact own active profile must be rejected
        let self_contact = FrontendContact {
            did: own_uid.clone(),
            profile: FrontendUserProfile {
                id: Some(own_uid.clone()),
                first_name: Some("Ich".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        let self_err = state
            .save_contact(&mut service, self_contact, Some(password))
            .unwrap_err();
        assert!(self_err.to_string().contains("active profile"));

        // 2. Own subaccount with trust_level must be rejected (Self trust forbidden)
        let rated_sub = FrontendContact {
            did: laden_uid.clone(),
            profile: FrontendUserProfile {
                id: Some(laden_uid.clone()),
                first_name: Some("Mein Laden".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(1),
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        let trust_err = state
            .save_contact(&mut service, rated_sub, Some(password))
            .unwrap_err();
        assert!(trust_err.to_string().contains("Self trust forbidden"));

        // 3. Two distinct own subaccounts (trust_level: None) co-exist without overwriting each other
        let laden_contact = FrontendContact {
            did: laden_uid.clone(),
            profile: FrontendUserProfile {
                id: Some(laden_uid.clone()),
                first_name: Some("Mein Laden".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        let privat_contact = FrontendContact {
            did: privat_uid.clone(),
            profile: FrontendUserProfile {
                id: Some(privat_uid.clone()),
                first_name: Some("Mein Privat".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        state
            .save_contact(&mut service, laden_contact, Some(password))
            .unwrap();
        state
            .save_contact(&mut service, privat_contact, Some(password))
            .unwrap();

        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(
            contacts.len(),
            2,
            "Both own subaccounts must stay in address book"
        );
        assert!(contacts
            .iter()
            .any(|c| c.did == laden_uid && c.trust_level.is_none()));
        assert!(contacts
            .iter()
            .any(|c| c.did == privat_uid && c.trust_level.is_none()));

        // 4. lookup_contact_name resolves exact subaccount name rather than sibling
        let book = state.cache_contacts();
        assert_eq!(
            super::lookup_contact_name(book.as_ref(), &laden_uid).as_deref(),
            Some("Mein Laden")
        );
        assert_eq!(
            super::lookup_contact_name(book.as_ref(), &privat_uid).as_deref(),
            Some("Mein Privat")
        );

        // 5. Deleting one subaccount must leave sibling intact
        state
            .delete_contact(&mut service, &privat_uid, Some(password))
            .unwrap();
        let remaining = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].did, laden_uid);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_encrypted_json_helpers_reject_corrupt_and_locked_state() {
        use human_money_core::app_service::AppFacadeError;

        let temp_dir =
            std::env::temp_dir().join(format!("humoco_json_hardening_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.lock_service().unwrap();

        // Missing keys cleanly return defaults or initialize defaults
        let book: crate::models::FrontendAddressBook = super::load_encrypted_json_or_default(
            &mut service,
            super::CONTACTS_DATA_NAME,
            Some(password),
        )
        .unwrap();
        assert!(book.contacts.is_empty());
        assert!(state
            .load_ui_preferences(&mut service, Some(password))
            .is_ok());
        assert!(state
            .load_security_policy(&mut service, Some(password))
            .is_ok());
        assert!(state
            .load_internal_lifecycle(&mut service, Some(password))
            .is_ok());

        // Missing session / wrong auth propagates error instead of silent fallback
        assert!(matches!(
            state.load_ui_preferences(&mut service, None),
            Err(AppFacadeError::SessionNotActive(_))
        ));
        assert!(matches!(
            state.load_security_policy(&mut service, None),
            Err(AppFacadeError::SessionNotActive(_))
        ));
        assert!(matches!(
            state.load_contacts(&mut service, Some("wrong-password")),
            Err(AppFacadeError::Storage(
                human_money_core::StorageError::AuthenticationFailed
            ))
        ));

        // Corrupt JSON on disk MUST return AppFacadeError::Json (never silent default)
        for key in [
            crate::settings::UI_PREFERENCES_KEY,
            crate::settings::SECURITY_POLICY_KEY,
            super::CONTACTS_DATA_NAME,
            super::WOT_DATA_NAME,
            super::TRANSACTION_HISTORY_KEY,
        ] {
            service
                .save_encrypted_data(key, b"{corrupt-json", Some(password))
                .unwrap();
        }
        service
            .save_local_sidecar_data(
                crate::settings::INTERNAL_LIFECYCLE_STATE_KEY,
                b"{corrupt-json",
                Some(password),
            )
            .unwrap();
        assert!(matches!(
            state.load_ui_preferences(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));
        assert!(matches!(
            state.load_security_policy(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));
        assert!(matches!(
            state.load_internal_lifecycle(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));
        assert!(matches!(
            state.load_contacts(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));
        assert!(matches!(
            state.load_wot_register(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));
        assert!(matches!(
            state.load_history_from_disk(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));

        drop(service);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_generic_revocation_helpers_for_descriptor_and_public_sync() {
        let notice = crate::sync_client::SyncRevocationNotice {
            drop_pub: "aa".repeat(32),
            issued_at: 1000,
            effective_at: 2000,
            reason: Some("Umzug".to_string()),
            replacement: Some(crate::sync_client::SyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: "https://new.example.org/d".to_string(),
                drop_pub: "bb".repeat(32),
            }),
            signature: "00".repeat(64),
        };

        let mut desc = FrontendSyncDescriptor::default();
        super::apply_revocation_notice(&mut desc, &notice, 1500);
        assert_eq!(desc.status, "deprecating");
        assert_eq!(desc.sunset_at, Some(2000));
        assert_eq!(desc.revocation_reason.as_deref(), Some("Umzug"));
        assert!(desc.replacement.is_some());
        assert!(super::clear_revocation(&mut desc, "active"));
        assert_eq!(desc.status, "active");
        assert!(desc.sunset_at.is_none());
        assert!(desc.revocation_reason.is_none());
        assert!(desc.replacement.is_none());
        assert!(!super::clear_revocation(&mut desc, "active"));

        let mut pub_sync = crate::settings::StoredPublicSync::default();
        super::apply_revocation_notice(&mut pub_sync, &notice, 2500);
        assert_eq!(pub_sync.status, "revoked");
        assert_eq!(pub_sync.sunset_at, Some(2000));
        assert!(super::clear_revocation(&mut pub_sync, "active"));
        assert_eq!(pub_sync.status, "active");
        assert!(pub_sync.sunset_at.is_none());
    }

    #[test]
    fn test_split_config_stores_isolation_and_owner_checked_mutator() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_split_cfg_test_{}", uuid::Uuid::new_v4()));
        let pw = "split-cfg-pw-2026";
        let alice_svc = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let alice_did = alice_svc
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let state = crate::test_support::create_test_state(alice_svc);

        {
            let mut s = state.lock_service().unwrap();
            state
                .mutate_ui_preferences(&mut s, Some(pw), |p| {
                    p.ui_mode = "expert".to_string();
                })
                .unwrap();
            state
                .mutate_security_policy(&mut s, Some(pw), |p| {
                    p.session_timeout_seconds = 900;
                })
                .unwrap();
            state
                .mutate_internal_lifecycle(&mut s, Some(pw), |lc| {
                    lc.pending_restore_l2_reconciliation = true;
                })
                .unwrap();
        }

        // Same owner: clears flag without clobbering UiPreferences or SecurityPolicy
        let updated = state
            .mutate_internal_lifecycle_for_owner(&alice_did, Some(pw), |lc| {
                lc.pending_restore_l2_reconciliation = false;
                lc.last_sync_backup_at = Some("2026-11-19T12:00:00Z".to_string());
                true
            })
            .unwrap()
            .expect("matching owner must mutate lifecycle");
        assert!(!updated.pending_restore_l2_reconciliation);
        assert_eq!(
            updated.last_sync_backup_at.as_deref(),
            Some("2026-11-19T12:00:00Z")
        );

        let cfg = state.get_cached_app_config().unwrap();
        assert_eq!(cfg.ui_preferences.ui_mode, "expert");
        assert_eq!(cfg.security_policy.session_timeout_seconds, 900);

        // Stale owner DID: returns Ok(None) and does NOT touch store
        let stale = state
            .mutate_internal_lifecycle_for_owner("did:key:zStaleOwner", Some(pw), |lc| {
                lc.backup_dirty = true;
                true
            })
            .unwrap();
        assert!(stale.is_none());
        assert!(!state.get_cached_internal_lifecycle().unwrap().backup_dirty);

        // Logged-out / locked wallet: returns Ok(None) and does NOT touch store
        state.lock_service().unwrap().logout();
        let locked = state
            .mutate_internal_lifecycle_for_owner(&alice_did, Some(pw), |lc| {
                lc.backup_dirty = true;
                true
            })
            .unwrap();
        assert!(locked.is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_canonicalize_did_rejects_malformed_prefixes_and_checksums() {
        let kp = human_money_core::test_utils::ACTORS.bob.identity.clone();
        let root_did = human_money_core::crypto::create_user_id(&kp.public_key, None).unwrap();
        let valid_sai =
            human_money_core::crypto::create_user_id(&kp.public_key, Some("shop-1")).unwrap();

        assert_eq!(super::canonicalize_did(&root_did).unwrap(), root_did);
        assert_eq!(
            super::canonicalize_did(&format!("  {} \n", valid_sai)).unwrap(),
            root_did
        );

        let broken_variants = [
            format!("fake@{}", root_did),
            format!("shop-1:BAD@{}", root_did),
            format!(":abc@{}", root_did),
            format!("@{}", root_did),
            format!("a@b:abc@{}", root_did),
            format!("Shop:abc@{}", root_did),
            format!("{}\n{}", &root_did[..12], &root_did[12..]),
        ];
        for invalid in &broken_variants {
            assert!(
                super::canonicalize_did(invalid).is_err(),
                "canonicalize_did must reject malformed input '{}'",
                invalid
            );
        }
    }

    #[test]
    fn test_external_sibling_prefixes_keep_separate_syncs_mirror_trust_and_delete_cleanly() {
        use crate::models::{FrontendContact, FrontendSyncDescriptor, FrontendUserProfile};
        use chrono::Utc;

        let temp_dir = std::env::temp_dir().join(format!(
            "humoco_ext_sibling_prefix_{}",
            uuid::Uuid::new_v4()
        ));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let bob_kp = human_money_core::test_utils::ACTORS.bob.identity.clone();
        let bob_sk = bob_kp.signing_key.clone();
        let bob_vk = bob_sk.verifying_key();
        let bob_laden = human_money_core::crypto::create_user_id(&bob_vk, Some("laden")).unwrap();
        let bob_privat = human_money_core::crypto::create_user_id(&bob_vk, Some("privat")).unwrap();
        let bob_can = super::canonicalize_did(&bob_laden).unwrap();

        // Populate CardStore with two distinct prefix cards for Bob
        let now = Utc::now().timestamp().max(0) as u64;
        let mut card_store = humoco_market::CardStore::default();
        for (did, url) in [
            (&bob_laden, "https://laden.example.com/sync.php"),
            (&bob_privat, "https://privat.example.com/sync.php"),
        ] {
            let card = humoco_market::sign_sd_card(
                humoco_market::CardPayloadInput {
                    did: did.clone(),
                    v: 100,
                    valid_until: now + 86_400,
                    max_hops: 2,
                    ad_count: 0,
                    allow_inquiries: false,
                    drops: vec![humoco_market::DropDescriptor {
                        write_url: url.to_string(),
                        read_url: url.to_string(),
                        slot_id: None,
                        drop_pub: "a".repeat(64),
                    }],
                },
                vec![(
                    humoco_market::CardItem::Name {
                        first_name: Some("Bob".to_string()),
                        last_name: None,
                    },
                    1,
                )],
                &bob_sk,
            )
            .unwrap();
            let wire =
                humoco_market::render_wire(&card, 1, humoco_market::WireFilter::Full).unwrap();
            humoco_market::accept_envelope(
                &mut card_store,
                &humoco_market::CardEnvelope {
                    card_wire: wire,
                    hops: 1,
                },
                now,
                100,
                None,
                &|_| false,
            );
        }
        super::save_encrypted_json(
            &mut service,
            super::MARKET_CARD_STORE_KEY,
            &card_store,
            Some(password),
        )
        .unwrap();

        let mk_sync = |url: &str| FrontendSyncDescriptor {
            write_url: url.to_string(),
            read_url: format!("{}/read", url),
            drop_pub: "b".repeat(64),
            status: "active".to_string(),
            ..Default::default()
        };
        let mk_ext =
            |did: &str, name: &str, sync_url: &str, trust_level: Option<u8>| FrontendContact {
                did: did.to_string(),
                profile: FrontendUserProfile {
                    id: Some(did.to_string()),
                    first_name: Some(name.to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level,
                desired_hops: Some(2),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![mk_sync(sync_url)],
            };

        // 1. Save both external prefixes with distinct sync endpoints
        state
            .save_contact(
                &mut service,
                mk_ext(
                    &bob_laden,
                    "Bob Laden",
                    "https://sync-laden.example.com",
                    Some(2),
                ),
                Some(password),
            )
            .unwrap();
        state
            .save_contact(
                &mut service,
                mk_ext(
                    &bob_privat,
                    "Bob Privat",
                    "https://sync-privat.example.com",
                    None,
                ),
                Some(password),
            )
            .unwrap();

        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(contacts.len(), 2);
        assert_eq!(
            contacts
                .iter()
                .find(|c| c.did == bob_laden)
                .unwrap()
                .trust_level,
            Some(2)
        );
        assert_eq!(
            contacts
                .iter()
                .find(|c| c.did == bob_privat)
                .unwrap()
                .trust_level,
            Some(2)
        );
        assert_eq!(
            state
                .get_or_load_wot_register(&mut service, Some(password))
                .unwrap()
                .own_contacts
                .get(&bob_can)
                .map(|e| e.level),
            Some(2)
        );
        assert_eq!(
            contacts.iter().find(|c| c.did == bob_laden).unwrap().syncs[0].write_url,
            "https://sync-laden.example.com"
        );
        assert_eq!(
            contacts.iter().find(|c| c.did == bob_privat).unwrap().syncs[0].write_url,
            "https://sync-privat.example.com"
        );

        // 2. Updating trust on one prefix atomically mirrors trust_level to sibling on disk & cache
        state
            .save_contact(
                &mut service,
                mk_ext(
                    &bob_laden,
                    "Bob Laden",
                    "https://sync-laden.example.com",
                    Some(3),
                ),
                Some(password),
            )
            .unwrap();
        let disk_book: crate::models::FrontendAddressBook = super::load_encrypted_json_or_default(
            &mut service,
            super::CONTACTS_DATA_NAME,
            Some(password),
        )
        .unwrap();
        assert_eq!(
            disk_book.contacts.get(&bob_laden).unwrap().trust_level,
            Some(3)
        );
        assert_eq!(
            disk_book.contacts.get(&bob_privat).unwrap().trust_level,
            Some(3)
        );

        // 3. update_contact_from_card for bob_privat never mutates bob_laden syncs
        let updated = state
            .update_contact_from_card(
                &mut service,
                &bob_privat,
                &[mk_sync("https://sync-privat-2.example.com")],
                None,
                Some(password),
            )
            .unwrap();
        assert!(updated);
        let after_card = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(
            after_card
                .iter()
                .find(|c| c.did == bob_laden)
                .unwrap()
                .syncs
                .len(),
            1
        );
        assert_eq!(
            after_card
                .iter()
                .find(|c| c.did == bob_privat)
                .unwrap()
                .syncs
                .len(),
            2
        );

        // 4. Deleting bob_laden keeps bob_privat, root WoT trust, and remaining CardStore intact
        state
            .delete_contact(&mut service, &bob_laden, Some(password))
            .unwrap();
        let remaining = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].did, bob_privat);
        let reg_mid = state
            .get_or_load_wot_register(&mut service, Some(password))
            .unwrap();
        assert_eq!(reg_mid.own_contacts.get(&bob_can).map(|e| e.level), Some(3));
        let store_mid: humoco_market::CardStore = super::load_encrypted_json_or_default(
            &mut service,
            super::MARKET_CARD_STORE_KEY,
            Some(password),
        )
        .unwrap();
        assert_eq!(store_mid.cards.len(), 2);

        // 5. Deleting last sibling removes root WoT trust AND prunes ALL prefix cards of Bob's key
        state
            .delete_contact(&mut service, &bob_privat, Some(password))
            .unwrap();
        let reg_end = state
            .get_or_load_wot_register(&mut service, Some(password))
            .unwrap();
        assert!(!reg_end.own_contacts.contains_key(&bob_can));
        let store_end: humoco_market::CardStore = super::load_encrypted_json_or_default(
            &mut service,
            super::MARKET_CARD_STORE_KEY,
            Some(password),
        )
        .unwrap();
        assert!(store_end.cards.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_known_lock_heads_sidecar_persists_without_advancing_seal_nonce() {
        use ed25519_dalek::{Signer, SigningKey};
        use human_money_core::models::layer2_api::{LockReceipt, LockReceiptKind};

        let temp_dir = std::env::temp_dir().join(format!(
            "humoco_known_lock_heads_test_{}",
            uuid::Uuid::new_v4()
        ));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let seal_before = service.get_current_seal().unwrap();
        let lock_sk = SigningKey::from_bytes(&[0x11u8; 32]);
        let lock_pub = lock_sk.verifying_key().to_bytes();
        let anchor_id = [0x99u8; 32];
        let mut receipt = LockReceipt {
            kind: LockReceiptKind::FirstWrite,
            lock_pub,
            bucket: [0x22u8; 32],
            slot: [0x33u8; 32],
            child_lock: [0x44u8; 32],
            entry_hash: [0x55u8; 32],
            retain_until_quarter: 20271,
            write_seq: 1,
            bucket_head: [0x66u8; 32],
            recipient_nonce: [0x77u8; 32],
            sig: [0u8; 64],
        };
        let digest = receipt.digest();
        receipt.sig = lock_sk.sign(&digest).to_bytes();

        super::record_known_head_receipts(
            &mut service,
            &anchor_id,
            std::slice::from_ref(&receipt),
            Some(password),
        )
        .unwrap();

        let loaded =
            super::load_known_head_receipts_for_anchor(&mut service, &anchor_id, Some(password))
                .expect("valid sidecar should load");
        assert_eq!(loaded.get(&lock_pub), Some(&receipt));

        assert!(service
            .load_local_sidecar_data(super::KNOWN_LOCK_HEADS_KEY, Some(password))
            .is_ok());
        assert!(service
            .load_encrypted_data(super::KNOWN_LOCK_HEADS_KEY, Some(password))
            .is_err());

        // Corrupt JSON inside the sidecar must fail closed rather than silently returning empty map.
        service
            .save_local_sidecar_data(
                super::KNOWN_LOCK_HEADS_KEY,
                b"{corrupt-sidecar",
                Some(password),
            )
            .unwrap();
        assert!(
            super::load_known_head_receipts_for_anchor(&mut service, &anchor_id, Some(password))
                .is_err(),
            "Corrupt known-lock-heads sidecar must fail closed"
        );
        assert!(
            super::record_known_head_receipts(
                &mut service,
                &anchor_id,
                std::slice::from_ref(&receipt),
                Some(password),
            )
            .is_err(),
            "Recording against corrupt known-lock-heads sidecar must also fail closed"
        );

        let seal_after = service.get_current_seal().unwrap();
        assert_eq!(
            serde_json::to_value(&seal_after).unwrap(),
            serde_json::to_value(&seal_before).unwrap()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_profile_session_lock_cross_instance_raii_and_validation() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_session_lock_test_{}", uuid::Uuid::new_v4()));
        let lock_dir = temp_dir.join("session_locks");

        let s1 = super::AppService::new(&temp_dir.join("w1")).unwrap();
        let s2 = super::AppService::new(&temp_dir.join("w2")).unwrap();
        let state1 = super::AppState::new(s1).with_session_lock_dir(lock_dir.clone());
        let state2 = super::AppState::new(s2).with_session_lock_dir(lock_dir.clone());

        // 1. Instance 1 acquires and commits profile_a
        let pending1 = state1.acquire_profile_lock("profile_a").unwrap();
        state1.commit_profile_lock(pending1);

        // 2. Instance 2 is blocked on profile_a while Instance 1 holds the RAII lock
        let err = state2.acquire_profile_lock("profile_a").err().unwrap();
        assert_eq!(err.code, "error.auth.profileAlreadyOpen");

        // 3. Instance 2 can open profile_b in parallel
        let pending_b = state2.acquire_profile_lock("profile_b").unwrap();
        state2.commit_profile_lock(pending_b);
        state2.release_profile_lock();

        // 4. Idempotency: Instance 1 re-acquiring its own active profile succeeds without blocking
        let reacquire = state1.acquire_profile_lock("profile_a").unwrap();
        assert!(matches!(reacquire, super::PendingProfileLock::AlreadyHeld));
        state1.commit_profile_lock(reacquire);
        assert_eq!(
            state2.acquire_profile_lock("profile_a").err().unwrap().code,
            "error.auth.profileAlreadyOpen"
        );

        // 5. After Instance 1 releases, Instance 2 can acquire and RAII-drop rolls back
        state1.release_profile_lock();
        {
            let uncommitted = state2.acquire_profile_lock("profile_a").unwrap();
            assert!(matches!(
                uncommitted,
                super::PendingProfileLock::Acquired(_)
            ));
            // Dropped here without commit -> lock released immediately
        }
        let after_rollback = state1.acquire_profile_lock("profile_a").unwrap();
        state1.commit_profile_lock(after_rollback);
        state1.release_profile_lock_if_matching("profile_a");
        // K2: flock-backed .lock file stays on disk and is never unlinked while Instance 2 can acquire cleanly.
        assert!(lock_dir.join("profile_a.lock").exists());
        assert!(state2.acquire_profile_lock("profile_a").is_ok());

        // 6. Path traversal / invalid folder names are rejected
        assert!(state1.acquire_profile_lock("../escape").is_err());
        assert!(state1.acquire_profile_lock("a/b").is_err());
        assert!(state1.acquire_profile_lock("").is_err());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn h1_save_contact_preserve_manual_card_keeps_card_on_trust_reset() {
        use humoco_market::CardStore;

        let dir_alice = crate::test_support::temp_test_dir("h1_card_alice");
        let dir_bob = crate::test_support::temp_test_dir("h1_card_bob");
        let pw = "StrongPw-H1-Card!";

        let alice_state = super::AppState::new(crate::test_support::setup_test_app(
            &dir_alice,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        ));
        let bob_state = super::AppState::new(crate::test_support::setup_test_app(
            &dir_bob,
            "Bob",
            crate::test_support::BOB_PHRASE,
            pw,
        ));
        let alice_did = alice_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let prof = crate::services::market_service::save_my_market_profile_impl(
            crate::models::FrontendSaveMyMarketProfileRequest {
                profile: crate::models::FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                visibility: Default::default(),
                skills: vec![],
                max_hops: 2,
                validity_years: 2,
                allow_inquiries: true,
                published: Some(true),
            },
            Some(pw.to_string()),
            &alice_state,
        )
        .unwrap();
        let share_uri = prof.share_uri.unwrap();

        crate::services::market_service::import_contact_card_impl(
            share_uri,
            Some(pw.to_string()),
            &bob_state,
        )
        .unwrap();

        {
            let mut s = bob_state.service.lock().unwrap();
            let trusted = crate::models::FrontendContact {
                did: alice_did.clone(),
                profile: crate::models::FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: chrono::Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: None,
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            bob_state
                .save_contact_with_options(&mut s, trusted, Some(pw), true)
                .unwrap();

            let reset = crate::models::FrontendContact {
                did: alice_did.clone(),
                profile: crate::models::FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: chrono::Utc::now().to_rfc3339(),
                notes: None,
                trust_level: None,
                desired_hops: None,
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            bob_state
                .save_contact_with_options(&mut s, reset, Some(pw), true)
                .unwrap();

            let store: CardStore =
                super::load_encrypted_json_opt(&mut s, super::MARKET_CARD_STORE_KEY, Some(pw))
                    .unwrap()
                    .unwrap();
            let can_alice = super::canonicalize_did(&alice_did).unwrap();
            assert!(
                store.cards.values().any(|sc| {
                    super::canonicalize_did(&sc.card.payload.did)
                        .ok()
                        .as_deref()
                        == Some(can_alice.as_str())
                }),
                "H1: preserve_manual_card=true must keep manually imported card in CardStore"
            );
        }

        let _ = std::fs::remove_dir_all(&dir_alice);
        let _ = std::fs::remove_dir_all(&dir_bob);
    }
}
