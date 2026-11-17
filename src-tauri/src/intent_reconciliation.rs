//! Background worker driving mandatory `1..=N` committee completion for pending outbound Sync Lock sends.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use human_money_core::humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS;
use human_money_core::models::l2_intent::L2SendIntent;
use rand::RngCore;
use tauri::Manager;

use crate::{models::FrontendError, settings::AppSettings, sync_client, AppState};

const RECONCILIATION_DELAY_MS: u64 = 30_000;
const TERMINAL_RETENTION_MS: u64 = 60_000;
const RECONCILIATION_INTERVAL_SECS: u64 = 15;
const INTENT_DISPATCH_TIMEOUT: Duration = Duration::from_secs(4);

fn wallet_unavailable() -> FrontendError {
    FrontendError::from("Wallet service unavailable".to_string())
}

fn now_ms() -> Result<u64, FrontendError> {
    u64::try_from(Utc::now().timestamp_millis())
        .map_err(|_| FrontendError::from("Invalid verification time".to_string()))
}

fn random_non_zero_nonce() -> [u8; 32] {
    loop {
        let mut nonce = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        if nonce != [0u8; 32] {
            return nonce;
        }
    }
}

fn determine_intent_pow_base_bits(settings: &AppSettings, intent: &L2SendIntent) -> u32 {
    let mut bits = DEFAULT_LOCK_POW_BASE_BITS;
    for srv in &intent.manifest.servers {
        let norm = crate::checkout::normalize_sync_write_url(&srv.write_url);
        if let Some(probe) = settings.l2_probe_cache.get(&norm) {
            bits = bits.max(probe.pow_base_bits);
        }
    }
    bits
}

async fn reconcile_due_intent(
    state: &AppState,
    settings: &AppSettings,
    intent: &L2SendIntent,
    owner: &str,
    now: u64,
) -> Result<(), FrontendError> {
    let local_id = &intent.local_instance_id;
    let nonce = random_non_zero_nonce();

    let requests = {
        let service = state.service.lock().map_err(|_| wallet_unavailable())?;
        let current_owner = service
            .with_wallet(|wallet| wallet.get_user_id().to_string())
            .map_err(FrontendError::from)?;
        if current_owner != owner {
            return Ok(());
        }
        service
            .build_intent_committee_requests(local_id, &nonce, &HashMap::new(), true)
            .map_err(FrontendError::from)?
    };

    if requests.is_empty() {
        return Ok(());
    }

    let client = sync_client::create_sync_http_client();
    let mut dispatch = crate::gateway_resolver::dispatch_lock_committee_batches(
        &client,
        &requests,
        INTENT_DISPATCH_TIMEOUT,
    )
    .await;

    // If a committee server rotated to a newer manifest, adopt it and retry missing servers once
    let mut best_upgraded: Option<human_money_core::humoco_proto::manifest::AnchorManifest> = None;
    for m in &dispatch.epoch_frozen_manifests {
        if m.revision > intent.manifest.revision
            && m.verify_anchor_id(&intent.anchor_id).is_ok()
            && best_upgraded
                .as_ref()
                .map(|b| m.revision > b.revision)
                .unwrap_or(true)
        {
            best_upgraded = Some(m.clone());
        }
    }
    if let Some(newer_manifest) = best_upgraded {
        let upgraded_reqs = {
            let mut service = state.service.lock().map_err(|_| wallet_unavailable())?;
            if service
                .update_l2_anchor_manifest(newer_manifest, None)
                .is_ok()
            {
                service
                    .build_intent_committee_requests(local_id, &nonce, &HashMap::new(), true)
                    .ok()
            } else {
                None
            }
        };
        if let Some(next_reqs) = upgraded_reqs {
            if !next_reqs.is_empty() {
                let extra = crate::gateway_resolver::dispatch_lock_committee_batches(
                    &client,
                    &next_reqs,
                    INTENT_DISPATCH_TIMEOUT,
                )
                .await;
                dispatch.receipts.extend(extra.receipts);
                dispatch.competing_entries.extend(extra.competing_entries);
            }
        }
    }

    let target_slot = intent.entries.last().map(|e| e.slot);
    let target_receipts: Vec<_> = dispatch
        .receipts
        .iter()
        .filter(|r| target_slot == Some(r.slot))
        .cloned()
        .collect();

    if target_receipts.is_empty() && dispatch.competing_entries.is_empty() {
        return Ok(());
    }

    let pow_base_bits = determine_intent_pow_base_bits(settings, intent);
    let mut service = state.service.lock().map_err(|_| wallet_unavailable())?;
    let current_owner = service
        .with_wallet(|wallet| wallet.get_user_id().to_string())
        .map_err(FrontendError::from)?;
    if current_owner != owner {
        return Ok(());
    }

    let (resolution_opt, equivocation_opt) = service
        .record_intent_server_receipts(
            local_id,
            &nonce,
            &target_receipts,
            dispatch.competing_entries.first(),
            pow_base_bits,
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
        let _ = crate::l2_lifecycle::record_equivocation_proof(
            &mut service,
            None,
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

    let (owner, intents) = {
        let mut service = state.service.lock().map_err(|_| wallet_unavailable())?;
        let owner = match service.with_wallet(|wallet| wallet.get_user_id().to_string()) {
            Ok(owner) => owner,
            Err(_) => return Ok(0),
        };
        if let Err(error) = service.cleanup_terminal_intents(TERMINAL_RETENTION_MS, now, None) {
            log::debug!("Failed to clean up terminal L2 send intents: {}", error);
        }
        let intents = service
            .unresolved_l2_send_intents()
            .map_err(FrontendError::from)?;
        (owner, intents)
    };

    if intents.is_empty() {
        return Ok(0);
    }

    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let mut handled = 0usize;

    for intent in intents {
        let local_id = intent.local_instance_id.clone();

        if now >= intent.root_valid_until_ms {
            let mut service = state.service.lock().map_err(|_| wallet_unavailable())?;
            if let Err(error) = service.expire_intent_if_overdue(&local_id, now, None) {
                log::debug!("Failed to expire L2 send intent {}: {}", local_id, error);
            }
            if let Err(error) = service.cleanup_terminal_intents(TERMINAL_RETENTION_MS, now, None) {
                log::debug!("Failed to clean up terminal L2 send intents: {}", error);
            }
            handled += 1;
            continue;
        }

        if now < intent.created_at_ms.saturating_add(RECONCILIATION_DELAY_MS) {
            continue;
        }

        {
            let mut in_flight_guard = in_flight.lock().map_err(|_| {
                FrontendError::from("Intent reconciliation state unavailable".to_string())
            })?;
            if !in_flight_guard.insert(local_id.clone()) {
                continue;
            }
        }

        let result = reconcile_due_intent(state, &settings, &intent, &owner, now).await;

        if let Ok(mut in_flight_guard) = in_flight.lock() {
            in_flight_guard.remove(&local_id);
        }

        match result {
            Ok(()) => handled += 1,
            Err(error) => log::debug!(
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

    fn state_with(service: AppService, settings: Option<AppSettings>) -> AppState {
        let state = AppState::new(service);
        *state.settings.lock().unwrap() = settings;
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

        (state_with(service, Some(AppSettings::default())), local_id)
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
}
