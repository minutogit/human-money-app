//! Central background sync coordinator (H10).
//!
//! Replaces the four parallel frontend `setInterval`/`setTimeout` sync loops (`sealSync`,
//! `autoBackup`, `postSync`, `marketSync`) with a single backend Tokio worker owned by `AppState`
//! and spawned once during Tauri setup. All passes bind to `AppState::session_generation` and
//! `owner_did` and discard results if the user logs out or switches profiles while HTTP requests
//! are in flight.

use std::time::{Duration, Instant};

use tauri::{Emitter, Manager};

use crate::services::{
    backup_service, l2_anchor_service, post_service, sync_engine,
};
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
    pub last_sync_backup_success_at: Option<Instant>,
    pub next_post_at: Option<Instant>,
    pub next_market_at: Option<Instant>,
    pub next_market_force_upload: bool,
}

impl CoordinatorSchedule {
    pub fn reset_for_session(&mut self, generation: u64, now: Instant) {
        self.active_generation = generation;
        self.next_seal_at = Some(now + SEAL_INITIAL_DELAY);
        self.next_local_backup_at = Some(now + LOCAL_BACKUP_INITIAL_DELAY);
        self.next_sync_backup_at = Some(now + SYNC_BACKUP_INITIAL_DELAY);
        self.next_sync_backup_forced = false;
        self.last_sync_backup_success_at = None;
        self.next_post_at = Some(now + POST_INITIAL_DELAY);
        self.next_market_at = Some(now + MARKET_INITIAL_DELAY);
        self.next_market_force_upload = false;
    }

    pub fn clear(&mut self) {
        self.active_generation = 0;
        self.next_seal_at = None;
        self.next_local_backup_at = None;
        self.next_sync_backup_at = None;
        self.next_sync_backup_forced = false;
        self.last_sync_backup_success_at = None;
        self.next_post_at = None;
        self.next_market_at = None;
        self.next_market_force_upload = false;
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

    /// Computes the earliest wakeup `Instant` across due tasks while gating network passes on
    /// connectivity and active 429 cooldown windows.
    pub fn earliest_due(
        &self,
        is_online: bool,
        cooldown_until: Option<Instant>,
    ) -> Option<Instant> {
        let mut earliest = self.next_local_backup_at;
        if let Some(seal_at) = self.next_seal_at {
            earliest = Some(earliest.map_or(seal_at, |e| e.min(seal_at)));
        }

        if is_online {
            let net_floor = |at: Instant| cooldown_until.map_or(at, |cd| at.max(cd));
            if let Some(at) = self.next_post_at.map(net_floor) {
                earliest = Some(earliest.map_or(at, |e| e.min(at)));
            }
            if let Some(at) = self.next_sync_backup_at.map(net_floor) {
                earliest = Some(earliest.map_or(at, |e| e.min(at)));
            }
            if let Some(at) = self.next_market_at {
                // Forced market uploads bypass periodic cooldown gating, matching legacy UX expectation
                let effective = if self.next_market_force_upload {
                    at
                } else {
                    net_floor(at)
                };
                earliest = Some(earliest.map_or(effective, |e| e.min(effective)));
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

fn is_cooling_down(cooldown_until: Option<Instant>, now: Instant) -> bool {
    cooldown_until.is_some_and(|until| now < until)
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

        // 1. WalletSeal check (local check; triggers backup immediately when PendingUpload is detected)
        if schedule.next_seal_at.is_some_and(|t| now >= t) {
            schedule.next_seal_at = Some(now + SEAL_CHECK_INTERVAL);
            if let Ok(status) = l2_anchor_service::get_seal_sync_status_impl(&state) {
                if status == "PendingUpload" {
                    schedule.next_local_backup_at = Some(now);
                    schedule.next_sync_backup_forced = true;
                    schedule.next_sync_backup_at = Some(now);
                }
            }
        }

        // 2. Local folder Auto-Backup (runs offline as well, executes sync I/O on blocking pool)
        if schedule.next_local_backup_at.is_some_and(|t| now >= t) && state.is_generation_current(generation)
        {
            schedule.next_local_backup_at = Some(now + LOCAL_BACKUP_PERIODIC_INTERVAL);
            let app_clone = app.clone();
            let res = tokio::task::spawn_blocking(move || {
                let s = app_clone.state::<AppState>();
                backup_service::run_auto_backup_sync_impl(&s, &app_clone)
            })
            .await;
            if state.is_generation_current(generation) {
                if let Ok(Ok(dto)) = res {
                    let _ = app.emit("humoco://backup-updated", &dto);
                }
            } else {
                continue;
            }
        }

        let net_cooldown = is_cooling_down(ctrl.cooldown_until, Instant::now());

        // 3. Off-site Sync Backup (requires online + not cooling down)
        if ctrl.is_online
            && !net_cooldown
            && schedule.next_sync_backup_at.is_some_and(|t| Instant::now() >= t)
            && state.is_generation_current(generation)
        {
            let forced = std::mem::take(&mut schedule.next_sync_backup_forced);
            let now_run = Instant::now();
            schedule.next_sync_backup_at = Some(now_run + SYNC_BACKUP_DAILY_INTERVAL);
            let throttled = !forced
                && schedule
                    .last_sync_backup_success_at
                    .is_some_and(|last| now_run.saturating_duration_since(last) < SYNC_BACKUP_DAILY_INTERVAL);
            if !throttled {
                match backup_service::sync_backup_impl(None, Some(ctrl.is_wifi), &state, &app).await
                {
                    Ok(res) if state.is_generation_current(generation) => {
                        if let Some(secs) = res.retry_after_secs {
                            state.record_sync_cooldown(secs);
                        }
                        if res.uploaded_to > 0 {
                            schedule.last_sync_backup_success_at = Some(Instant::now());
                        }
                        let _ = app.emit("humoco://backup-updated", serde_json::Value::Null);
                        let _ = app.emit("humoco://sync-status-updated", &res);
                    }
                    Err(e) if state.is_generation_current(generation) => {
                        if let Some(secs) =
                            crate::sync_client::extract_retry_after_from_error_str(&e.message)
                        {
                            state.record_sync_cooldown(secs);
                        }
                    }
                    _ => {}
                }
            }
        }

        if !state.is_generation_current(generation) {
            continue;
        }

        // 4. PoST Channel Sync
        let now_post = Instant::now();
        let cooldown_post = state.sync_control.lock().unwrap().cooldown_until;
        if ctrl.is_online
            && !is_cooling_down(cooldown_post, now_post)
            && schedule.next_post_at.is_some_and(|t| now_post >= t)
        {
            schedule.next_post_at = Some(now_post + POST_PERIODIC_INTERVAL);
            match post_service::sync_post_channels_impl(
                Some(&app),
                Some(ctrl.is_wifi),
                None,
                None,
                &state,
            )
            .await
            {
                Ok(res) if state.is_generation_current(generation) => {
                    if let Some(secs) = res.retry_after_secs {
                        state.record_sync_cooldown(secs);
                    }
                    let has_backup_changes = res.uploaded_to > 0
                        || res.imported_payments > 0
                        || res.imported_receipts > 0
                        || res.imported_signatures > 0
                        || res.new_sign_requests > 0
                        || res.new_wot_handshakes > 0
                        || res.new_inquiries > 0
                        || res.new_inquiry_messages > 0;
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
                Err(e) if state.is_generation_current(generation) => {
                    if let Some(secs) =
                        crate::sync_client::extract_retry_after_from_error_str(&e.message)
                    {
                        state.record_sync_cooldown(secs);
                    }
                }
                _ => {}
            }
        }

        if !state.is_generation_current(generation) {
            continue;
        }

        // 5. Market (`card` + `wot`) Channel Sync
        let now_market = Instant::now();
        let cooldown_now = state.sync_control.lock().unwrap().cooldown_until;
        if ctrl.is_online && schedule.next_market_at.is_some_and(|t| now_market >= t) {
            let force_upload = schedule.next_market_force_upload;
            if force_upload || !is_cooling_down(cooldown_now, now_market) {
                schedule.next_market_force_upload = false;
                schedule.next_market_at = Some(now_market + MARKET_PERIODIC_INTERVAL);
                match sync_engine::sync_market_channels_with_options(
                    Some(ctrl.is_wifi),
                    Some(force_upload),
                    Some(!force_upload),
                    None,
                    &state,
                )
                .await
                {
                    Ok(res) if state.is_generation_current(generation) => {
                        if let Some(secs) = res.retry_after_secs {
                            state.record_sync_cooldown(secs);
                        }
                        let _ = app.emit("humoco://market-synced", &res);
                    }
                    Err(e) if state.is_generation_current(generation) => {
                        if let Some(secs) =
                            crate::sync_client::extract_retry_after_from_error_str(&e.message)
                        {
                            state.record_sync_cooldown(secs);
                        }
                    }
                    _ => {}
                }
            }
        }

        let next_due = schedule.earliest_due(ctrl.is_online, state.sync_control.lock().unwrap().cooldown_until);
        wait_for_next_event(next_due, &mut gen_rx, &wake).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let due_offline = sched.earliest_due(false, None).unwrap();
        assert_eq!(due_offline, t0 + SEAL_INITIAL_DELAY);

        // Cooldown pushes non-forced remote jobs out while allowing local backup earlier
        let cooldown = t0 + Duration::from_secs(120);
        sched.next_seal_at = Some(t0 + Duration::from_secs(120));
        sched.next_local_backup_at = Some(t0 + Duration::from_secs(5));
        assert_eq!(
            sched.earliest_due(true, Some(cooldown)),
            Some(t0 + Duration::from_secs(5))
        );

        // Forced market upload wake uses MARKET_UPLOAD_DEBOUNCE and bypasses cooldown floor
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
        assert_eq!(sched.earliest_due(true, None), None);
    }
}
