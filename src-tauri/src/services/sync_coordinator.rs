//! Central background sync coordinator (H10).
//!
//! Replaces the four parallel frontend `setInterval`/`setTimeout` sync loops (`sealSync`,
//! `autoBackup`, `postSync`, `marketSync`) with a single backend Tokio worker owned by `AppState`
//! and spawned once during Tauri setup. All passes bind to `AppState::session_generation` and
//! `owner_did` and discard results if the user logs out or switches profiles while HTTP requests
//! are in flight.

use std::time::{Duration, Instant};

use tauri::{Emitter, Manager};

use crate::models::MarketSyncStatus;
use crate::services::{backup_service, l2_anchor_service, post_service, sync_engine};
use crate::state::{AppState, SyncWakeRequest};

pub const SEAL_CHECK_INTERVAL: Duration = Duration::from_secs(30);
pub const SEAL_INITIAL_DELAY: Duration = Duration::from_secs(1);

pub const LOCAL_BACKUP_INITIAL_DELAY: Duration = Duration::from_secs(3);
pub const LOCAL_BACKUP_PERIODIC_INTERVAL: Duration = Duration::from_secs(60);
pub const LOCAL_BACKUP_DEBOUNCE: Duration = Duration::from_secs(2);

pub const SYNC_BACKUP_INITIAL_DELAY: Duration = Duration::from_secs(3);
pub const SYNC_BACKUP_DAILY_INTERVAL: Duration = Duration::from_secs(86_400);
pub const SYNC_BACKUP_DEBOUNCE: Duration = Duration::from_secs(30);

pub const POST_INITIAL_DELAY: Duration = Duration::from_secs(1);
pub const POST_PERIODIC_INTERVAL: Duration = Duration::from_secs(90);

pub const MARKET_INITIAL_DELAY: Duration = Duration::from_secs(2);
pub const MARKET_PERIODIC_INTERVAL: Duration = Duration::from_secs(20 * 60);
pub const MARKET_UPLOAD_DEBOUNCE: Duration = Duration::from_secs(3);

/// Pure schedule state machine for the central background sync coordinator.
#[derive(Debug, Clone, Default)]
pub(crate) struct CoordinatorSchedule {
    pub active_generation: u64,
    pub next_seal_at: Option<Instant>,
    pub next_local_backup_at: Option<Instant>,
    pub next_sync_backup_at: Option<Instant>,
    pub next_sync_backup_forced: bool,
    pub sync_backup_deferred_for_wifi: bool,
    pub last_sync_backup_success_at: Option<Instant>,
    pub sync_backup_consecutive_failures: u32,
    pub sync_backup_no_targets: bool,
    pub next_post_at: Option<Instant>,
    pub post_deferred_for_wifi: bool,
    pub next_sync_members_at: Option<Instant>,
    pub next_market_at: Option<Instant>,
    pub next_market_force_upload: bool,
    pub market_deferred_for_wifi: bool,
    pub lock_refresh_done: bool,
}

impl CoordinatorSchedule {
    pub fn reset_for_session(&mut self, generation: u64, now: Instant) {
        self.active_generation = generation;
        self.next_seal_at = Some(now + SEAL_INITIAL_DELAY);
        self.next_local_backup_at = Some(now + LOCAL_BACKUP_INITIAL_DELAY);
        self.next_sync_backup_at = Some(now + SYNC_BACKUP_INITIAL_DELAY);
        self.next_sync_backup_forced = false;
        self.sync_backup_deferred_for_wifi = false;
        self.last_sync_backup_success_at = None;
        self.sync_backup_consecutive_failures = 0;
        self.sync_backup_no_targets = false;
        self.next_post_at = Some(now + POST_INITIAL_DELAY);
        self.post_deferred_for_wifi = false;
        self.next_sync_members_at = None;
        self.next_market_at = Some(now + MARKET_INITIAL_DELAY);
        self.next_market_force_upload = false;
        self.market_deferred_for_wifi = false;
        self.lock_refresh_done = false;
    }

    pub fn clear(&mut self) {
        self.active_generation = 0;
        self.next_seal_at = None;
        self.next_local_backup_at = None;
        self.next_sync_backup_at = None;
        self.next_sync_backup_forced = false;
        self.sync_backup_deferred_for_wifi = false;
        self.last_sync_backup_success_at = None;
        self.sync_backup_consecutive_failures = 0;
        self.sync_backup_no_targets = false;
        self.next_post_at = None;
        self.post_deferred_for_wifi = false;
        self.next_sync_members_at = None;
        self.next_market_at = None;
        self.next_market_force_upload = false;
        self.market_deferred_for_wifi = false;
        self.lock_refresh_done = false;
    }

    pub fn promote_wifi_deferred(&mut self, is_online: bool, is_wifi: bool, now: Instant) {
        if !(is_online && is_wifi) {
            return;
        }
        if std::mem::take(&mut self.sync_backup_deferred_for_wifi) {
            self.next_sync_backup_at = Some(now);
        }
        if std::mem::take(&mut self.post_deferred_for_wifi) {
            self.next_post_at = Some(now);
        }
        if std::mem::take(&mut self.market_deferred_for_wifi) {
            self.next_market_at = Some(now);
        }
    }

    pub fn apply_wake(&mut self, wake: SyncWakeRequest, now: Instant) {
        if wake.seal {
            self.next_seal_at = Some(now);
        }
        if wake.local_backup {
            let target = now + LOCAL_BACKUP_DEBOUNCE;
            self.next_local_backup_at = Some(
                self.next_local_backup_at
                    .map_or(target, |prev| prev.min(target)),
            );
        }
        if wake.sync_backup || wake.sync_backup_force {
            self.sync_backup_no_targets = false;
            self.sync_backup_consecutive_failures = 0;
            if wake.sync_backup_force {
                self.next_sync_backup_forced = true;
            }
            let target = if wake.sync_backup_force {
                now + SYNC_BACKUP_DEBOUNCE
            } else {
                now
            };
            self.next_sync_backup_at = Some(
                self.next_sync_backup_at
                    .map_or(target, |prev| prev.min(target)),
            );
        }
        if wake.post {
            self.next_post_at = Some(now);
        }
        if wake.sync_members {
            self.next_sync_members_at = Some(now);
        }
        if wake.market || wake.market_force_upload {
            if wake.market_force_upload {
                self.next_market_force_upload = true;
                let target = now + MARKET_UPLOAD_DEBOUNCE;
                self.next_market_at =
                    Some(self.next_market_at.map_or(target, |prev| prev.min(target)));
            } else {
                self.next_market_at = Some(now);
            }
        }
    }

    /// Computes the earliest wakeup `Instant` across due local/network tasks.
    pub fn earliest_due(&self, is_online: bool) -> Option<Instant> {
        let mut earliest = self.next_local_backup_at;
        if let Some(seal_at) = self.next_seal_at {
            earliest = Some(earliest.map_or(seal_at, |e| e.min(seal_at)));
        }

        if is_online {
            if let Some(at) = self.next_post_at {
                earliest = Some(earliest.map_or(at, |e| e.min(at)));
            }
            if let Some(at) = self.next_sync_backup_at {
                earliest = Some(earliest.map_or(at, |e| e.min(at)));
            }
            if let Some(at) = self.next_sync_members_at {
                earliest = Some(earliest.map_or(at, |e| e.min(at)));
            }
            if let Some(at) = self.next_market_at {
                earliest = Some(earliest.map_or(at, |e| e.min(at)));
            }
        }

        earliest
    }
}

/// Spawns the central background sync coordinator task. Must only be called from `lib.rs::setup()`
/// after `app.manage(app_state)` has been registered (never from `AppState::new`).
pub fn spawn_background_sync_coordinator(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        run_coordinator_loop(app).await;
    });
}

fn active_session_generation(state: &AppState) -> Option<u64> {
    let service = state.lock_service().ok()?;
    if service.is_wallet_unlocked() && service.is_session_active() {
        Some(state.current_session_generation())
    } else {
        None
    }
}

fn is_session_active_for_generation(state: &AppState, generation: u64) -> bool {
    active_session_generation(state) == Some(generation)
}

async fn wait_for_next_event(
    next_due: Option<Instant>,
    gen_rx: &mut tokio::sync::watch::Receiver<u64>,
    wake: &tokio::sync::Notify,
) {
    match next_due {
        Some(deadline) => {
            tokio::select! {
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {}
                _ = gen_rx.changed() => {}
                _ = wake.notified() => {}
            }
        }
        None => {
            tokio::select! {
                _ = gen_rx.changed() => {}
                _ = wake.notified() => {}
            }
        }
    }
}

async fn run_coordinator_loop(app: tauri::AppHandle) {
    let state_ref = app.state::<AppState>();
    let mut gen_rx = state_ref.subscribe_session_generation();
    let wake = state_ref.sync_coordinator_wake.clone();
    let mut schedule = CoordinatorSchedule::default();

    loop {
        let state = app.state::<AppState>();
        let Some(generation) = active_session_generation(&state) else {
            schedule.clear();
            wait_for_next_event(None, &mut gen_rx, &wake).await;
            continue;
        };

        let now = Instant::now();
        if schedule.active_generation != generation {
            schedule.reset_for_session(generation, now);
        }

        let (ctrl, pending_wake) = state.take_sync_coordinator_snapshot();
        if !pending_wake.is_empty() {
            schedule.apply_wake(pending_wake, now);
        }
        schedule.promote_wifi_deferred(ctrl.is_online, ctrl.is_wifi, now);

        // 1. WalletSeal check (local check; triggers backup immediately when PendingUpload is detected)
        if schedule.next_seal_at.is_some_and(|t| now >= t) {
            schedule.next_seal_at = Some(now + SEAL_CHECK_INTERVAL);
            if let Ok(status) = l2_anchor_service::get_seal_sync_status_impl(&state) {
                if status == "PendingUpload" {
                    schedule.next_local_backup_at = Some(now);
                    if !schedule.sync_backup_deferred_for_wifi
                        && !schedule.sync_backup_no_targets
                        && schedule.sync_backup_consecutive_failures == 0
                    {
                        schedule.next_sync_backup_forced = true;
                        schedule.next_sync_backup_at = Some(now);
                    }
                }
            }
        }

        // 2. Local folder Auto-Backup (runs offline as well, executes sync I/O on blocking pool)
        if schedule.next_local_backup_at.is_some_and(|t| now >= t)
            && is_session_active_for_generation(&state, generation)
        {
            schedule.next_local_backup_at = Some(now + LOCAL_BACKUP_PERIODIC_INTERVAL);
            let app_clone = app.clone();
            let res = tokio::task::spawn_blocking(move || {
                let s = app_clone.state::<AppState>();
                backup_service::run_auto_backup_sync_impl(&s, &app_clone)
            })
            .await;
            if is_session_active_for_generation(&state, generation) {
                if let Ok(Ok(dto)) = res {
                    let _ = app.emit("humoco://backup-updated", &dto);
                }
            } else {
                continue;
            }
        }

        // 3. Off-site Sync Backup (requires online; per-origin 429 cooldowns are enforced per mirror)
        let ctrl = state.sync_control_snapshot();
        if ctrl.is_online
            && schedule
                .next_sync_backup_at
                .is_some_and(|t| Instant::now() >= t)
            && is_session_active_for_generation(&state, generation)
        {
            let forced = std::mem::take(&mut schedule.next_sync_backup_forced);
            let now_run = Instant::now();
            schedule.next_sync_backup_at = Some(now_run + SYNC_BACKUP_DAILY_INTERVAL);
            let throttled = !forced
                && schedule.last_sync_backup_success_at.is_some_and(|last| {
                    now_run.saturating_duration_since(last) < SYNC_BACKUP_DAILY_INTERVAL
                });
            if !throttled {
                match backup_service::sync_backup_impl(None, Some(ctrl.is_wifi), &state, &app).await
                {
                    Ok(res) if is_session_active_for_generation(&state, generation) => {
                        if res.skipped_wifi_only {
                            if forced {
                                schedule.next_sync_backup_forced = true;
                            }
                            schedule.sync_backup_deferred_for_wifi = true;
                        } else if res.authorized_targets == 0 && res.errors.is_empty() {
                            schedule.sync_backup_no_targets = true;
                            schedule.next_sync_backup_at = None;
                            schedule.sync_backup_consecutive_failures = 0;
                        } else {
                            if res.uploaded_to > 0 || res.errors.is_empty() {
                                schedule.last_sync_backup_success_at = Some(Instant::now());
                                schedule.sync_backup_consecutive_failures = 0;
                            } else {
                                schedule.sync_backup_consecutive_failures =
                                    schedule.sync_backup_consecutive_failures.saturating_add(1);
                                let exp = schedule
                                    .sync_backup_consecutive_failures
                                    .saturating_sub(1)
                                    .min(10);
                                let backoff_secs = (30u64).saturating_mul(1u64 << exp).min(86_400);
                                schedule.next_sync_backup_at =
                                    Some(Instant::now() + Duration::from_secs(backoff_secs));
                            }
                            let _ = app.emit("humoco://backup-updated", serde_json::Value::Null);
                            let _ = app.emit("humoco://sync-status-updated", &res);
                        }
                    }
                    Err(_e) if is_session_active_for_generation(&state, generation) => {
                        schedule.sync_backup_consecutive_failures =
                            schedule.sync_backup_consecutive_failures.saturating_add(1);
                        let exp = schedule
                            .sync_backup_consecutive_failures
                            .saturating_sub(1)
                            .min(10);
                        let backoff_secs = (30u64).saturating_mul(1u64 << exp).min(86_400);
                        schedule.next_sync_backup_at =
                            Some(Instant::now() + Duration::from_secs(backoff_secs));
                    }
                    _ => {}
                }
            }
        }

        if !is_session_active_for_generation(&state, generation) {
            continue;
        }

        // 4. PoST Channel Sync (per-origin 429 cooldowns are enforced per endpoint inside PoST)
        let ctrl = state.sync_control_snapshot();
        let now_post = Instant::now();
        if ctrl.is_online && schedule.next_post_at.is_some_and(|t| now_post >= t) {
            schedule.next_post_at = Some(now_post + POST_PERIODIC_INTERVAL);
            if let Ok(res) = post_service::sync_post_channels_impl(
                Some(&app),
                Some(ctrl.is_wifi),
                None,
                None,
                &state,
            )
            .await
            {
                if is_session_active_for_generation(&state, generation) {
                    if res.skipped_wifi_only {
                        schedule.post_deferred_for_wifi = true;
                    }
                    let has_backup_changes = res.uploaded_to > 0
                        || res.imported_payments > 0
                        || res.imported_receipts > 0
                        || res.imported_signatures > 0
                        || res.new_sign_requests > 0
                        || res.new_wot_handshakes > 0
                        || res.new_inquiries > 0
                        || res.new_inquiry_messages > 0
                        || res.new_tolerance_payments > 0;
                    if has_backup_changes {
                        schedule.apply_wake(
                            SyncWakeRequest {
                                local_backup: true,
                                sync_backup: true,
                                sync_backup_force: true,
                                ..Default::default()
                            },
                            Instant::now(),
                        );
                    }
                    let _ = app.emit("humoco://post-synced", &res);
                }
            }
        }

        if !is_session_active_for_generation(&state, generation) {
            continue;
        }

        // 4b. Background channel members push (serialized before Market sync)
        let ctrl = state.sync_control_snapshot();
        let now_members = Instant::now();
        if ctrl.is_online
            && schedule
                .next_sync_members_at
                .is_some_and(|t| now_members >= t)
            && is_session_active_for_generation(&state, generation)
        {
            schedule.next_sync_members_at = None;
            if let Ok(session) =
                state.authorize_and_begin_session(crate::state::RequiredAccess::Unlocked, None)
            {
                let client = crate::sync_client::create_sync_http_client();
                if let Some(err_msg) =
                    sync_engine::admin::refresh_own_admin_members(&client, &state, &session, None)
                        .await
                {
                    log::warn!("Background admin member refresh warning: {}", err_msg);
                }
            }
        }

        if !is_session_active_for_generation(&state, generation) {
            continue;
        }

        // 5. Market (`card` + `wot`) Channel Sync
        let ctrl = state.sync_control_snapshot();
        let now_market = Instant::now();
        if ctrl.is_online && schedule.next_market_at.is_some_and(|t| now_market >= t) {
            let force_upload = schedule.next_market_force_upload;
            schedule.next_market_force_upload = false;
            schedule.next_market_at = Some(now_market + MARKET_PERIODIC_INTERVAL);
            if let Ok(res) = sync_engine::sync_market_channels_with_options(
                Some(ctrl.is_wifi),
                Some(force_upload),
                Some(!force_upload),
                None,
                &state,
            )
            .await
            {
                if is_session_active_for_generation(&state, generation) {
                    if res.status == MarketSyncStatus::SkippedWifiOnly {
                        if force_upload {
                            schedule.next_market_force_upload = true;
                        }
                        schedule.market_deferred_for_wifi = true;
                    }
                    let _ = app.emit("humoco://market-synced", &res);
                }
            }
        }

        // 6. One-shot per-session Sync-Lock probe refresh & silent offline anchor pre-warm
        let ctrl = state.sync_control_snapshot();
        if ctrl.is_online
            && !schedule.lock_refresh_done
            && is_session_active_for_generation(&state, generation)
            && l2_anchor_service::refresh_lock_servers_for_coordinator(&state).await
        {
            schedule.lock_refresh_done = true;
        }

        let ctrl = state.sync_control_snapshot();
        let next_due = schedule.earliest_due(ctrl.is_online);
        wait_for_next_event(next_due, &mut gen_rx, &wake).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDirGuard(std::path::PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn h10_schedule_resets_per_session_debounces_wakes_and_respects_offline_cooldown() {
        let t0 = Instant::now();
        let mut sched = CoordinatorSchedule::default();

        sched.reset_for_session(1, t0);
        assert_eq!(sched.active_generation, 1);
        assert_eq!(sched.next_seal_at, Some(t0 + SEAL_INITIAL_DELAY));
        assert_eq!(
            sched.next_local_backup_at,
            Some(t0 + LOCAL_BACKUP_INITIAL_DELAY)
        );
        assert_eq!(sched.next_post_at, Some(t0 + POST_INITIAL_DELAY));
        assert_eq!(sched.next_market_at, Some(t0 + MARKET_INITIAL_DELAY));

        // Offline gating keeps local backup + seal due, skipping remote jobs until back online
        let due_offline = sched.earliest_due(false).unwrap();
        assert_eq!(due_offline, t0 + SEAL_INITIAL_DELAY);

        // Online includes both remote and local jobs, picking whichever is earliest
        sched.next_seal_at = Some(t0 + Duration::from_secs(120));
        sched.next_local_backup_at = Some(t0 + Duration::from_secs(5));
        assert_eq!(sched.earliest_due(true), Some(t0 + POST_INITIAL_DELAY));
        sched.next_post_at = Some(t0 + Duration::from_secs(120));
        sched.next_market_at = Some(t0 + Duration::from_secs(120));
        sched.next_sync_backup_at = Some(t0 + Duration::from_secs(120));
        assert_eq!(sched.earliest_due(true), Some(t0 + Duration::from_secs(5)));

        // Forced market upload wake uses MARKET_UPLOAD_DEBOUNCE
        let t1 = t0 + Duration::from_secs(10);
        sched.next_local_backup_at = Some(t1 + LOCAL_BACKUP_PERIODIC_INTERVAL);
        sched.next_market_at = Some(t1 + MARKET_PERIODIC_INTERVAL);
        sched.apply_wake(
            SyncWakeRequest {
                local_backup: true,
                sync_backup: true,
                sync_backup_force: true,
                market: true,
                market_force_upload: true,
                ..Default::default()
            },
            t1,
        );
        assert!(sched.next_sync_backup_forced);
        assert!(sched.next_market_force_upload);
        assert_eq!(sched.next_local_backup_at, Some(t1 + LOCAL_BACKUP_DEBOUNCE));
        assert_eq!(sched.next_market_at, Some(t1 + MARKET_UPLOAD_DEBOUNCE));

        // Session logout/switch clears all pending debounces and resets generation
        sched.clear();
        assert_eq!(sched.active_generation, 0);
        assert!(!sched.next_sync_backup_forced);
        assert!(!sched.next_market_force_upload);
        assert!(!sched.sync_backup_deferred_for_wifi);
        assert!(!sched.post_deferred_for_wifi);
        assert!(!sched.market_deferred_for_wifi);
        assert_eq!(sched.earliest_due(true), None);
    }

    #[test]
    fn f4_wifi_deferred_jobs_promote_immediately_when_wifi_returns() {
        let t0 = Instant::now();
        let mut sched = CoordinatorSchedule::default();
        sched.reset_for_session(1, t0);
        sched.next_sync_backup_at = Some(t0 + SYNC_BACKUP_DAILY_INTERVAL);
        sched.next_post_at = Some(t0 + POST_PERIODIC_INTERVAL);
        sched.next_market_at = Some(t0 + MARKET_PERIODIC_INTERVAL);
        sched.sync_backup_deferred_for_wifi = true;
        sched.next_sync_backup_forced = true;
        sched.post_deferred_for_wifi = true;
        sched.market_deferred_for_wifi = true;
        sched.next_market_force_upload = true;

        let t1 = t0 + Duration::from_secs(10);
        // Online on cellular (!is_wifi): deferred flags stay queued, timers stay at long interval.
        sched.promote_wifi_deferred(true, false, t1);
        assert!(sched.sync_backup_deferred_for_wifi);
        assert!(sched.post_deferred_for_wifi);
        assert!(sched.market_deferred_for_wifi);
        assert_eq!(sched.next_market_at, Some(t0 + MARKET_PERIODIC_INTERVAL));

        // Switching to Wi-Fi while online promotes all deferred jobs to run immediately.
        let t2 = t0 + Duration::from_secs(15);
        sched.promote_wifi_deferred(true, true, t2);
        assert!(!sched.sync_backup_deferred_for_wifi);
        assert!(!sched.post_deferred_for_wifi);
        assert!(!sched.market_deferred_for_wifi);
        assert!(sched.next_sync_backup_forced);
        assert!(sched.next_market_force_upload);
        assert_eq!(sched.next_sync_backup_at, Some(t2));
        assert_eq!(sched.next_post_at, Some(t2));
        assert_eq!(sched.next_market_at, Some(t2));
    }

    #[test]
    fn sync_members_wake_and_no_targets_park_clear_on_wake() {
        let t0 = Instant::now();
        let mut sched = CoordinatorSchedule::default();
        sched.reset_for_session(1, t0);
        sched.sync_backup_no_targets = true;
        sched.sync_backup_consecutive_failures = 3;
        sched.next_sync_backup_at = None;

        // Offline ignores sync_members due
        sched.apply_wake(
            SyncWakeRequest {
                sync_members: true,
                ..Default::default()
            },
            t0 + Duration::from_secs(1),
        );
        assert_eq!(
            sched.next_sync_members_at,
            Some(t0 + Duration::from_secs(1))
        );
        assert_eq!(sched.earliest_due(true), Some(t0 + Duration::from_secs(1)));
        assert_eq!(
            sched.earliest_due(false),
            Some(t0 + SEAL_INITIAL_DELAY.min(LOCAL_BACKUP_INITIAL_DELAY))
        );

        // sync_backup wake clears no_targets park & failure backoff
        let t2 = t0 + Duration::from_secs(5);
        sched.apply_wake(
            SyncWakeRequest {
                sync_backup: true,
                ..Default::default()
            },
            t2,
        );
        assert!(!sched.sync_backup_no_targets);
        assert_eq!(sched.sync_backup_consecutive_failures, 0);
        assert_eq!(sched.next_sync_backup_at, Some(t2));
    }

    #[tokio::test]
    async fn s08_f9_wake_request_strict_and_locked_session_stops_l2_probe() {
        // 1. SyncWakeRequest rejects unknown JSON fields
        let parse_err = serde_json::from_str::<SyncWakeRequest>(r#"{"post":true,"unknown":1}"#)
            .expect_err("SyncWakeRequest must reject unknown JSON fields");
        assert!(
            parse_err.to_string().contains("unknown field"),
            "unexpected error: {parse_err}"
        );

        // 2. Locking session immediately blocks refresh_lock_servers_for_coordinator even with cached policy
        let dir = TempDirGuard(crate::test_support::temp_test_dir("coord_lock"));
        let pw = "TestPass123!";
        let mut svc = crate::test_support::setup_test_app(
            &dir.0,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        svc.unlock_session(pw, 600).unwrap();
        let state = crate::test_support::create_test_state(svc);
        // Warm caches in active session
        let policy = state.get_or_load_security_policy_locked(Some(pw)).unwrap();
        assert!(policy.market_selected_syncs.is_empty());
        let lifecycle = state
            .get_or_load_internal_lifecycle_locked(Some(pw))
            .unwrap();
        assert!(!lifecycle.pending_restore_l2_reconciliation);
        let gen_id = state.current_session_generation();
        assert!(is_session_active_for_generation(&state, gen_id));

        // Live network updates reflect in sync_control_snapshot & survive mutex poison
        state.update_network_status(true, false, true);
        let snap1 = state.sync_control_snapshot();
        assert!(snap1.is_online && !snap1.is_wifi);

        let panic_payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let guard = state.sync_control.lock().unwrap();
            assert!(guard.is_online && !guard.is_wifi);
            panic!("simulate mutex poison");
        }))
        .expect_err("closure must panic to poison mutex");
        assert_eq!(
            panic_payload.downcast_ref::<&str>(),
            Some(&"simulate mutex poison")
        );
        assert!(state.sync_control.is_poisoned());
        state.update_network_status(false, false, true);
        let snap2 = state.sync_control_snapshot();
        assert!(!snap2.is_online);

        // Lock session without bumping session_generation
        state.service.lock().unwrap().lock_session();
        assert!(!is_session_active_for_generation(&state, gen_id));
        assert!(
            !l2_anchor_service::refresh_lock_servers_for_coordinator(&state).await,
            "locked session must not initiate L2 coordinator probes"
        );
    }

    #[test]
    fn s08_q3_per_origin_cooldown_isolates_throttled_origin_from_honest_origins() {
        let dir = TempDirGuard(crate::test_support::temp_test_dir("coord_origin_cd"));
        let pw = "TestPass123!";
        let svc = crate::test_support::setup_test_app(
            &dir.0,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let state = crate::test_support::create_test_state(svc);

        let now = Instant::now();
        let recorded = state.record_origin_cooldown_for_sync_error(
            "https://evil-peer.example.org/data/alice.bin",
            &crate::sync_client::SyncClientError::RateLimited {
                status: 429,
                retry_after_secs: 3600,
                context: String::new(),
                snippet: String::new(),
            },
        );
        assert_eq!(recorded, Some(3600));
        assert!(state
            .record_origin_cooldown_for_sync_error(
                "https://other.example.org/sync.php",
                &crate::sync_client::SyncClientError::HttpStatus {
                    status: 500,
                    retry_after_secs: None,
                    context: String::new(),
                    snippet: String::new(),
                },
            )
            .is_none());
        assert!(state
            .record_origin_cooldown_for_sync_error(
                "https://other.example.org/sync.php",
                &crate::sync_client::SyncClientError::LocalValidation("Invalid URL".to_string(),),
            )
            .is_none());
        assert!(state.is_origin_cooling_down("https://evil-peer.example.org/sync.php", now));
        assert!(
            !state.is_origin_cooling_down("https://own-backup.example.org/sync.php", now),
            "429 from one origin must NOT throttle unrelated honest origins"
        );
        assert!(
            !state.is_origin_cooling_down("https://evil-peer.example.org:8443/sync.php", now),
            "different port is a distinct origin"
        );

        state.clear_sync_cooldown();
        assert!(!state.is_origin_cooling_down("https://evil-peer.example.org/sync.php", now));
    }

    #[test]
    fn test_coordinator_schedule_all_wake_reset_and_earliest_due_edges() {
        let t0 = Instant::now();
        let mut sched = CoordinatorSchedule {
            active_generation: 99,
            sync_backup_no_targets: true,
            sync_backup_consecutive_failures: 7,
            last_sync_backup_success_at: Some(t0),
            next_sync_backup_forced: true,
            sync_backup_deferred_for_wifi: true,
            post_deferred_for_wifi: true,
            next_sync_members_at: Some(t0),
            next_market_force_upload: true,
            market_deferred_for_wifi: true,
            lock_refresh_done: true,
            ..Default::default()
        };

        sched.reset_for_session(2, t0);
        assert_eq!(sched.active_generation, 2);
        assert_eq!(sched.next_seal_at, Some(t0 + SEAL_INITIAL_DELAY));
        assert_eq!(
            sched.next_local_backup_at,
            Some(t0 + LOCAL_BACKUP_INITIAL_DELAY)
        );
        assert_eq!(
            sched.next_sync_backup_at,
            Some(t0 + SYNC_BACKUP_INITIAL_DELAY)
        );
        assert_eq!(sched.next_post_at, Some(t0 + POST_INITIAL_DELAY));
        assert_eq!(sched.next_market_at, Some(t0 + MARKET_INITIAL_DELAY));
        assert!(!sched.next_sync_backup_forced);
        assert!(!sched.sync_backup_deferred_for_wifi);
        assert_eq!(sched.last_sync_backup_success_at, None);
        assert_eq!(sched.sync_backup_consecutive_failures, 0);
        assert!(!sched.sync_backup_no_targets);
        assert!(!sched.post_deferred_for_wifi);
        assert_eq!(sched.next_sync_members_at, None);
        assert!(!sched.next_market_force_upload);
        assert!(!sched.market_deferred_for_wifi);
        assert!(!sched.lock_refresh_done);

        // Offline even with Wi-Fi must NOT promote deferred jobs.
        sched.sync_backup_deferred_for_wifi = true;
        sched.post_deferred_for_wifi = true;
        sched.market_deferred_for_wifi = true;
        sched.promote_wifi_deferred(false, true, t0 + Duration::from_secs(5));
        assert!(sched.sync_backup_deferred_for_wifi);
        assert!(sched.post_deferred_for_wifi);
        assert!(sched.market_deferred_for_wifi);

        // Individual wake flags & min-preservation invariants.
        let t1 = t0 + Duration::from_secs(20);
        sched.clear();
        sched.apply_wake(
            SyncWakeRequest {
                seal: true,
                post: true,
                market: true,
                ..Default::default()
            },
            t1,
        );
        assert_eq!(sched.next_seal_at, Some(t1));
        assert_eq!(sched.next_post_at, Some(t1));
        assert_eq!(sched.next_market_at, Some(t1));
        assert!(!sched.next_market_force_upload);

        sched.next_market_at = None;
        sched.apply_wake(
            SyncWakeRequest {
                market_force_upload: true,
                local_backup: true,
                sync_backup_force: true,
                ..Default::default()
            },
            t1,
        );
        assert!(sched.next_market_force_upload);
        assert_eq!(sched.next_market_at, Some(t1 + MARKET_UPLOAD_DEBOUNCE));
        assert_eq!(
            sched.next_local_backup_at,
            Some(t1 + LOCAL_BACKUP_DEBOUNCE)
        );
        assert!(sched.next_sync_backup_forced);
        assert_eq!(
            sched.next_sync_backup_at,
            Some(t1 + SYNC_BACKUP_DEBOUNCE)
        );

        // Existing earlier deadline is preserved on forced/debounced wakes.
        let earlier = t1 + Duration::from_millis(500);
        sched.next_local_backup_at = Some(earlier);
        sched.next_sync_backup_at = Some(earlier);
        sched.next_market_at = Some(earlier);
        sched.apply_wake(
            SyncWakeRequest {
                local_backup: true,
                sync_backup_force: true,
                market_force_upload: true,
                ..Default::default()
            },
            t1,
        );
        assert_eq!(sched.next_local_backup_at, Some(earlier));
        assert_eq!(sched.next_sync_backup_at, Some(earlier));
        assert_eq!(sched.next_market_at, Some(earlier));

        // earliest_due checks offline seal fallback and each online network deadline.
        sched.clear();
        sched.next_seal_at = Some(t1 + Duration::from_secs(40));
        sched.next_sync_backup_at = Some(t1 + Duration::from_secs(3));
        assert_eq!(sched.earliest_due(false), Some(t1 + Duration::from_secs(40)));
        assert_eq!(sched.earliest_due(true), Some(t1 + Duration::from_secs(3)));
        sched.next_market_at = Some(t1 + Duration::from_secs(2));
        assert_eq!(sched.earliest_due(true), Some(t1 + Duration::from_secs(2)));
        sched.next_sync_members_at = Some(t1 + Duration::from_secs(1));
        assert_eq!(sched.earliest_due(true), Some(t1 + Duration::from_secs(1)));
    }
}
