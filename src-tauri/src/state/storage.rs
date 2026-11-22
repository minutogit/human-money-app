// src-tauri/src/state/storage.rs
use super::{
    app_state_err, AppState, EncryptedBatchCommit, APP_NOTICES_KEY, KNOWN_LOCK_HEADS_KEY,
    MARKET_CARD_STORE_KEY, MARKET_SYNC_KEY, MAX_APP_NOTICES, OUTGOING_BUNDLES_KEY,
    TRANSACTION_HISTORY_KEY,
};
use crate::models::TransactionRecord;
use crate::session::{ActiveProfileCaches, SessionToken};
use crate::settings::{
    InternalLifecycleState, SecurityPolicy, UiPreferences, INTERNAL_LIFECYCLE_STATE_KEY,
    SECURITY_POLICY_KEY, UI_PREFERENCES_KEY,
};
use human_money_core::app_service::{AppFacadeError, AppService};
use log::{error, info};

pub fn own_media_storage_key(image_token: &str) -> String {
    format!("market_own_media_{}", image_token)
}

pub fn cached_media_storage_key(blob_id: &str) -> String {
    format!("market_media_cache_{}", blob_id)
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
    pub processed_msg_ids: std::collections::BTreeMap<String, u64>,
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
    #[serde(default)]
    pub outbox_rotation_tick: u64,
    #[serde(default)]
    pub replied_sign_request_ids: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub pruned_outbox_tag16s: std::collections::BTreeSet<String>,
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
    pub policies: std::collections::BTreeMap<String, humoco_market::MemberPolicy>,
    pub allow: Vec<String>,
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
            let expected_bucket =
                human_money_core::humoco_proto::derive_bucket(anchor_id, &receipt.lock_pub);
            if k != expected_key || receipt.bucket != expected_bucket || !receipt.verify_signature()
            {
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
        let expected_bucket = human_money_core::humoco_proto::derive_bucket(anchor_id, &r.lock_pub);
        if r.bucket != expected_bucket || !r.verify_signature() {
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

impl AppState {
    pub fn load_card_store_or_cached(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<humoco_market::CardStore, AppFacadeError> {
        self.load_encrypted_json_or_cached(
            service,
            |c| &c.card_store,
            |c| &mut c.card_store,
            MARKET_CARD_STORE_KEY,
            password,
        )
    }

    pub fn save_card_store(
        &self,
        service: &mut AppService,
        store: &humoco_market::CardStore,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        self.save_store(
            service,
            |c| &mut c.card_store,
            MARKET_CARD_STORE_KEY,
            store.clone(),
            password,
        )
    }

    pub fn load_sync_state_or_cached(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<StoredSyncState, AppFacadeError> {
        self.load_encrypted_json_or_cached(
            service,
            |c| &c.sync_state,
            |c| &mut c.sync_state,
            MARKET_SYNC_KEY,
            password,
        )
    }

    pub fn save_sync_state(
        &self,
        service: &mut AppService,
        sync_state: &StoredSyncState,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        self.save_store(
            service,
            |c| &mut c.sync_state,
            MARKET_SYNC_KEY,
            sync_state.clone(),
            password,
        )
    }

    fn load_encrypted_json_or_cached<T>(
        &self,
        service: &mut AppService,
        read_field: fn(&ActiveProfileCaches) -> &Option<T>,
        write_field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        key: &str,
        password: Option<&str>,
    ) -> Result<T, AppFacadeError>
    where
        T: serde::de::DeserializeOwned + Default + Clone,
    {
        match load_encrypted_json_or_default::<T>(service, key, password) {
            Ok(loaded) => {
                self.write_cache_slot_for_service(service, write_field, Some(loaded.clone()));
                Ok(loaded)
            }
            Err(
                AppFacadeError::SessionNotActive(_)
                | AppFacadeError::SessionExpired(_)
                | AppFacadeError::App(
                    human_money_core::error::AppError::SessionNotActive { .. }
                    | human_money_core::error::AppError::SessionExpired { .. },
                ),
            ) if password.is_none() => {
                if let Some(cached) = self.read_cache_slot_for_service(service, read_field) {
                    return Ok(cached);
                }
                Err(AppFacadeError::App(
                    human_money_core::error::AppError::SessionNotActive {
                        reason: "Password required. Please use 'unlock_session'.".to_string(),
                    },
                ))
            }
            Err(e) => Err(e),
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
                save_res
                    .map_err(|e| app_state_err(format!("Failed to save default {}: {}", key, e)))?;
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

    // 4. History & Outgoing Bundle Helpers
    /// Returns cached transaction history with `"sent"` records hydrated from `outgoing_bundles`
    /// so UI retry/download paths receive exact bytes without storing large payloads inside `tx_history`.
    pub fn get_hydrated_history(&self) -> Result<Vec<TransactionRecord>, AppFacadeError> {
        let current_gen = self.current_session_generation();
        let (mut history, cached_outgoing) = {
            let guard = self
                .profile_caches
                .lock()
                .map_err(|_| app_state_err("History cache lock poisoned".into()))?;
            let caches = guard
                .as_ref()
                .filter(|c| c.generation == current_gen)
                .ok_or_else(|| app_state_err("History cache not initialized".into()))?;
            let history = caches
                .history
                .clone()
                .ok_or_else(|| app_state_err("History cache not initialized".into()))?;
            (history, caches.outgoing_bundles.clone())
        };

        let outgoing = match cached_outgoing {
            Some(o) => o,
            None => {
                let mut service = self.lock_service().map_err(|e| {
                    app_state_err(format!("Service lock unavailable: {}", e.message))
                })?;
                load_outgoing_bundles(&mut service, None).map_err(|e| app_state_err(e.message))?
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
        let baseline_outgoing =
            load_outgoing_bundles(service, password).map_err(|e| app_state_err(e.message))?;
        let prep = prepare_outgoing_history_and_bundle_companions(
            &baseline_history,
            &baseline_outgoing,
            entries,
        )
        .map_err(|e| AppFacadeError::Json(e.message))?;
        let mut batch = EncryptedBatchCommit::new();
        batch.history(prep.history).outgoing_bundles(prep.outgoing);
        batch.commit(self, service, password)?;
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
