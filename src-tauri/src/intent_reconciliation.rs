//! Background worker driving mandatory `1..=N` committee completion for pending outbound Sync Lock sends.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use human_money_core::humoco_proto::{
    causal::LockEntry,
    crypto::DEFAULT_LOCK_POW_BASE_BITS,
    types::{LockPubKey, LockReceiptKind},
};
use human_money_core::models::l2_intent::L2SendIntent;
use tauri::Manager;

use crate::{models::FrontendError, settings::InternalLifecycleState, sync_client, AppState};

const RECONCILIATION_DELAY_MS: u64 = 30_000;
const TERMINAL_RETENTION_MS: u64 = 60_000;
const RECONCILIATION_INTERVAL_SECS: u64 = 15;
const INTENT_DISPATCH_TIMEOUT: Duration = Duration::from_secs(4);

fn now_ms() -> Result<u64, FrontendError> {
    u64::try_from(Utc::now().timestamp_millis())
        .map_err(|_| FrontendError::from("Invalid verification time".to_string()))
}

async fn reconcile_due_intent(
    state: &AppState,
    session: &crate::session::SessionToken,
    lifecycle: &mut InternalLifecycleState,
    intent: &L2SendIntent,
    now: u64,
) -> Result<(), FrontendError> {
    let local_id = &intent.local_instance_id;
    let mut active_nonce = crate::checkout::random_non_zero_nonce();

    let Some((requests, creator_id, guarantor_dids)) = state.with_session(session, |service| {
        let known_heads =
            crate::state::load_known_head_receipts_for_anchor(service, &intent.anchor_id, None)?;
        let reqs = service
            .build_intent_committee_requests(local_id, &active_nonce, &known_heads, true)
            .map_err(FrontendError::from)?;
        let (creator_id, guarantor_dids) = service
            .with_wallet(|w| {
                w.get_voucher_instance(local_id)
                    .map(|inst| {
                        let creator = inst.voucher.creator_profile.id.clone();
                        let guarantors = inst
                            .voucher
                            .signatures
                            .iter()
                            .filter(|s| s.role.eq_ignore_ascii_case("guarantor"))
                            .map(|s| s.signer_id.clone())
                            .collect::<Vec<_>>();
                        (creator, guarantors)
                    })
                    .unwrap_or((None, Vec::new()))
            })
            .map_err(FrontendError::from)?;
        Ok((reqs, creator_id, guarantor_dids))
    })?
    else {
        return Ok(());
    };

    if requests.is_empty() {
        return Ok(());
    }

    let client = sync_client::create_sync_http_client();
    let mut dispatch = crate::gateway_resolver::dispatch_lock_committee_batches(
        &client,
        &requests,
        INTENT_DISPATCH_TIMEOUT,
        &lifecycle.l2_probe_cache,
    )
    .await;
    let mut all_competing_entries = dispatch.competing_entries.clone();
    let mut all_receipts_for_equiv = dispatch.receipts.clone();
    let mut all_equiv_proofs = dispatch.equivocation_proofs.clone();

    // Handle `409 epoch_frozen` (with or without inline manifest), `410 revoked` relocation,
    // and total committee unreachability via shared discovery/relocation path.
    let upgrade = crate::checkout::resolve_manifest_upgrade_after_dispatch(
        state,
        session,
        &client,
        &intent.anchor_id,
        creator_id.as_deref(),
        &guarantor_dids,
        lifecycle,
        &intent.manifest,
        &dispatch,
        None,
    )
    .await?;

    if upgrade.upgraded_manifest.is_some() || upgrade.relocated {
        if let Some((newer_manifest, _)) = upgrade.upgraded_manifest {
            let next_nonce = crate::checkout::random_non_zero_nonce();
            let upgraded_step = state.with_session(session, |service| {
                let updated_reqs = if service
                    .update_l2_anchor_manifest(newer_manifest.clone(), None)
                    .is_ok()
                {
                    let known_heads = crate::state::load_known_head_receipts_for_anchor(
                        service,
                        &intent.anchor_id,
                        None,
                    )?;
                    service
                        .build_intent_committee_requests(local_id, &next_nonce, &known_heads, true)
                        .ok()
                } else {
                    None
                };
                let fresh_lc = state
                    .get_or_load_internal_lifecycle(service, None)
                    .map_err(FrontendError::from)?;
                Ok((fresh_lc, updated_reqs))
            })?;
            if let Some((fresh_lc, next_reqs_opt)) = upgraded_step {
                *lifecycle = fresh_lc;
                if let Some(next_reqs) = next_reqs_opt {
                    active_nonce = next_nonce;
                    if !next_reqs.is_empty() {
                        let dispatch2 = crate::gateway_resolver::dispatch_lock_committee_batches(
                            &client,
                            &next_reqs,
                            INTENT_DISPATCH_TIMEOUT,
                            &lifecycle.l2_probe_cache,
                        )
                        .await;
                        all_competing_entries.extend(dispatch2.competing_entries.iter().cloned());
                        all_receipts_for_equiv.extend(dispatch2.receipts.iter().cloned());
                        all_equiv_proofs.extend(dispatch2.equivocation_proofs.iter().cloned());
                        dispatch = dispatch2;
                    } else {
                        dispatch = crate::gateway_resolver::CommitteeDispatchResult::default();
                    }
                }
            }
        } else {
            if let Some(fresh_lc) = state.with_session(session, |service| {
                state
                    .get_or_load_internal_lifecycle(service, None)
                    .map_err(FrontendError::from)
            })? {
                *lifecycle = fresh_lc;
            }
            let pending_retry_pubs: HashSet<LockPubKey> = dispatch
                .pending_retries
                .iter()
                .map(|(s, _)| s.lock_pub)
                .collect();
            let retry_reqs: Vec<_> = requests
                .iter()
                .filter(|(s, _)| {
                    upgrade.relocated_lock_pubs.contains(&s.lock_pub)
                        || pending_retry_pubs.contains(&s.lock_pub)
                })
                .cloned()
                .collect();
            if !retry_reqs.is_empty() {
                let dispatch2 = crate::gateway_resolver::dispatch_lock_committee_batches(
                    &client,
                    &retry_reqs,
                    INTENT_DISPATCH_TIMEOUT,
                    &lifecycle.l2_probe_cache,
                )
                .await;
                all_competing_entries.extend(dispatch2.competing_entries.iter().cloned());
                all_receipts_for_equiv.extend(dispatch2.receipts.iter().cloned());
                all_equiv_proofs.extend(dispatch2.equivocation_proofs.iter().cloned());
                crate::checkout::merge_receipts_dedup(&mut dispatch.receipts, dispatch2.receipts);
            }
        }
    }

    if dispatch.receipts.is_empty()
        && all_competing_entries.is_empty()
        && all_receipts_for_equiv.is_empty()
        && all_equiv_proofs.is_empty()
    {
        return Ok(());
    }

    let Some(mut service) = state
        .lock_service()
        .ok()
        .filter(|s| state.is_session_current(session, s))
    else {
        return Ok(());
    };
    crate::state::record_known_head_receipts(
        &mut service,
        &intent.anchor_id,
        &all_receipts_for_equiv,
        None,
    )?;

    if let Some(proof) =
        human_money_core::services::l2_gateway::detect_equivocation(&all_receipts_for_equiv)
    {
        all_equiv_proofs.push(proof);
    }
    if !all_equiv_proofs.is_empty() {
        let now_secs = sync_client::current_unix_timestamp().max(1);
        for proof in &all_equiv_proofs {
            log::error!(
                "EquivocationProof detected during L2 send intent {} on server {}",
                local_id,
                hex::encode(proof.receipt_a.lock_pub)
            );
            let _ = crate::l2_lifecycle::record_equivocation_and_refresh_cache(
                state,
                &mut service,
                &lifecycle.l2_probe_cache,
                proof,
                now_secs,
                None,
            );
        }
    }

    let offenders = state.get_or_load_server_offender_list(&mut service, None)?;
    let Ok(current_intent) = service.pending_l2_send_intent(local_id) else {
        return Ok(());
    };
    let voucher_opt = service
        .with_wallet(|w| {
            let mut v_opt = current_intent
                .created_change_local_id
                .as_deref()
                .and_then(|cid| w.get_voucher_instance(cid))
                .or_else(|| w.get_voucher_instance(local_id))
                .map(|i| i.voucher.clone());
            if let (Some(ref mut v), Ok(intent_tx)) = (
                &mut v_opt,
                serde_json::from_slice::<human_money_core::models::voucher::Transaction>(
                    &current_intent.transaction_bytes,
                ),
            ) {
                if !v.transactions.iter().any(|t| t.t_id == intent_tx.t_id) {
                    v.transactions.push(intent_tx);
                }
            }
            v_opt
        })
        .map_err(FrontendError::from)?;

    // Verify all candidate competing entries using historical DEFAULT_LOCK_POW_BASE_BITS so one
    // forged competing entry at index 0 cannot mask a valid conflict or roll back the tx.
    type CompetingEntryKey = ([u8; 32], [u8; 32], [u8; 32]);
    let mut verified_competing_by_key: HashMap<CompetingEntryKey, LockEntry> = HashMap::new();
    if let Some(ref voucher) = voucher_opt {
        for ce in &all_competing_entries {
            let Ok(ce_hash) = ce.entry_hash() else {
                continue;
            };
            let valid = voucher.transactions.iter().rev().any(|hop_tx| {
                let Ok(ds_tag) =
                    human_money_core::services::l2_gateway::derive_challenge_tag_bytes(hop_tx)
                else {
                    return false;
                };
                if human_money_core::humoco_proto::derive_slot(&intent.anchor_id, &ds_tag)
                    != ce.slot
                {
                    return false;
                }
                let Ok(local_t_id) =
                    human_money_core::crypto::decode_bs58_fixed::<32>(&hop_tx.t_id, "hop.t_id")
                else {
                    return false;
                };
                if ce.child_lock == local_t_id {
                    return false;
                }
                human_money_core::services::l2_gateway::verify_competing_entry_and_build_fingerprint(
                    &intent.anchor_id,
                    &ds_tag,
                    ce,
                    voucher,
                    Some(&current_intent.manifest.authority.creator_mig_pub),
                    DEFAULT_LOCK_POW_BASE_BITS,
                )
                .is_ok()
            });
            if valid {
                verified_competing_by_key
                    .entry((ce.slot, ce.child_lock, ce_hash))
                    .or_insert_with(|| ce.clone());
            }
        }
    }

    let Ok(committee) = current_intent.manifest.committee() else {
        return Ok(());
    };
    let admin_by_lock: HashMap<LockPubKey, [u8; 32]> = current_intent
        .manifest
        .servers
        .iter()
        .map(|s| (s.lock_pub, s.admin_pub))
        .collect();

    // Filter across ALL slots in `current_intent.entries` (never only `.last()`), rejecting
    // malformed, revoked, or unverified-conflict receipts so 1 rogue server cannot abort the tx.
    let mut filtered_receipts = Vec::new();
    let mut chosen_competing: Option<LockEntry> = None;
    for r in &dispatch.receipts {
        let Some(matched_entry) = current_intent.entries.iter().find(|e| e.slot == r.slot) else {
            continue;
        };
        let Ok(matched_entry_hash) = matched_entry.entry_hash() else {
            continue;
        };
        if !committee.contains(&r.lock_pub)
            || crate::l2_lifecycle::is_server_key_revoked(
                &offenders,
                &r.lock_pub,
                admin_by_lock.get(&r.lock_pub),
            )
            || r.bucket
                != human_money_core::humoco_proto::derive_bucket(&intent.anchor_id, &r.lock_pub)
            || r.recipient_nonce != active_nonce
            || r.write_seq == 0
            || r.retain_until_quarter != current_intent.manifest.retain_until_quarter
            || !r.verify_signature()
        {
            continue;
        }
        let is_conflict = r.kind == LockReceiptKind::ConflictSeen
            || r.child_lock != matched_entry.child_lock
            || (r.kind == LockReceiptKind::FirstWrite && r.entry_hash != matched_entry_hash);
        if is_conflict {
            let Some(verified_ce) =
                verified_competing_by_key.get(&(r.slot, r.child_lock, r.entry_hash))
            else {
                continue;
            };
            if chosen_competing.is_none() {
                chosen_competing = Some(verified_ce.clone());
            }
        }
        filtered_receipts.push(r.clone());
    }

    if filtered_receipts.is_empty() {
        return Ok(());
    }

    let (resolution_opt, equivocation_opt) = service
        .record_intent_server_receipts(
            local_id,
            &active_nonce,
            &filtered_receipts,
            chosen_competing.as_ref(),
            DEFAULT_LOCK_POW_BASE_BITS,
            now,
            None,
        )
        .map_err(FrontendError::from)?;

    if let Some(proof) = equivocation_opt {
        log::error!(
            "EquivocationProof detected for L2 send intent {} on server {}",
            local_id,
            hex::encode(proof.receipt_a.lock_pub)
        );
        let now_secs = sync_client::current_unix_timestamp().max(1);
        let _ = crate::l2_lifecycle::record_equivocation_and_refresh_cache(
            state,
            &mut service,
            &lifecycle.l2_probe_cache,
            &proof,
            now_secs,
            None,
        );
    }
    if let Some(res) = resolution_opt {
        log::info!(
            "L2 send intent {} reached terminal state: {:?}",
            local_id,
            res
        );
    }
    let _ = service.cleanup_terminal_intents(TERMINAL_RETENTION_MS, now, None);

    Ok(())
}

pub async fn reconcile_due_intents(
    state: &AppState,
    in_flight: &Arc<Mutex<HashSet<String>>>,
) -> Result<usize, FrontendError> {
    let now = now_ms()?;

    let (session, intents) = {
        let mut service = state.lock_service()?;
        let session = match state.session_token_for_service(&service) {
            Ok(session) => session,
            Err(_) => return Ok(0),
        };
        if let Err(error) = service.cleanup_terminal_intents(TERMINAL_RETENTION_MS, now, None) {
            log::debug!("Failed to clean up terminal L2 send intents: {}", error);
        }
        let intents = service
            .unresolved_l2_send_intents()
            .map_err(FrontendError::from)?;
        (session, intents)
    };

    if intents.is_empty() {
        return Ok(0);
    }

    let mut lifecycle_opt: Option<InternalLifecycleState> = None;
    let mut handled = 0usize;

    for intent in intents {
        if !state.is_generation_current(session.generation) {
            break;
        }
        let local_id = intent.local_instance_id.clone();

        if now >= intent.root_valid_until_ms {
            let expired = state.with_session(&session, |service| {
                if let Err(error) = service.expire_intent_if_overdue(&local_id, now, None) {
                    log::debug!("Failed to expire L2 send intent {}: {}", local_id, error);
                }
                if let Err(error) =
                    service.cleanup_terminal_intents(TERMINAL_RETENTION_MS, now, None)
                {
                    log::debug!("Failed to clean up terminal L2 send intents: {}", error);
                }
                Ok(())
            })?;
            if expired.is_none() {
                break;
            }
            handled += 1;
            continue;
        }

        if now < intent.created_at_ms.saturating_add(RECONCILIATION_DELAY_MS) {
            continue;
        }

        if lifecycle_opt.is_none() {
            let loaded = state.with_session(&session, |service| {
                state
                    .get_or_load_internal_lifecycle(service, None)
                    .map_err(FrontendError::from)
            })?;
            let Some(lc) = loaded else {
                break;
            };
            lifecycle_opt = Some(lc);
        }
        let lifecycle = lifecycle_opt
            .as_mut()
            .expect("lifecycle initialized before dispatch");

        {
            let mut in_flight_guard = in_flight.lock().map_err(|_| {
                FrontendError::from("Intent reconciliation state unavailable".to_string())
            })?;
            if !in_flight_guard.insert(local_id.clone()) {
                continue;
            }
        }

        let result = reconcile_due_intent(state, &session, lifecycle, &intent, now).await;

        if let Ok(mut in_flight_guard) = in_flight.lock() {
            in_flight_guard.remove(&local_id);
        }

        match result {
            Ok(()) => handled += 1,
            Err(error) => log::warn!(
                "L2 send intent reconciliation failed for {}: {}",
                local_id,
                error.message
            ),
        }
    }

    Ok(handled)
}

pub fn spawn_intent_reconciliation_worker(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(RECONCILIATION_INTERVAL_SECS));
        interval.tick().await;
        loop {
            interval.tick().await;
            let state = app.state::<AppState>();
            match reconcile_due_intents(state.inner(), &in_flight).await {
                Ok(0) => {}
                Ok(count) => log::info!("Reconciled {} unresolved L2 send intent(s)", count),
                Err(error) => log::debug!("L2 intent reconciliation failed: {}", error.message),
            }
            crate::l2_lifecycle::maybe_run_background_lifecycle_tick(state.inner()).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::{
        app_service::AppService,
        humoco_proto::{
            crypto::DEFAULT_LOCK_POW_BASE_BITS,
            encode_quarter,
            manifest::{AnchorAuthority, AnchorManifest, GenesisTicket, NominatedBy, ServerEntry},
        },
        models::{
            l2_intent::IntentTerminalResolution, profile::PublicProfile, voucher::ValueDefinition,
        },
        services::l2_gateway,
        test_utils::{self, ACTORS},
        NewVoucherData,
    };
    use std::path::{Path, PathBuf};

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "human-money-app-intent-reconciliation-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    fn state_with(service: AppService, lifecycle: Option<InternalLifecycleState>) -> AppState {
        let state = AppState::new(service);
        state.set_cached_internal_lifecycle(lifecycle);
        state
    }

    fn unlocked_fixture(path: &Path, password: &str) -> (AppState, String) {
        let (mut service, _) = test_utils::setup_service_with_profile(
            path,
            &ACTORS.test_user,
            "Intent Reconciliation",
            password,
        );
        service.unlock_session(password, 600).unwrap();

        let anchor_nonce = [55u8; 32];
        let creator_mig_pub = service
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let sk1 = SigningKey::from_bytes(&[81u8; 32]);
        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: "https://s1.example/sync.php".into(),
                read_url: "https://s1.example/d".into(),
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
                Some(password),
            )
            .unwrap();

        let user = service
            .with_wallet(|wallet| wallet.get_user_id().to_string())
            .unwrap();
        let standard = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        service
            .create_new_voucher(
                &standard,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(user),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "100".into(),
                        ..Default::default()
                    },
                    layer2_anchor: Some(anchor_id),
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();

        let local_id = service
            .with_wallet_and_identity(|wallet, identity| {
                wallet.list_vouchers(Some(identity), None, None, None)
            })
            .unwrap()[0]
            .local_instance_id
            .clone();
        service
            .prepare_l2_send_intent(&local_id, DEFAULT_LOCK_POW_BASE_BITS, Some(password))
            .unwrap();

        (
            state_with(service, Some(InternalLifecycleState::default())),
            local_id,
        )
    }

    #[tokio::test]
    async fn skips_locked_wallets() {
        let path = temp_path();
        let state = state_with(AppService::new(&path).unwrap(), None);
        let in_flight = Arc::new(Mutex::new(HashSet::new()));

        let handled = reconcile_due_intents(&state, &in_flight)
            .await
            .expect("locked wallet reconciliation must be a no-op");

        assert_eq!(handled, 0);
        assert!(in_flight.lock().unwrap().is_empty());
        drop(state);
        cleanup(&path);
    }

    #[tokio::test]
    async fn skips_intents_younger_than_thirty_seconds() {
        let path = temp_path();
        let password = "young-intent-password";
        let (state, local_id) = unlocked_fixture(&path, password);
        let in_flight = Arc::new(Mutex::new(HashSet::new()));

        let handled = reconcile_due_intents(&state, &in_flight).await.unwrap();

        assert_eq!(handled, 0);
        let service = state.service.lock().unwrap();
        let intent = service.pending_l2_send_intent(&local_id).unwrap();
        assert!(intent.terminal_resolution.is_none());
        assert!(now_ms().unwrap() < intent.created_at_ms.saturating_add(RECONCILIATION_DELAY_MS));
        drop(service);
        drop(state);
        cleanup(&path);
    }

    #[tokio::test]
    async fn handles_expired_intents() {
        let path = temp_path();
        let password = "expired-intent-password";
        let (state, local_id) = unlocked_fixture(&path, password);

        {
            let mut service = state.service.lock().unwrap();
            let wallet = service.get_wallet_mut().unwrap();
            wallet
                .profile
                .l2_send_intents
                .get_mut(&local_id)
                .unwrap()
                .root_valid_until_ms = now_ms().unwrap().saturating_sub(1_000);
        }

        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let handled = reconcile_due_intents(&state, &in_flight).await.unwrap();

        assert_eq!(handled, 1);
        let service = state.service.lock().unwrap();
        let intent = service.pending_l2_send_intent(&local_id).unwrap();
        assert!(matches!(
            intent.terminal_resolution,
            Some(IntentTerminalResolution::Expired { .. })
        ));
        drop(service);
        drop(state);
        cleanup(&path);
    }

    #[tokio::test]
    async fn cleans_up_terminal_intents_without_unresolved_intents() {
        let path = temp_path();
        let password = "terminal-intent-password";
        let (state, local_id) = unlocked_fixture(&path, password);
        let now = now_ms().unwrap();

        {
            let mut service = state.service.lock().unwrap();
            let wallet = service.get_wallet_mut().unwrap();
            let intent = wallet.profile.l2_send_intents.get_mut(&local_id).unwrap();
            intent.root_valid_until_ms = now.saturating_add(60_000);
            intent.terminal_resolution = Some(IntentTerminalResolution::Confirmed {
                resolved_at_ms: now.saturating_sub(61_000),
            });
        }

        {
            let service = state.service.lock().unwrap();
            assert!(service.unresolved_l2_send_intents().unwrap().is_empty());
            assert!(service
                .with_wallet(|wallet| wallet.profile.l2_send_intents.contains_key(&local_id))
                .unwrap());
        }

        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let handled = reconcile_due_intents(&state, &in_flight).await.unwrap();

        assert_eq!(handled, 0);
        let service = state.service.lock().unwrap();
        assert!(!service
            .with_wallet(|wallet| wallet.profile.l2_send_intents.contains_key(&local_id))
            .unwrap());
        drop(service);
        drop(state);
        cleanup(&path);
    }

    #[tokio::test]
    async fn test_s04_f5_expires_overdue_intent_even_when_lifecycle_cache_empty() {
        let path = temp_path();
        let password = "s04-f5-expired-no-cache";
        let (state, local_id) = unlocked_fixture(&path, password);

        state.set_cached_internal_lifecycle(None);
        {
            let mut service = state.service.lock().unwrap();
            let wallet = service.get_wallet_mut().unwrap();
            wallet
                .profile
                .l2_send_intents
                .get_mut(&local_id)
                .unwrap()
                .root_valid_until_ms = now_ms().unwrap().saturating_sub(1_000);
        }

        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        let handled = reconcile_due_intents(&state, &in_flight)
            .await
            .expect("S04-F5: overdue intent must expire even when lifecycle cache is unpopulated");
        assert_eq!(handled, 1);

        let service = state.service.lock().unwrap();
        let intent = service.pending_l2_send_intent(&local_id).unwrap();
        assert!(matches!(
            intent.terminal_resolution,
            Some(IntentTerminalResolution::Expired { .. })
        ));
        drop(service);
        drop(state);
        cleanup(&path);
    }

    #[tokio::test]
    async fn test_s04_f5_skips_forged_competing_entry_and_reconciles_valid_conflict_at_default_pow()
    {
        use human_money_core::humoco_proto::{
            causal::LOCK_ENTRY_WIRE_LEN,
            decode_hex_fixed,
            types::{LockReceipt, LockReceiptKind},
        };
        use human_money_core::models::layer2_api::{
            SyncLockBatchRequest, SyncLockBatchResponse, SyncLockConflict,
        };
        use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
        use human_money_core::wallet::types::{MultiTransferRequest, SourceTransfer};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir_alice = temp_path();
        let dir_alice_clone = temp_path();
        let dir_bob = temp_path();
        let pw = "s04-f5-forged-conflict-pw";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        svc_alice.unlock_session(pw, 600).unwrap();

        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

        let sk1 = SigningKey::from_bytes(&[91u8; 32]);
        let sk2 = SigningKey::from_bytes(&[92u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();

        let anchor_nonce = [93u8; 32];
        let creator_mig_pub = svc_alice
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![(&sk1, p1), (&sk2, p2)]
                .into_iter()
                .map(|(sk, port)| ServerEntry {
                    write_url: format!("http://127.0.0.1:{}/sync.php", port),
                    read_url: format!("http://127.0.0.1:{}/d", port),
                    admin_pub: sk.verifying_key().to_bytes(),
                    lock_pub: sk.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Creator,
                })
                .collect(),
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
        let tickets: Vec<GenesisTicket> = [&sk1, &sk2]
            .into_iter()
            .map(|sk| {
                let mut t = GenesisTicket {
                    anchor_id,
                    retain_until_quarter: retain_q,
                    issued_at: 1_760_000_000,
                    lock_pub: sk.verifying_key().to_bytes(),
                    sig: [0u8; 64],
                };
                t.sig = sk.sign(&t.digest()).to_bytes();
                t
            })
            .collect();
        svc_alice
            .register_l2_anchor_template_with_nonce(
                manifest.clone(),
                tickets,
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();
        svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
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
        let local_id = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();

        // Snapshot Alice before send to create a genuine conflicting LockEntry mined at DEFAULT_LOCK_POW_BASE_BITS (8)
        fn copy_recursive(src: &Path, dst: &Path) {
            std::fs::create_dir_all(dst).unwrap();
            for e in std::fs::read_dir(src).unwrap() {
                let e = e.unwrap();
                let target = dst.join(e.file_name());
                if e.file_type().unwrap().is_dir() {
                    copy_recursive(&e.path(), &target);
                } else {
                    std::fs::copy(e.path(), target).unwrap();
                }
            }
        }
        copy_recursive(&dir_alice, &dir_alice_clone);
        let mut svc_clone = AppService::new(&dir_alice_clone).unwrap();
        let folder = svc_alice.list_profiles().unwrap()[0].folder_name.clone();
        svc_clone
            .login(&folder, pw, false, "test-id".into())
            .unwrap();

        let alt_bundle = svc_clone
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: ACTORS.charlie.identity.user_id.clone(),
                    sources: vec![SourceTransfer {
                        local_instance_id: local_id.clone(),
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();
        let opened_alt = human_money_core::services::bundle_processor::open_and_verify_bundle(
            &ACTORS.charlie.identity,
            &alt_bundle.bundle_bytes,
        )
        .unwrap();
        let valid_comp = l2_gateway::build_voucher_lock_batch(
            &opened_alt.vouchers[0],
            0,
            &opened_alt.causal_outputs,
            opened_alt.vouchers[0]
                .transactions
                .first()
                .and_then(|t| opened_alt.root_mig_sig_for(&t.t_id)),
            DEFAULT_LOCK_POW_BASE_BITS,
        )
        .unwrap()
        .into_iter()
        .last()
        .unwrap();
        let mut forged_comp = valid_comp.clone();
        forged_comp.causal_sig = [0x42u8; 64];

        // Create outbound DurableIntent on Alice (sent to Bob), aged past 30s delay
        let _ = svc_alice
            .create_transfer_bundle_with_l2_intents_and_companions(
                MultiTransferRequest {
                    recipient_id: bob_id,
                    sources: vec![SourceTransfer {
                        local_instance_id: local_id,
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
                DEFAULT_LOCK_POW_BASE_BITS,
                |_, _| Ok(vec![]),
            )
            .unwrap();
        let intent_id = svc_alice.unresolved_l2_send_intents().unwrap()[0]
            .local_instance_id
            .clone();
        {
            let wallet = svc_alice.get_wallet_mut().unwrap();
            wallet
                .profile
                .l2_send_intents
                .get_mut(&intent_id)
                .unwrap()
                .created_at_ms = now_ms().unwrap().saturating_sub(60_000);
        }

        // Server 1 returns FORGED competing entry; Server 2 returns VALID competing entry (8-bit PoW)
        let spawn_conflict = |listener: tokio::net::TcpListener,
                              sk: SigningKey,
                              comp: LockEntry| {
            tokio::spawn(async move {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let mut buf = [0u8; 65_536];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
                    let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
                    let _ = decode_hex_fixed::<LOCK_ENTRY_WIRE_LEN>(&req.entries[0]).unwrap();
                    let mut r = LockReceipt {
                        kind: LockReceiptKind::ConflictSeen,
                        lock_pub: sk.verifying_key().to_bytes(),
                        bucket: req.bucket,
                        slot: comp.slot,
                        child_lock: comp.child_lock,
                        entry_hash: comp.entry_hash().unwrap(),
                        retain_until_quarter: retain_q,
                        write_seq: 1,
                        bucket_head: [7u8; 32],
                        recipient_nonce: req.recipient_nonce,
                        sig: [0u8; 64],
                    };
                    r.sig = sk.sign(&r.digest()).to_bytes();
                    let resp = serde_json::to_string(&SyncLockBatchResponse {
                        status: "conflict".to_string(),
                        bucket: req.bucket,
                        retain_until_quarter: retain_q,
                        receipts: vec![],
                        conflict: Some(SyncLockConflict {
                            entry_index: 0,
                            slot: comp.slot,
                            receipt: r,
                            competing_entry: comp.to_wire_hex().unwrap(),
                        }),
                    })
                    .unwrap();
                    let http = format!(
                        "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        resp.len(),
                        resp
                    );
                    let _ = stream.write_all(http.as_bytes()).await;
                }
            })
        };
        let h1 = spawn_conflict(l1, sk1.clone(), forged_comp);
        let h2 = spawn_conflict(l2, sk2.clone(), valid_comp);

        // High live pow_base_bits=18 in probe cache must NOT prevent verifying 8-bit historical conflict
        let mut lc = InternalLifecycleState::default();
        let sk1_hex = hex::encode(sk1.verifying_key().as_bytes());
        lc.l2_probe_cache.insert(
            sk1_hex.clone(),
            sync_client::FrontendLockServerProbeResult {
                write_url: format!("http://127.0.0.1:{}/sync.php", p1),
                read_url: format!("http://127.0.0.1:{}/d", p1),
                drop_pub: sk1_hex.clone(),
                admin_pub: sk1_hex.clone(),
                lock_pub: sk1_hex,
                delegation_valid: true,
                waf_probe_passed: true,
                selftest_passed: true,
                key_leak_safe: true,
                passed: true,
                latency_ms: 5,
                max_retention_quarter: retain_q,
                pow_base_bits: 18,
                allow_pow_tickets: true,
                ticket_pow_bits: 16,
                inode_warning: false,
                used_inodes: 0,
                max_inodes: 50000,
                probed_at_secs: 1,
                delegation: None,
                error: None,
            },
        );
        let state = state_with(svc_alice, Some(lc));
        let in_flight = Arc::new(Mutex::new(HashSet::new()));

        let handled = reconcile_due_intents(&state, &in_flight).await.unwrap();
        assert_eq!(handled, 1);

        let service = state.service.lock().unwrap();
        let intent = service.pending_l2_send_intent(&intent_id).unwrap();
        assert!(
            matches!(
                intent.terminal_resolution,
                Some(IntentTerminalResolution::Conflicted { .. })
            ),
            "S04-F5: intent must transition to Conflicted using valid entry at index 1 despite forged index 0 and high live PoW; got {:?}",
            intent.terminal_resolution
        );

        h1.abort();
        h2.abort();
        drop(service);
        drop(state);
        drop(svc_clone);
        drop(svc_bob);
        cleanup(&dir_alice);
        cleanup(&dir_alice_clone);
        cleanup(&dir_bob);
    }
}
