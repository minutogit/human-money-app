use std::collections::HashSet;
use std::time::Duration;

use human_money_core::app_service::l2_intents::L2SendIntent;
use human_money_core::app_service::AppService;
use rand::RngCore;

use crate::gateway_resolver::{
    dispatch_lock_committee_batches, summarize_intent_dispatch, IntentDispatchObservation,
};
use crate::l2_lifecycle::{
    is_server_key_revoked, load_server_offender_list, record_equivocation_proof,
};
use crate::models::FrontendError;
use crate::services::common::ensure_restore_l2_reconciled;
use crate::state::AppState;

/// Unified pre-flight checks enforced across value-outflow, claim, swap, issuance, and endorsement paths.
pub struct ValueOutflowPreflight<'a> {
    pub require_restore_reconciled: bool,
    pub check_known_offender_dids: &'a [&'a str],
    pub check_blocking_intent_local_ids: &'a [&'a str],
    pub check_anchor_id_quorum_health: Option<[u8; 32]>,
}

/// Generates a fresh 32-byte non-zero Sync Lock request nonce.
pub(crate) fn random_non_zero_nonce() -> [u8; 32] {
    let mut nonce = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    if nonce == [0u8; 32] {
        nonce[0] = 1;
    }
    nonce
}

/// Runs configured pre-flight safety checks synchronously under the locked `AppService`.
pub fn run_value_outflow_preflight(
    state: &AppState,
    service: &mut AppService,
    password: Option<&str>,
    req: &ValueOutflowPreflight<'_>,
) -> Result<(), FrontendError> {
    if req.require_restore_reconciled {
        ensure_restore_l2_reconciled(state, service, password)?;
    }

    if !req.check_known_offender_dids.is_empty() || !req.check_blocking_intent_local_ids.is_empty()
    {
        service
            .with_wallet(|wallet| -> Result<(), FrontendError> {
                for did in req.check_known_offender_dids {
                    wallet
                        .assert_not_known_offender(did)
                        .map_err(FrontendError::from)?;
                }
                for local_id in req.check_blocking_intent_local_ids {
                    wallet
                        .assert_no_blocking_l2_intent(local_id)
                        .map_err(FrontendError::from)?;
                }
                Ok(())
            })
            .map_err(FrontendError::from)??;
    }

    if let Some(anchor_id) = req.check_anchor_id_quorum_health {
        let anchor_hex = hex::encode(anchor_id);
        let manifest = service
            .with_wallet(|w| w.profile.l2_anchor_manifests.get(&anchor_hex).cloned())
            .map_err(FrontendError::from)?
            .ok_or_else(|| {
                FrontendError::with_code(
                    "error.l2.anchorManifestMissing",
                    format!("Anchor manifest not found for {anchor_hex}"),
                )
            })?;
        manifest
            .verify_anchor_id(&anchor_id)
            .map_err(|e| FrontendError::from(format!("Invalid anchor manifest: {e}")))?;
        let committee = manifest
            .committee()
            .map_err(|e| FrontendError::from(format!("Invalid anchor committee: {e}")))?;

        let offenders = load_server_offender_list(service, password)?;
        let healthy_servers = manifest
            .servers
            .iter()
            .filter(|s| !is_server_key_revoked(&offenders, &s.lock_pub, Some(&s.admin_pub)))
            .count();
        let threshold = usize::from(committee.threshold);
        if healthy_servers < threshold {
            return Err(FrontendError::with_code(
                "error.l2.anchorQuorumUnhealthy",
                format!(
                    "Anchor quorum has only {healthy_servers}/{} non-banned servers (threshold {threshold})",
                    manifest.servers.len()
                ),
            ));
        }
    }

    Ok(())
}

/// Dispatches staged durable `L2SendIntent`s to their committee servers, records receipts
/// (including multi-hop conflict receipts and server equivocation proofs), and returns observations.
pub async fn dispatch_l2_send_intents(
    state: &AppState,
    intents: &[L2SendIntent],
    pow_base_bits: u32,
    password: Option<&str>,
) -> Result<Vec<IntentDispatchObservation>, FrontendError> {
    if intents.is_empty() {
        return Ok(Vec::new());
    }

    let client = crate::sync_client::create_sync_http_client();
    let now_ms = u64::try_from(chrono::Utc::now().timestamp_millis())
        .map_err(|_| FrontendError::from("Invalid verification time".to_string()))?;
    let now_secs = now_ms / 1000;
    let mut l2_dispatch = Vec::with_capacity(intents.len());

    for intent in intents {
        let nonce = random_non_zero_nonce();
        let reqs_res = {
            let mut service = state.lock_service()?;
            let known_heads = crate::state::load_known_head_receipts_for_anchor(
                &mut service,
                &intent.anchor_id,
                password,
            );
            service
                .build_intent_committee_requests(
                    &intent.local_instance_id,
                    &nonce,
                    &known_heads,
                    true,
                )
                .map_err(FrontendError::from)
        };

        let Ok(requests) = reqs_res else {
            l2_dispatch.push(IntentDispatchObservation {
                local_instance_id: intent.local_instance_id.clone(),
                status: "pending".into(),
                http_status: None,
            });
            continue;
        };

        let dispatch =
            dispatch_lock_committee_batches(&client, &requests, Duration::from_secs(4)).await;

        let valid_slots: HashSet<[u8; 32]> = intent.entries.iter().map(|e| e.slot).collect();
        let matching_receipts: Vec<_> = dispatch
            .receipts
            .iter()
            .filter(|r| valid_slots.contains(&r.slot))
            .cloned()
            .collect();

        if !matching_receipts.is_empty()
            || !dispatch.competing_entries.is_empty()
            || !dispatch.receipts.is_empty()
        {
            let mut service = state.lock_service()?;
            let _ = crate::state::record_known_head_receipts(
                &mut service,
                &intent.anchor_id,
                &dispatch.receipts,
                password,
            );
            if !matching_receipts.is_empty() || !dispatch.competing_entries.is_empty() {
                match service.record_intent_server_receipts(
                    &intent.local_instance_id,
                    &nonce,
                    &matching_receipts,
                    dispatch.competing_entries.first(),
                    pow_base_bits,
                    now_ms,
                    password,
                ) {
                    Ok((_, Some(equiv_proof))) => {
                        if let Err(err) = record_equivocation_proof(
                            &mut service,
                            None,
                            &equiv_proof,
                            now_secs,
                            password,
                        ) {
                            log::warn!(
                                "Failed to persist server equivocation proof: {}",
                                err.message
                            );
                        }
                    }
                    Ok((_, None)) => {}
                    Err(err) => {
                        log::warn!("Recording L2 send intent receipts failed: {}", err);
                    }
                }
            }
        }

        l2_dispatch.push(summarize_intent_dispatch(
            &intent.local_instance_id,
            &dispatch,
        ));
    }

    Ok(l2_dispatch)
}
