// src-tauri/src/state/mod.rs
mod contacts_repo;
mod profile_cache;
mod session_auth;
mod storage;
#[cfg(test)]
mod tests;
mod wot_repo;

use crate::session::ActiveProfileCaches;
use human_money_core::app_service::{AppFacadeError, AppService};
use std::sync::Mutex;

use session_auth::OneShotPinGrant;

pub use crate::models::SyncRevocationFields;
pub use crate::services::sync_endpoints::sync_endpoint_key;
pub use contacts_repo::{
    apply_card_profile_to_contact, apply_revocation_notice,
    apply_revocation_notice_to_address_book, clear_revocation, is_revocation_effective,
    merge_contact_syncs,
};
pub(crate) use contacts_repo::{apply_contact_sync_health_batch, apply_contact_update_from_card};
pub(crate) use profile_cache::EncryptedBatchCommit;
pub use session_auth::{PendingProfileLock, ProfileSessionLock, RequiredAccess};
pub(crate) use storage::{apply_app_notices_update, merge_btree_map_3way, merge_sync_state_3way};
pub use storage::{
    cached_media_storage_key, get_outgoing_bundle_bytes, load_encrypted_bytes_opt,
    load_encrypted_json_opt, load_encrypted_json_or_default, load_known_head_receipts_for_anchor,
    load_local_sidecar_bytes_opt, load_local_sidecar_json_opt, load_local_sidecar_json_or_default,
    load_outgoing_bundles, own_media_storage_key, prepare_outgoing_history_and_bundle_companions,
    prepare_receive_history_companion, record_known_head_receipts, save_encrypted_bytes,
    save_encrypted_json, save_local_sidecar_bytes, save_local_sidecar_json, BackupMirrorState,
    CompanionBlobBatch, KnownLockHeadsState, OutboxMessageClass, OutgoingCompanionBatch,
    PostOutboxEntry, StoredBackupSyncState, StoredPendingTolerancePayment, StoredPostState,
    StoredPrecommitL2Evidence, StoredSyncAdmin, StoredSyncState,
};
pub use wot_repo::{canonicalize_did, DidWotSigner};

#[cfg(test)]
use contacts_repo::lookup_contact_name;
#[cfg(test)]
use wot_repo::truncate_did;

pub(in crate::state) fn app_state_err(message: String) -> AppFacadeError {
    AppFacadeError::App(human_money_core::error::AppError::StateConflict { message })
}

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    #[serde(default)]
    pub sync_members: bool,
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
        self.sync_members |= other.sync_members;
    }

    pub fn is_empty(&self) -> bool {
        !(self.seal
            || self.local_backup
            || self.sync_backup
            || self.sync_backup_force
            || self.post
            || self.market
            || self.market_force_upload
            || self.sync_members)
    }
}

#[derive(Debug, Clone)]
pub struct SyncCoordinatorControl {
    pub is_online: bool,
    pub is_wifi: bool,
    pub is_visible: bool,
    pub pending_wake: SyncWakeRequest,
    pub origin_cooldowns: std::collections::HashMap<String, std::time::Instant>,
}

impl Default for SyncCoordinatorControl {
    fn default() -> Self {
        Self {
            is_online: true,
            is_wifi: true,
            is_visible: true,
            pending_wake: SyncWakeRequest::default(),
            origin_cooldowns: std::collections::HashMap::new(),
        }
    }
}

/// Central application state shared across Tauri commands and background workers.
///
/// # Lock Ordering Invariant
/// Whenever multiple mutexes on `AppState` must be acquired simultaneously,
/// acquire them strictly in the following total order to prevent deadlocks:
/// `recovery_op_lock -> backup_op_lock -> post_op_lock -> market_op_lock -> service -> profile_caches`.
/// `active_profile_lock`, `sync_control`, and `one_shot_pin_grant` are leaf mutexes never held while acquiring `service` or across `.await`.
///
/// Never hold any `std::sync::MutexGuard` across an `.await` point.
pub struct AppState {
    pub service: Mutex<AppService>,
    pub profile_caches: Mutex<Option<ActiveProfileCaches>>,
    pub session_generation: std::sync::atomic::AtomicU64,
    pub(in crate::state) session_gen_tx: tokio::sync::watch::Sender<u64>,
    pub recovery_op_lock: tokio::sync::Mutex<()>,
    pub backup_op_lock: tokio::sync::Mutex<()>,
    pub post_op_lock: tokio::sync::Mutex<()>,
    pub market_op_lock: tokio::sync::Mutex<()>,
    pub startup_launch_items: Mutex<Vec<String>>,
    pub sync_control: Mutex<SyncCoordinatorControl>,
    pub sync_coordinator_wake: std::sync::Arc<tokio::sync::Notify>,
    pub(in crate::state) session_lock_dir: Option<std::path::PathBuf>,
    pub(in crate::state) active_profile_lock: Mutex<Option<ProfileSessionLock>>,
    pub(in crate::state) one_shot_pin_grant: Mutex<Option<OneShotPinGrant>>,
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
            backup_op_lock: tokio::sync::Mutex::new(()),
            post_op_lock: tokio::sync::Mutex::new(()),
            market_op_lock: tokio::sync::Mutex::new(()),
            startup_launch_items: Mutex::new(Vec::new()),
            sync_control: Mutex::new(SyncCoordinatorControl::default()),
            sync_coordinator_wake: std::sync::Arc::new(tokio::sync::Notify::new()),
            session_lock_dir: None,
            active_profile_lock: Mutex::new(None),
            one_shot_pin_grant: Mutex::new(None),
        }
    }
}
