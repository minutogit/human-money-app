use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use human_money_core::models::l2_intent::L2SendIntent;
use tauri::Manager;

use crate::{models::FrontendError, AppState};

const RECONCILIATION_DELAY_MS: u64 = 30_000;
const TERMINAL_RETENTION_MS: u64 = 60_000;
const RECONCILIATION_INTERVAL_SECS: u64 = 15;

fn wallet_unavailable() -> FrontendError {
    FrontendError::from("Wallet service unavailable".to_string())
}

fn now_ms() -> Result<u64, FrontendError> {
    u64::try_from(Utc::now().timestamp_millis())
        .map_err(|_| FrontendError::from("Invalid verification time".to_string()))
}

async fn reconcile_due_intent(
    state: &AppState,
    intent: &L2SendIntent,
    owner: &str,
    candidates: &[String],
    now: u64,
) -> Result<(), FrontendError> {
    let local_id = &intent.local_instance_id;

    if !intent.retained_original_observed {
        let tag = {
            let service = state.service.lock().map_err(|_| wallet_unavailable())?;
            match service.retained_choice_target(local_id, now) {
                Ok(tag) => Some(tag),
                Err(error) => {
                    log::warn!(
                        "Retained choice target unavailable for {}: {}",
                        local_id,
                        error
                    );
                    None
                }
            }
        };
        if let Some(tag) = tag {
            match crate::gateway_resolver::fetch_retained(candidates, &tag).await {
                Ok(Some(bytes)) => {
                    let mut service = state.service.lock().map_err(|_| wallet_unavailable())?;
                    if let Err(error) = service.record_retained_choice_observation(
                        local_id, &bytes, now, None,
                    ) {
                        log::warn!(
                            "Retained choice observation rejected for {}: {}",
                            local_id,
                            error
                        );
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    log::warn!("Retained choice fetch failed for {}: {}", local_id, error);
                }
            }
        }
    }

    let (view_owner, views) = crate::checkout::ensure_cached_views(state).await?;
    if view_owner != owner {
        return Ok(());
    }

    let context = {
        let mut service = state.service.lock().map_err(|_| wallet_unavailable())?;
        service
            .begin_intent_reconciliation(local_id, &views, now, None)
            .map_err(FrontendError::from)?
    };

    let Some(proof) =
        crate::gateway_resolver::fetch_live_proof(candidates, &context)
            .await
            .map_err(FrontendError::from)?
    else {
        return Ok(());
    };

    let mut service = state.service.lock().map_err(|_| wallet_unavailable())?;
    let current_owner = service
        .with_wallet(|wallet| wallet.get_user_id().to_string())
        .map_err(FrontendError::from)?;
    if current_owner != owner {
        return Ok(());
    }
    if let Err(error) = service.complete_intent_reconciliation(
        local_id, &proof, &views, now, None,
    ) {
        log::warn!(
            "Live intent reconciliation failed for {}: {}",
            local_id,
            error
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
        if let Err(error) =
            service.cleanup_terminal_intents(TERMINAL_RETENTION_MS, now, None)
        {
            log::warn!(
                "Failed to clean up terminal L2 send intents: {}",
                error
            );
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
    let candidates = crate::gateway_resolver::cached_candidates(&settings);
    let mut handled = 0usize;

    for intent in intents {
        let local_id = intent.local_instance_id.clone();

        if now >= intent.root_valid_until_ms {
            let mut service = state.service.lock().map_err(|_| wallet_unavailable())?;
            if let Err(error) = service.expire_intent_if_overdue(&local_id, now, None) {
                log::warn!("Failed to expire L2 send intent {}: {}", local_id, error);
            }
            if let Err(error) =
                service.cleanup_terminal_intents(TERMINAL_RETENTION_MS, now, None)
            {
                log::warn!(
                    "Failed to clean up terminal L2 send intents: {}",
                    error
                );
            }
            handled += 1;
            continue;
        }

        if now < intent.created_at_ms.saturating_add(RECONCILIATION_DELAY_MS) {
            continue;
        }

        {
            let mut in_flight = in_flight.lock().map_err(|_| {
                FrontendError::from("Intent reconciliation state unavailable".to_string())
            })?;
            if !in_flight.insert(local_id.clone()) {
                continue;
            }
        }

        let result =
            reconcile_due_intent(state, &intent, &owner, &candidates, now).await;

        if let Ok(mut in_flight) = in_flight.lock() {
            in_flight.remove(&local_id);
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
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            RECONCILIATION_INTERVAL_SECS,
        ));
        interval.tick().await;
        loop {
            interval.tick().await;
            let state = app.state::<AppState>();
            match reconcile_due_intents(state.inner(), &in_flight).await {
                Ok(0) => {}
                Ok(count) => {
                    log::info!("Reconciled {} unresolved L2 send intent(s)", count)
                }
                Err(error) => {
                    log::warn!("L2 intent reconciliation failed: {}", error.message)
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppSettings;
    use human_money_core::{
        app_service::{AppService, IntentTerminalResolution},
        models::{profile::PublicProfile, voucher::ValueDefinition},
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

    fn state_with(
        service: AppService,
        settings: Option<AppSettings>,
    ) -> AppState {
        AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(settings),
            regional_views: Mutex::new(None),
        }
    }

    fn unlocked_fixture(path: &Path, password: &str) -> (AppState, String) {
        let (mut service, _) = test_utils::setup_service_with_profile(
            path,
            &ACTORS.test_user,
            "Intent Reconciliation",
            password,
        );
        service.unlock_session(password, 600).unwrap();

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
            .prepare_l2_send_intent(&local_id, 0, Some(password))
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
            let intent = wallet
                .profile
                .l2_send_intents
                .get_mut(&local_id)
                .unwrap();
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
