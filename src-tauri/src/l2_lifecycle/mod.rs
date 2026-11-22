//! Sync-Lock committee lifecycle orchestration (`APP-071`, Concept §5.4–§5.7):
//! - Permanent `ServerOffenderList` persistence and equivocation recording (`WOT-013`).
//! - `KeySuccessorAttestation` storage and 4-stage long-term manifest discovery cascade (§5.6).
//! - Periodic server health checks with 5-day hysteresis (immediate on `SyncRevocationNotice` / `410`).
//! - Crash-resumable 3-phase committee rotation (`propose` -> `fetch_snapshot`/`import` -> `commit`/`finalize`).
//! - Automatic guarantor vetoing of unauthorized or protection-reducing proposals during timelock.
//! - Regular L1 `Tx_claim` at guarantors (§5.7).

use std::collections::{BTreeMap, BTreeSet};

use human_money_core::humoco_proto::{ImportAck, MigrationProposal};
use serde::{Deserialize, Serialize};

use crate::models::FrontendError;

mod discovery;
mod offenders;
mod rotation_policy;
mod rotation_runner;

#[cfg(test)]
mod tests;

pub(crate) use discovery::try_follow_relocation_notice;
pub use discovery::{
    discover_anchor_manifest, mirror_manifest_to_own_syncs, record_revoked_servers,
};
pub use offenders::{
    is_server_key_revoked, load_key_successor_attestations, load_server_offender_list,
    record_equivocation_and_refresh_cache, record_equivocation_proof, resolve_current_did,
    save_key_successor_attestations, save_server_offender_list,
};
pub(crate) use rotation_runner::maybe_run_background_lifecycle_tick;

pub const SERVER_OFFENDER_LIST_KEY: &str = "l2_server_offenders";
pub const KEY_SUCCESSOR_STORE_KEY: &str = "wot_key_successors";
pub const L2_LIFECYCLE_STATE_KEY: &str = "l2_lifecycle_state";
pub const HEALTH_CHECK_INTERVAL_SECS: u64 = 6 * 3600;
pub const UNHEALTHY_HYSTERESIS_SECS: u64 = 5 * 86_400;

pub(crate) fn lifecycle_err(msg: impl Into<String>) -> FrontendError {
    FrontendError::from(msg.into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredGuarantorRole {
    pub anchor_id: String,
    pub guarantor_index: u8,
    pub context_nonce_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum StoredRotationPhase {
    Proposed {
        proposal: MigrationProposal,
        effective_at_secs: u64,
    },
    Imported {
        proposal: MigrationProposal,
        imported_state_hash_hex: String,
        import_acks: Vec<ImportAck>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredAnchorHealth {
    pub anchor_id: String,
    pub last_checked_secs: u64,
    pub unhealthy_since_by_lock_pub: BTreeMap<String, u64>,
    pub revoked_lock_pubs: BTreeSet<String>,
    pub active_rotation: Option<StoredRotationPhase>,
    pub last_rotated_rev: u64,
    pub last_error: Option<String>,
}

impl StoredAnchorHealth {
    fn new(anchor_id: String) -> Self {
        Self {
            anchor_id,
            last_checked_secs: 0,
            unhealthy_since_by_lock_pub: BTreeMap::new(),
            revoked_lock_pubs: BTreeSet::new(),
            active_rotation: None,
            last_rotated_rev: 1,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredL2LifecycleState {
    pub last_tick_secs: u64,
    pub anchors: BTreeMap<String, StoredAnchorHealth>,
    pub guarantor_roles: BTreeMap<String, StoredGuarantorRole>,
    pub auto_vetoes_sent: u64,
    pub last_veto_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAnchorLifecycleDto {
    pub anchor_id: String,
    pub revision: u64,
    pub reduced_guarantor_protection: bool,
    pub unhealthy_servers: Vec<String>,
    pub revoked_servers: Vec<String>,
    pub active_rotation_phase: Option<String>,
    pub effective_at_secs: Option<u64>,
    pub last_checked_secs: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendL2LifecycleStatusDto {
    pub last_tick_secs: u64,
    pub tracked_anchors_count: usize,
    pub guarantor_roles_count: usize,
    pub server_offenders_count: usize,
    pub auto_vetoes_sent: u64,
    pub last_veto_reason: Option<String>,
    pub anchors: Vec<FrontendAnchorLifecycleDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAnchorDiscoveryDto {
    pub anchor_id: String,
    pub revision: u64,
    pub threshold: u8,
    pub server_count: usize,
    pub reduced_guarantor_protection: bool,
    pub manifest_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendClaimBundleDto {
    pub bundle_data: Vec<u8>,
    pub bundle_id: String,
    pub guarantor_id: String,
    pub claimed_amount: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_deposit: Option<crate::models::FrontendPostDepositResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_deposit_error: Option<String>,
}

pub fn load_lifecycle_state(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Result<StoredL2LifecycleState, FrontendError> {
    crate::state::load_encrypted_json_or_default(service, L2_LIFECYCLE_STATE_KEY, password)
        .map_err(FrontendError::from)
}

pub fn save_lifecycle_state(
    service: &mut human_money_core::app_service::AppService,
    lc_state: &StoredL2LifecycleState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, L2_LIFECYCLE_STATE_KEY, lc_state, password)
        .map_err(FrontendError::from)
}
