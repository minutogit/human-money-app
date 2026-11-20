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
    session: &crate::session::SessionToken,
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
        let (known_heads, nonce, reqs_res) = state.with_session_required(
            session,
            "Wallet session changed during L2 dispatch",
            |service| {
                let nonce = random_non_zero_nonce();
                let known_heads = crate::state::load_known_head_receipts_for_anchor(
                    service,
                    &intent.anchor_id,
                    password,
                )?;
                let reqs_res = service
                    .build_intent_committee_requests(
                        &intent.local_instance_id,
                        &nonce,
                        &known_heads,
                        true,
                    )
                    .map_err(FrontendError::from);
                Ok((known_heads, nonce, reqs_res))
            },
        )?;
        let _ = known_heads;

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
            state.with_session_required(
                session,
                "Wallet session changed before L2 intent receipts could be persisted",
                |service| {
                    crate::state::record_known_head_receipts(
                        service,
                        &intent.anchor_id,
                        &dispatch.receipts,
                        password,
                    )?;
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
                                    service,
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
                    Ok(())
                },
            )?;
        }

        l2_dispatch.push(summarize_intent_dispatch(
            &intent.local_instance_id,
            &dispatch,
        ));
    }

    Ok(l2_dispatch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::{
        humoco_proto::{
            crypto::DEFAULT_LOCK_POW_BASE_BITS,
            encode_quarter,
            manifest::{AnchorAuthority, AnchorManifest, GenesisTicket, NominatedBy, ServerEntry},
        },
        models::{profile::PublicProfile, voucher::ValueDefinition},
        services::l2_gateway,
        test_utils::{self, ACTORS},
        NewVoucherData,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn k2_corrupt_known_heads_blocks_intent_dispatch_without_http() {
        let dir = crate::test_support::temp_test_dir("k2_corrupt_sidecar_preflight");
        let pw = "test-pw";
        let (mut service, _) =
            test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        service.unlock_session(pw, 600).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_bg = Arc::clone(&hits);
        let server_task = tokio::spawn(async move {
            while let Ok((_sock, _)) = listener.accept().await {
                hits_bg.fetch_add(1, Ordering::SeqCst);
            }
        });

        let anchor_nonce = [42u8; 32];
        let creator_mig_pub = service
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let sk1 = SigningKey::from_bytes(&[73u8; 32]);
        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: format!("http://{addr}/sync.php"),
                read_url: format!("http://{addr}/d"),
                admin_pub: sk1.verifying_key().to_bytes(),
                lock_pub: sk1.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            }],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let mut t = GenesisTicket {
            anchor_id,
            retain_until_quarter: retain_q,
            issued_at: 1_760_000_000,
            lock_pub: sk1.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        t.sig = sk1.sign(&t.digest()).to_bytes();
        service
            .register_l2_anchor_template_with_nonce(
                manifest,
                vec![t],
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();

        let user_id = service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        service
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(user_id),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "50".into(),
                        ..Default::default()
                    },
                    layer2_anchor: Some(anchor_id),
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();
        let local_id = service
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();
        service
            .prepare_l2_send_intent(&local_id, DEFAULT_LOCK_POW_BASE_BITS, Some(pw))
            .unwrap();
        let intent = service.pending_l2_send_intent(&local_id).unwrap();

        // Corrupt the anti-rollback sidecar before dispatch.
        service
            .save_local_sidecar_data(
                crate::state::KNOWN_LOCK_HEADS_KEY,
                b"{corrupt-lock-heads",
                Some(pw),
            )
            .unwrap();

        let state = crate::test_support::create_test_state(service);
        let session = {
            let s = state.lock_service().unwrap();
            state.session_token_for_service(&s).unwrap()
        };
        let res = dispatch_l2_send_intents(
            &state,
            &session,
            &[intent],
            DEFAULT_LOCK_POW_BASE_BITS,
            Some(pw),
        )
        .await;

        server_task.abort();
        assert!(
            res.is_err(),
            "Corrupt anti-rollback sidecar must fail closed before sending L2 requests"
        );
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "Zero HTTP requests may be dispatched when known heads fail to load"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
