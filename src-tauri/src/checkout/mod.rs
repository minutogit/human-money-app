//! Recipient Sync-Lock checkout orchestration (`CORE-020` / `APP-070`).
//!
//! Computes `base_trusted_keys` and `independently_pinned_creator_keys` strictly from locally
//! trusted sources (own Syncs, Hop-1/Hop-2 non-offender contacts, ADR-08 provider filter seals)
//! and resolves authentic `LockPubKey`s without trusting untrusted manifest claims.

use std::collections::HashSet;
use std::time::Duration;

use human_money_core::humoco_proto::types::LockPubKey;
use rand::RngCore;

use crate::models::FrontendError;

mod execute;
mod precommit;
mod trusted_keys;

#[cfg(test)]
mod tests;

pub(crate) use execute::{checkout_bundle, resolve_manifest_upgrade_after_dispatch};
pub(crate) use precommit::{verify_bundle_l2_precommit, PrecommitL2Verification};

const CHECKOUT_BATCH_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VerifiedServerBinding {
    lock_pub: LockPubKey,
    admin_pub: [u8; 32],
}

pub(crate) fn random_non_zero_nonce() -> [u8; 32] {
    loop {
        let mut nonce = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        if nonce != [0u8; 32] {
            return nonce;
        }
    }
}

type ReceiptDedupKey = (LockPubKey, [u8; 32], [u8; 32], [u8; 32]);

pub(crate) fn merge_receipts_dedup(
    base: &mut Vec<human_money_core::humoco_proto::types::LockReceipt>,
    extra: impl IntoIterator<Item = human_money_core::humoco_proto::types::LockReceipt>,
) {
    let mut seen: HashSet<ReceiptDedupKey> = base
        .iter()
        .map(|r| (r.lock_pub, r.slot, r.child_lock, r.entry_hash))
        .collect();
    for r in extra {
        if seen.insert((r.lock_pub, r.slot, r.child_lock, r.entry_hash)) {
            base.push(r);
        }
    }
}

fn fail(message: &str) -> FrontendError {
    FrontendError::from(message.to_string())
}

fn now() -> Result<u64, FrontendError> {
    u64::try_from(chrono::Utc::now().timestamp_millis())
        .map_err(|_| fail("Invalid verification time"))
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseCompletion {
    pub purchase_id: String,
    pub newly_committed: bool,
    pub is_replay: bool,
    pub verdict: crate::models::FrontendCheckoutReleaseVerdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receive_payload: Option<crate::models::ReceiveSuccessPayload>,
}

fn format_bff_name(raw_name: &str, is_test: bool) -> String {
    if is_test {
        if raw_name.starts_with("TEST-") {
            raw_name.to_string()
        } else {
            format!("TEST-{}", raw_name)
        }
    } else {
        raw_name.to_string()
    }
}
