use super::*;
use std::collections::BTreeMap;

use human_money_core::humoco_proto::{
    decode_hex_fixed,
    types::{EquivocationProof, LockPubKey, LockReceiptKind},
};
use humoco_wot::{
    resolve_successor_chain, KeySuccessorAttestation, ServerOffenderList, ServerOffenderRecord,
    WotEquivocationProof, WotLockDelegation, WotLockReceipt, WotLockReceiptKind,
};

use crate::models::FrontendError;
use crate::state::AppState;
use crate::sync_client::{self, FrontendLockServerProbeResult};

pub fn load_server_offender_list(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Result<ServerOffenderList, FrontendError> {
    crate::state::load_encrypted_json_opt::<ServerOffenderList>(
        service,
        SERVER_OFFENDER_LIST_KEY,
        password,
    )
    .map(|opt| opt.unwrap_or_else(ServerOffenderList::new))
    .map_err(FrontendError::from)
}

pub fn save_server_offender_list(
    service: &mut human_money_core::app_service::AppService,
    list: &ServerOffenderList,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, SERVER_OFFENDER_LIST_KEY, list, password)
        .map_err(FrontendError::from)
}

fn to_wot_receipt(r: &human_money_core::humoco_proto::types::LockReceipt) -> WotLockReceipt {
    WotLockReceipt {
        kind: match r.kind {
            LockReceiptKind::FirstWrite => WotLockReceiptKind::FirstWrite,
            LockReceiptKind::ConflictSeen => WotLockReceiptKind::ConflictSeen,
        },
        lock_pub: r.lock_pub,
        bucket: r.bucket,
        slot: r.slot,
        child_lock: r.child_lock,
        entry_hash: r.entry_hash,
        retain_until_quarter: r.retain_until_quarter,
        write_seq: r.write_seq,
        bucket_head: r.bucket_head,
        recipient_nonce: r.recipient_nonce,
        sig: r.sig,
    }
}

pub fn is_server_key_revoked(
    offenders: &ServerOffenderList,
    lock_pub: &LockPubKey,
    admin_pub: Option<&[u8; 32]>,
) -> bool {
    offenders.is_lock_pub_revoked(lock_pub)
        || admin_pub.is_some_and(|a| offenders.is_admin_pub_revoked(a))
}

fn record_equivocation_proof_inner(
    service: &mut human_money_core::app_service::AppService,
    delegation_opt: Option<WotLockDelegation>,
    proof: &EquivocationProof,
    now_secs: u64,
    password: Option<&str>,
) -> Result<(bool, ServerOffenderList), FrontendError> {
    let now = now_secs.max(1);
    let offending_lock = proof
        .verify()
        .map_err(|e| lifecycle_err(format!("Invalid equivocation proof: {}", e)))?;
    let wot_proof = WotEquivocationProof {
        receipt_a: to_wot_receipt(&proof.receipt_a),
        receipt_b: to_wot_receipt(&proof.receipt_b),
    };
    wot_proof
        .receipt_a
        .verify_signature()
        .map_err(|e| lifecycle_err(format!("Invalid equivocation receipt A: {}", e)))?;
    wot_proof
        .receipt_b
        .verify_signature()
        .map_err(|e| lifecycle_err(format!("Invalid equivocation receipt B: {}", e)))?;
    if let Some(ref del) = delegation_opt {
        if del.lock_pub != offending_lock {
            return Err(lifecycle_err(format!(
                "Invalid lock delegation: lock_pub {} does not match equivocation lock_pub {}",
                hex::encode(del.lock_pub),
                hex::encode(offending_lock)
            )));
        }
        del.verify()
            .map_err(|e| lifecycle_err(format!("Invalid lock delegation: {}", e)))?;
    }

    let mut list = load_server_offender_list(service, password)?;
    let lock_already = list.is_lock_pub_revoked(&offending_lock);
    let admin_already = delegation_opt
        .as_ref()
        .is_none_or(|del| list.is_admin_pub_revoked(&del.admin_pub));
    if lock_already && admin_already {
        return Ok((false, list));
    }

    let record = ServerOffenderRecord {
        proof: wot_proof,
        delegation: delegation_opt,
        recorded_at: now,
    };
    if record.proof.receipt_a.child_lock != record.proof.receipt_b.child_lock {
        list.insert_verified(record, now)
            .map_err(|e| lifecycle_err(format!("Invalid equivocation proof: {}", e)))?;
    } else {
        // Note: upstream humoco-wot `WotEquivocationProof::verify` currently only permits distinct
        // `child_lock` (follow-up alignment to also permit distinct `entry_hash`). Both
        // `humoco_proto::EquivocationProof::verify()` and `WotLockReceipt::verify_signature()`
        // plus `WotLockDelegation::verify()` have already been verified above.
        list.revoked_lock_pubs.insert(offending_lock);
        if let Some(ref del) = record.delegation {
            list.revoked_admin_pubs.insert(del.admin_pub);
        }
        list.records.push(record);
    }
    save_server_offender_list(service, &list, password)?;
    Ok((true, list))
}

pub fn record_equivocation_proof(
    service: &mut human_money_core::app_service::AppService,
    delegation_opt: Option<WotLockDelegation>,
    proof: &EquivocationProof,
    now_secs: u64,
    password: Option<&str>,
) -> Result<bool, FrontendError> {
    record_equivocation_proof_inner(service, delegation_opt, proof, now_secs, password)
        .map(|(newly, _)| newly)
}

pub(crate) fn sync_delegation_to_wot(
    del: &sync_client::SyncLockDelegationDto,
) -> Option<WotLockDelegation> {
    let admin_pub = decode_hex_fixed::<32>(del.admin_pub.trim()).ok()?;
    let lock_pub = decode_hex_fixed::<32>(del.lock_pub.trim()).ok()?;
    let sig = decode_hex_fixed::<64>(del.sig.trim()).ok()?;
    Some(WotLockDelegation {
        admin_pub,
        lock_pub,
        write_url: del.write_url.trim().to_string(),
        read_url: del.read_url.trim().to_string(),
        issued_at: del.issued_at,
        sig,
    })
}

/// Persists a verified `EquivocationProof`, attaching the verified `WotLockDelegation` from
/// `probe_cache` when available (enabling ADR-10 `revoked_admin_pubs` operator bans), and updates
/// `state`'s `server_offenders` cache slot atomically while `service` is held.
pub fn record_equivocation_and_refresh_cache(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    probe_cache: &BTreeMap<String, FrontendLockServerProbeResult>,
    proof: &EquivocationProof,
    now_secs: u64,
    password: Option<&str>,
) -> Result<bool, FrontendError> {
    let lock_hex = hex::encode(proof.receipt_a.lock_pub);
    let wot_delegation = probe_cache
        .get(&lock_hex)
        .filter(|probe| probe.delegation_valid)
        .and_then(|probe| probe.delegation.as_ref())
        .filter(|del| sync_client::verify_lock_delegation(&lock_hex, del, None))
        .and_then(sync_delegation_to_wot);
    let (newly_recorded, updated_list) =
        record_equivocation_proof_inner(service, wot_delegation, proof, now_secs, password)?;
    state.write_cache_slot_for_service(service, |c| &mut c.server_offenders, Some(updated_list));
    Ok(newly_recorded)
}

pub fn load_key_successor_attestations(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Result<Vec<KeySuccessorAttestation>, FrontendError> {
    crate::state::load_encrypted_json_or_default(service, KEY_SUCCESSOR_STORE_KEY, password)
        .map_err(FrontendError::from)
}

pub fn save_key_successor_attestations(
    service: &mut human_money_core::app_service::AppService,
    list: &[KeySuccessorAttestation],
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, KEY_SUCCESSOR_STORE_KEY, &list, password)
        .map_err(FrontendError::from)
}

pub fn resolve_current_did(
    service: &mut human_money_core::app_service::AppService,
    did: &str,
    now_secs: u64,
    password: Option<&str>,
) -> Result<String, FrontendError> {
    let attestations = load_key_successor_attestations(service, password)?;
    let canonical = crate::state::canonicalize_did(did)
        .map_err(|e| FrontendError::from(format!("Invalid DID '{}': {}", did.trim(), e)))?;
    Ok(resolve_successor_chain(&canonical, &attestations, now_secs).unwrap_or(canonical))
}
