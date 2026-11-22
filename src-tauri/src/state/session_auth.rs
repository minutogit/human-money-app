// src-tauri/src/state/session_auth.rs
use super::{
    app_state_err, load_encrypted_json_opt, load_encrypted_json_or_default, load_outgoing_bundles,
    AppState, EncryptedBatchCommit, StoredPostState, StoredSyncState, SyncCoordinatorControl,
    SyncWakeRequest, APP_NOTICES_KEY, BUNDLE_RETENTION_DAYS, MARKET_CARD_STORE_KEY,
    MARKET_OWN_PROFILE_KEY, MARKET_POST_STATE_KEY, MARKET_SYNC_KEY, ONE_SHOT_PIN_GRANT_PREFIX,
    RECOVERY_GUARDIAN_VAULT_KEY, RECOVERY_OWNER_STATE_KEY,
};
use crate::session::{ActiveProfileCaches, AuthorizedSession, SessionToken};
use chrono::{Duration, Utc};
use human_money_core::app_service::{AppFacadeError, AppService};
use log::info;
use std::sync::MutexGuard;

const ONE_SHOT_PIN_GRANT_TTL: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredAccess {
    Unlocked,
    DailyPay,
    Full,
}

/// OS-level exclusive session lock for a single profile folder (`<session_locks>/<folder_name>.lock`).
///
/// Released automatically on `Drop` (logout, failed login rollback, or process termination).
pub struct ProfileSessionLock {
    folder_name: String,
    _file: std::fs::File,
}

/// Result of attempting to acquire a profile session lock prior to login/recovery.
pub enum PendingProfileLock {
    None,
    AlreadyHeld,
    Acquired(ProfileSessionLock),
}

fn validate_lock_folder_name(folder_name: &str) -> Result<(), crate::models::FrontendError> {
    if folder_name.is_empty() || folder_name.contains(['/', '\\']) {
        return Err(crate::models::FrontendError::from(
            "Invalid profile folder name".to_string(),
        ));
    }
    let mut components = std::path::Path::new(folder_name).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(_)), None) => Ok(()),
        _ => Err(crate::models::FrontendError::from(
            "Invalid profile folder name".to_string(),
        )),
    }
}

impl ProfileSessionLock {
    fn try_acquire(
        lock_dir: &std::path::Path,
        folder_name: &str,
    ) -> Result<Self, crate::models::FrontendError> {
        validate_lock_folder_name(folder_name)?;
        std::fs::create_dir_all(lock_dir).map_err(|e| {
            crate::models::FrontendError::from(format!(
                "Failed to create session lock directory: {}",
                e
            ))
        })?;
        let lock_path = lock_dir.join(format!("{folder_name}.lock"));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| {
                crate::models::FrontendError::from(format!(
                    "Failed to open profile session lock file: {}",
                    e
                ))
            })?;
        match file.try_lock() {
            Ok(()) => Ok(Self {
                folder_name: folder_name.to_string(),
                _file: file,
            }),
            Err(std::fs::TryLockError::WouldBlock) => Err(crate::models::FrontendError::with_code(
                "error.auth.profileAlreadyOpen",
                "This profile is already open in another app instance.",
            )),
            Err(std::fs::TryLockError::Error(e)) => Err(crate::models::FrontendError::from(
                format!("Failed to acquire profile session lock: {}", e),
            )),
        }
    }
}

/// Ephemeral single-use server-side grant issued when unlocking with Quick-PIN under `timeout == 0`.
pub(super) struct OneShotPinGrant {
    token: String,
    secret: zeroize::Zeroizing<String>,
    generation: u64,
    owner_did: String,
    expires_at: std::time::Instant,
}

impl AppState {
    pub fn update_network_status(&self, is_online: bool, is_wifi: bool, is_visible: bool) {
        let should_notify = {
            let mut guard = self.sync_control.lock().unwrap_or_else(|e| e.into_inner());
            let became_online = !guard.is_online && is_online;
            let became_wifi = !guard.is_wifi && is_wifi;
            let became_visible = !guard.is_visible && is_visible;
            guard.is_online = is_online;
            guard.is_wifi = is_wifi;
            guard.is_visible = is_visible;
            became_online || became_wifi || became_visible
        };
        if should_notify {
            self.sync_coordinator_wake.notify_one();
        }
    }

    pub fn wake_sync_coordinator(&self, req: SyncWakeRequest) {
        {
            let mut guard = self.sync_control.lock().unwrap_or_else(|e| e.into_inner());
            guard.pending_wake.merge(req);
        }
        self.sync_coordinator_wake.notify_one();
    }

    pub fn notify_sync_coordinator(&self) {
        self.sync_coordinator_wake.notify_one();
    }

    pub fn sync_control_snapshot(&self) -> SyncCoordinatorControl {
        self.sync_control
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn take_sync_coordinator_snapshot(&self) -> (SyncCoordinatorControl, SyncWakeRequest) {
        let mut guard = self.sync_control.lock().unwrap_or_else(|e| e.into_inner());
        let wake = std::mem::take(&mut guard.pending_wake);
        (guard.clone(), wake)
    }

    pub fn record_origin_cooldown(&self, url: &str, retry_after_secs: u64) {
        let Some(origin) = crate::sync_client::sync_origin_key(url) else {
            return;
        };
        let clamped = retry_after_secs.clamp(5, 3600);
        let now = std::time::Instant::now();
        let target = now + std::time::Duration::from_secs(clamped);
        let mut guard = self.sync_control.lock().unwrap_or_else(|e| e.into_inner());
        guard.origin_cooldowns.retain(|_, until| *until > now);
        const MAX_ORIGIN_COOLDOWNS: usize = 256;
        if !guard.origin_cooldowns.contains_key(&origin)
            && guard.origin_cooldowns.len() >= MAX_ORIGIN_COOLDOWNS
        {
            if let Some(oldest_key) = guard
                .origin_cooldowns
                .iter()
                .min_by_key(|(_, until)| **until)
                .map(|(k, _)| k.clone())
            {
                guard.origin_cooldowns.remove(&oldest_key);
            }
        }
        let entry = guard.origin_cooldowns.entry(origin).or_insert(target);
        if target > *entry {
            *entry = target;
        }
    }

    pub fn record_origin_cooldown_for_sync_error(
        &self,
        url: &str,
        err: &crate::sync_client::SyncClientError,
    ) -> Option<u64> {
        let secs = err.retry_after_secs()?;
        self.record_origin_cooldown(url, secs);
        Some(secs)
    }

    pub fn is_origin_cooling_down(&self, url: &str, now: std::time::Instant) -> bool {
        let Some(origin) = crate::sync_client::sync_origin_key(url) else {
            return false;
        };
        let guard = self.sync_control.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .origin_cooldowns
            .get(&origin)
            .is_some_and(|until| now < *until)
    }

    pub fn clear_sync_cooldown(&self) {
        let mut guard = self.sync_control.lock().unwrap_or_else(|e| e.into_inner());
        guard.origin_cooldowns.clear();
        guard.pending_wake = SyncWakeRequest::default();
    }

    pub fn current_session_generation(&self) -> u64 {
        self.session_generation
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn is_generation_current(&self, generation: u64) -> bool {
        self.current_session_generation() == generation
    }

    pub fn subscribe_session_generation(&self) -> tokio::sync::watch::Receiver<u64> {
        self.session_gen_tx.subscribe()
    }

    pub fn session_token_for_service(
        &self,
        service: &AppService,
    ) -> Result<SessionToken, crate::models::FrontendError> {
        if !service.is_wallet_unlocked() {
            return Err(crate::models::FrontendError::from(
                AppFacadeError::WalletLocked,
            ));
        }
        let owner_did = service
            .with_wallet(|w| w.get_user_id().to_string())
            .map_err(crate::models::FrontendError::from)?;
        Ok(SessionToken {
            generation: self.current_session_generation(),
            owner_did,
        })
    }

    pub fn begin_session_op(&self) -> Result<SessionToken, crate::models::FrontendError> {
        let service = self.lock_service()?;
        self.session_token_for_service(&service)
    }

    pub fn authorize_and_begin_session(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<SessionToken, crate::models::FrontendError> {
        let service = self.authorize_and_lock(required, password)?;
        self.session_token_for_service(&service)
    }

    /// Synchronizes host session caches, one-shot grants, and generation when Core
    /// intentionally locks itself (e.g. post-mutation failure in `TransactionOrchestrator`).
    pub fn sync_if_core_locked(&self, service: &AppService) {
        if !service.is_wallet_unlocked() {
            let has_host_state = self.profile_caches.lock().unwrap().is_some()
                || self.one_shot_pin_grant.lock().unwrap().is_some();
            if has_host_state {
                self.clear_all_caches();
            }
        }
    }

    pub fn is_session_current(&self, token: &SessionToken, service: &AppService) -> bool {
        if self.current_session_generation() != token.generation || !service.is_wallet_unlocked() {
            return false;
        }
        service
            .with_wallet(|w| w.get_user_id() == token.owner_did)
            .unwrap_or(false)
    }

    pub fn with_session<F, R>(
        &self,
        token: &SessionToken,
        f: F,
    ) -> Result<Option<R>, crate::models::FrontendError>
    where
        F: FnOnce(&mut AppService) -> Result<R, crate::models::FrontendError>,
    {
        let mut service = self.lock_service()?;
        self.sync_if_core_locked(&service);
        if !self.is_session_current(token, &service) {
            return Ok(None);
        }
        let res = f(&mut service);
        self.sync_if_core_locked(&service);
        res.map(Some)
    }

    pub fn with_session_required<F, R>(
        &self,
        token: &SessionToken,
        superseded_msg: &str,
        f: F,
    ) -> Result<R, crate::models::FrontendError>
    where
        F: FnOnce(&mut AppService) -> Result<R, crate::models::FrontendError>,
    {
        self.with_session(token, f)?
            .ok_or_else(|| crate::models::FrontendError::from(superseded_msg.to_string()))
    }

    pub fn with_session_lock_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.session_lock_dir = Some(dir);
        self
    }

    pub fn acquire_profile_lock(
        &self,
        folder_name: &str,
    ) -> Result<PendingProfileLock, crate::models::FrontendError> {
        let Some(ref lock_dir) = self.session_lock_dir else {
            return Ok(PendingProfileLock::None);
        };
        {
            let guard = self.active_profile_lock.lock().unwrap();
            if let Some(ref existing) = *guard {
                if existing.folder_name == folder_name {
                    return Ok(PendingProfileLock::AlreadyHeld);
                }
            }
        }
        let lock = ProfileSessionLock::try_acquire(lock_dir, folder_name)?;
        Ok(PendingProfileLock::Acquired(lock))
    }

    pub fn commit_profile_lock(&self, pending: PendingProfileLock) {
        if let PendingProfileLock::Acquired(lock) = pending {
            *self.active_profile_lock.lock().unwrap() = Some(lock);
        }
    }

    pub fn release_profile_lock(&self) {
        *self.active_profile_lock.lock().unwrap() = None;
    }

    /// Releases the held `ProfileSessionLock` guard iff it belongs to `folder_name`.
    /// Never unlinks the underlying `.lock` file path so live `flock` inodes stay intact.
    pub fn release_profile_lock_if_matching(&self, folder_name: &str) {
        let mut guard = self.active_profile_lock.lock().unwrap();
        if guard
            .as_ref()
            .is_some_and(|existing| existing.folder_name == folder_name)
        {
            *guard = None;
        }
    }

    // 1. Central helper methods to access the locked service
    pub fn service(&self) -> MutexGuard<'_, AppService> {
        self.service.lock().unwrap()
    }

    pub fn lock_service(&self) -> Result<MutexGuard<'_, AppService>, crate::models::FrontendError> {
        self.service.lock().map_err(|_| {
            crate::models::FrontendError::from("Wallet service unavailable".to_string())
        })
    }

    pub fn clear_one_shot_pin_grant(&self) {
        *self.one_shot_pin_grant.lock().unwrap() = None;
    }

    pub fn issue_one_shot_pin_grant(
        &self,
        service: &AppService,
        secret: zeroize::Zeroizing<String>,
    ) -> Result<String, crate::models::FrontendError> {
        if !service.is_wallet_unlocked() {
            return Err(crate::models::FrontendError::from(
                AppFacadeError::WalletLocked,
            ));
        }
        let owner_did = service
            .with_wallet(|w| w.get_user_id().to_string())
            .map_err(crate::models::FrontendError::from)?;
        let generation = self.current_session_generation();
        let mut rnd = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut rnd);
        let token = format!("{}{}", ONE_SHOT_PIN_GRANT_PREFIX, hex::encode(rnd));
        let grant = OneShotPinGrant {
            token: token.clone(),
            secret,
            generation,
            owner_did,
            expires_at: std::time::Instant::now() + ONE_SHOT_PIN_GRANT_TTL,
        };
        *self.one_shot_pin_grant.lock().unwrap() = Some(grant);
        Ok(token)
    }

    fn consume_one_shot_pin_grant(
        &self,
        service: &AppService,
        token: &str,
    ) -> Result<zeroize::Zeroizing<String>, crate::models::FrontendError> {
        let grant = {
            let mut guard = self.one_shot_pin_grant.lock().unwrap();
            let Some(existing) = guard.as_ref() else {
                return Err(crate::models::FrontendError::from(
                    human_money_core::Error::App(
                        human_money_core::error::AppError::SessionExpired {
                            reason: "One-shot PIN grant missing or already used.".to_string(),
                        },
                    ),
                ));
            };
            if existing.token != token {
                return Err(crate::models::FrontendError::from(
                    human_money_core::Error::App(
                        human_money_core::error::AppError::SessionExpired {
                            reason: "One-shot PIN grant token mismatch or superseded.".to_string(),
                        },
                    ),
                ));
            }
            guard.take().unwrap()
        };
        let owner_matches = service
            .with_wallet(|w| w.get_user_id() == grant.owner_did)
            .unwrap_or(false);
        if std::time::Instant::now() > grant.expires_at
            || self.current_session_generation() != grant.generation
            || !service.is_wallet_unlocked()
            || !owner_matches
        {
            return Err(crate::models::FrontendError::from(
                human_money_core::Error::App(human_money_core::error::AppError::SessionExpired {
                    reason: "One-shot PIN grant expired or bound to a different session."
                        .to_string(),
                }),
            ));
        }
        Ok(grant.secret)
    }

    /// Enforces `required` access and resolves any single-use `hmc_grant_*` PIN grant into its
    /// underlying zeroizing secret for `DailyPay` or `Unlocked` operations.
    pub fn authorize_and_lock_secret(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<
        (
            MutexGuard<'_, AppService>,
            Option<zeroize::Zeroizing<String>>,
        ),
        crate::models::FrontendError,
    > {
        let service = self.lock_service()?;
        self.sync_if_core_locked(&service);
        if let Some(raw) = password {
            if raw.starts_with(ONE_SHOT_PIN_GRANT_PREFIX) {
                match required {
                    RequiredAccess::Full => {
                        return Err(crate::models::FrontendError::from(
                            human_money_core::Error::App(
                                human_money_core::error::AppError::InsufficientAccessMode {
                                    required: human_money_core::AccessMode::Full
                                        .as_str()
                                        .to_string(),
                                    current: human_money_core::AccessMode::DailyPay
                                        .as_str()
                                        .to_string(),
                                },
                            ),
                        ));
                    }
                    RequiredAccess::DailyPay => {
                        let secret = self.consume_one_shot_pin_grant(&service, raw)?;
                        service
                            .require_access_mode(
                                human_money_core::AccessMode::DailyPay,
                                Some(secret.as_str()),
                            )
                            .map_err(crate::models::FrontendError::from)?;
                        return Ok((service, Some(secret)));
                    }
                    RequiredAccess::Unlocked => {
                        let secret = self.consume_one_shot_pin_grant(&service, raw)?;
                        if !service.is_wallet_unlocked() {
                            return Err(crate::models::FrontendError::from(
                                AppFacadeError::WalletLocked,
                            ));
                        }
                        return Ok((service, Some(secret)));
                    }
                }
            }
        }

        match required {
            RequiredAccess::Unlocked => {
                if !service.is_wallet_unlocked() {
                    return Err(crate::models::FrontendError::from(
                        AppFacadeError::WalletLocked,
                    ));
                }
            }
            RequiredAccess::DailyPay => {
                service
                    .require_access_mode(human_money_core::AccessMode::DailyPay, password)
                    .map_err(crate::models::FrontendError::from)?;
            }
            RequiredAccess::Full => {
                service
                    .require_access_mode(human_money_core::AccessMode::Full, password)
                    .map_err(crate::models::FrontendError::from)?;
            }
        }
        Ok((
            service,
            password.map(|p| zeroize::Zeroizing::new(p.to_string())),
        ))
    }

    pub fn authorize_and_begin_session_secret(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<(SessionToken, Option<zeroize::Zeroizing<String>>), crate::models::FrontendError>
    {
        let (service, resolved_secret) = self.authorize_and_lock_secret(required, password)?;
        let session = self.session_token_for_service(&service)?;
        Ok((session, resolved_secret))
    }

    pub fn authorize_session(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<AuthorizedSession, crate::models::FrontendError> {
        let (token, resolved_secret) =
            self.authorize_and_begin_session_secret(required, password)?;
        Ok(AuthorizedSession::new(token, resolved_secret))
    }

    /// Enforces `required` access at the Tauri boundary and returns the locked `AppService`.
    pub fn authorize_and_lock(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<MutexGuard<'_, AppService>, crate::models::FrontendError> {
        let service = self.lock_service()?;
        self.sync_if_core_locked(&service);
        if password.is_some_and(|p| p.starts_with(ONE_SHOT_PIN_GRANT_PREFIX)) {
            return match required {
                RequiredAccess::Full => Err(crate::models::FrontendError::from(
                    human_money_core::Error::App(
                        human_money_core::error::AppError::InsufficientAccessMode {
                            required: human_money_core::AccessMode::Full.as_str().to_string(),
                            current: human_money_core::AccessMode::DailyPay.as_str().to_string(),
                        },
                    ),
                )),
                RequiredAccess::DailyPay | RequiredAccess::Unlocked => Err(
                    crate::models::FrontendError::from(human_money_core::Error::App(
                        human_money_core::error::AppError::SessionExpired {
                            reason: "One-shot PIN grant must be consumed via secret guard."
                                .to_string(),
                        },
                    )),
                ),
            };
        }
        match required {
            RequiredAccess::Unlocked => {
                if !service.is_wallet_unlocked() {
                    return Err(crate::models::FrontendError::from(
                        AppFacadeError::WalletLocked,
                    ));
                }
            }
            RequiredAccess::DailyPay => {
                service
                    .require_access_mode(human_money_core::AccessMode::DailyPay, password)
                    .map_err(crate::models::FrontendError::from)?;
            }
            RequiredAccess::Full => {
                service
                    .require_access_mode(human_money_core::AccessMode::Full, password)
                    .map_err(crate::models::FrontendError::from)?;
            }
        }
        Ok(service)
    }

    /// Enforces `required` access without holding `AppService` mutex across `.await` points.
    pub fn authorize(
        &self,
        required: RequiredAccess,
        password: Option<&str>,
    ) -> Result<(), crate::models::FrontendError> {
        let _service = self.authorize_and_lock(required, password)?;
        Ok(())
    }

    // 6. Logout Helpers
    pub fn clear_all_caches(&self) {
        info!("Clearing all caches on logout...");
        let mut guard = self.profile_caches.lock().unwrap();
        let next = self
            .session_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .saturating_add(1);
        let _ = self.session_gen_tx.send(next);
        *guard = None;
        self.clear_one_shot_pin_grant();
    }

    /// Central session initializer — called after Login, Create Profile, Recovery, and Handover.
    /// Loads all caches (Settings, History, Events, Contacts, WoT) and starts the session timer.
    pub fn initialize_profile_session(
        &self,
        service: &mut AppService,
        password: &str,
    ) -> Result<(), AppFacadeError> {
        self.initialize_profile_session_inner(service, password, false)
    }

    /// Central session initializer for secondary (PIN / Cashier) logins.
    pub fn initialize_profile_session_with_secondary(
        &self,
        service: &mut AppService,
        secondary_secret: &str,
    ) -> Result<(), AppFacadeError> {
        self.initialize_profile_session_inner(service, secondary_secret, true)
    }

    pub fn rollback_failed_session(&self, service: &mut AppService) {
        service.logout();
        self.clear_all_caches();
        self.clear_sync_cooldown();
        self.release_profile_lock();
    }

    fn initialize_profile_session_inner(
        &self,
        service: &mut AppService,
        credential: &str,
        is_secondary: bool,
    ) -> Result<(), AppFacadeError> {
        let res = self.initialize_profile_session_fallible(service, credential, is_secondary);
        if let Err(err) = res {
            self.rollback_failed_session(service);
            return Err(err);
        }
        Ok(())
    }

    fn initialize_profile_session_fallible(
        &self,
        service: &mut AppService,
        credential: &str,
        is_secondary: bool,
    ) -> Result<(), AppFacadeError> {
        info!("Initializing profile session and member caches...");
        let owner_did = service.with_wallet(|w| w.get_user_id().to_string())?;
        {
            let mut guard = self.profile_caches.lock().unwrap();
            let next = self
                .session_generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                .saturating_add(1);
            let _ = self.session_gen_tx.send(next);
            *guard = Some(ActiveProfileCaches::new(next, owner_did));
        }
        self.clear_one_shot_pin_grant();

        // 1. Load security policy fail-closed; leave non-vital stores uncached (`None`) on error
        // so corrupt disk data is never overwritten and emergency seed recovery stays unblocked.
        let security_policy = self.load_security_policy(service, Some(credential))?;
        if let Err(e) = self.load_ui_preferences(service, Some(credential)) {
            log::warn!(
                "Failed to pre-load UiPreferences during session init: {}",
                e
            );
        }
        if let Err(e) = self.load_internal_lifecycle(service, Some(credential)) {
            log::error!(
                "Failed to load InternalLifecycleState during session init (leaving cache unpopulated): {}",
                e
            );
        }

        // 2. Start session timer if configured
        if security_policy.session_timeout_seconds > 0 {
            if is_secondary {
                service.unlock_secondary_session(
                    credential,
                    security_policy.session_timeout_seconds,
                )?;
            } else {
                service.unlock_session(credential, security_policy.session_timeout_seconds)?;
            }
        }

        // 3. Load transaction history + canonical outgoing bundles and prune expired retained bundles (non-fatal)
        match (
            self.load_history_from_disk(service, Some(credential)),
            load_outgoing_bundles(service, Some(credential)).map_err(|e| app_state_err(e.message)),
        ) {
            (Ok(mut history), Ok(mut outgoing)) => {
                let retention_duration = Duration::days(BUNDLE_RETENTION_DAYS);
                let now = Utc::now();

                let mut history_changed = false;
                for record in &mut history {
                    if !record.bundle_data.is_empty() {
                        record.bundle_data.clear();
                        history_changed = true;
                    }
                }
                let prev_outgoing_len = outgoing.bundles.len();
                outgoing.bundles.retain(|_, entry| {
                    entry
                        .created_at
                        .parse::<chrono::DateTime<Utc>>()
                        .map(|ts| now.signed_duration_since(ts) <= retention_duration)
                        .unwrap_or(true)
                });
                let outgoing_changed = outgoing.bundles.len() != prev_outgoing_len;

                if history_changed || outgoing_changed {
                    info!("Saving pruned outgoing bundles / transaction history back to disk...");
                    let mut batch = EncryptedBatchCommit::new();
                    batch.outgoing_bundles(outgoing).history(history);
                    batch.commit(self, service, Some(credential))?;
                } else {
                    self.write_cache_slot_for_service(
                        service,
                        |c| &mut c.outgoing_bundles,
                        Some(outgoing),
                    );
                }
            }
            (hist_res, out_res) => {
                if let Err(e) = hist_res {
                    log::warn!(
                        "Failed to pre-load transaction history during session init: {}",
                        e
                    );
                }
                match out_res {
                    Ok(outgoing) => {
                        self.write_cache_slot_for_service(
                            service,
                            |c| &mut c.outgoing_bundles,
                            Some(outgoing),
                        );
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to pre-load outgoing bundles during session init: {}",
                            e
                        );
                    }
                }
            }
        }

        // 4. Pre-cache the last 50 wallet events
        if let Err(e) = self.refresh_events_cache(service, Some(credential)) {
            log::warn!(
                "Failed to pre-cache wallet events during session init: {}",
                e
            );
        }

        // 5. Load the address book
        if let Err(e) = self.load_contacts(service, Some(credential)) {
            log::warn!("Failed to pre-load contacts during session init: {}", e);
        }

        // 6. Load the Web of Trust register
        if let Err(e) = self.load_wot_register(service, Some(credential)) {
            log::warn!("Failed to pre-load WoT register during session init: {}", e);
        }

        // 7. Pre-cache Social Recovery owner state and guardian vault (supports timeout = 0)
        match load_encrypted_json_opt::<crate::models::recovery::RecoveryOwnerState>(
            service,
            RECOVERY_OWNER_STATE_KEY,
            Some(credential),
        ) {
            Ok(owner_opt) => {
                self.write_cache_slot_for_service(
                    service,
                    |c| &mut c.recovery_owner,
                    Some(owner_opt),
                );
            }
            Err(e) => {
                log::warn!("Failed to pre-load recovery owner state: {}", e);
            }
        }
        match load_encrypted_json_or_default::<humoco_market::GuardianVaultStore>(
            service,
            RECOVERY_GUARDIAN_VAULT_KEY,
            Some(credential),
        ) {
            Ok(vault) => {
                self.write_cache_slot_for_service(service, |c| &mut c.guardian_vault, Some(vault));
            }
            Err(e) => {
                log::warn!("Failed to pre-load guardian vault: {}", e);
            }
        }

        // 8. Pre-cache own market profile (supports timeout = 0)
        match load_encrypted_json_or_default::<crate::models::StoredOwnMarketProfile>(
            service,
            MARKET_OWN_PROFILE_KEY,
            Some(credential),
        ) {
            Ok(own_profile) => {
                self.write_cache_slot_for_service(
                    service,
                    |c| &mut c.own_market_profile,
                    Some(own_profile),
                );
            }
            Err(e) => {
                log::warn!("Failed to pre-load own market profile: {}", e);
            }
        }

        // 9. Pre-cache app notices (supports timeout = 0)
        match load_encrypted_json_or_default::<crate::models::StoredAppNotices>(
            service,
            APP_NOTICES_KEY,
            Some(credential),
        ) {
            Ok(notices) => {
                self.write_cache_slot_for_service(service, |c| &mut c.app_notices, Some(notices));
            }
            Err(e) => {
                log::warn!("Failed to pre-load app notices: {}", e);
            }
        }

        // 10. Pre-cache server offender list (supports timeout = 0)
        match crate::l2_lifecycle::load_server_offender_list(service, Some(credential)) {
            Ok(offenders) => {
                self.write_cache_slot_for_service(
                    service,
                    |c| &mut c.server_offenders,
                    Some(offenders),
                );
            }
            Err(e) => {
                log::warn!("Failed to pre-load server offender list: {}", e.message);
            }
        }

        // 11. Pre-cache non-sensitive own sync descriptor (supports timeout = 0)
        match crate::services::sync_engine::admin::load_stored_sync_admin_opt(
            service,
            Some(credential),
        ) {
            Ok(admin_opt) => {
                self.write_cache_slot_for_service(
                    service,
                    |c| &mut c.own_sync_descriptor,
                    Some(super::active_own_sync_descriptor(admin_opt.as_ref())),
                );
            }
            Err(e) => {
                log::warn!("Failed to pre-load own sync descriptor: {}", e.message);
            }
        }

        // 12. Pre-cache PoST outbox/inbox state (supports timeout = 0)
        match load_encrypted_json_or_default::<StoredPostState>(
            service,
            MARKET_POST_STATE_KEY,
            Some(credential),
        ) {
            Ok(post_state) => {
                self.write_cache_slot_for_service(service, |c| &mut c.post_state, Some(post_state));
            }
            Err(e) => {
                log::warn!("Failed to pre-load PoST state: {}", e);
            }
        }

        // 13. Pre-cache received market CardStore (supports timeout = 0)
        match load_encrypted_json_or_default::<humoco_market::CardStore>(
            service,
            MARKET_CARD_STORE_KEY,
            Some(credential),
        ) {
            Ok(card_store) => {
                self.set_cached_card_store_for_service(service, Some(card_store));
            }
            Err(e) => {
                log::warn!("Failed to pre-load market CardStore: {}", e);
            }
        }

        // 14. Pre-cache sync state (supports timeout = 0)
        match load_encrypted_json_or_default::<StoredSyncState>(
            service,
            MARKET_SYNC_KEY,
            Some(credential),
        ) {
            Ok(sync_state) => {
                self.set_cached_sync_state_for_service(service, Some(sync_state));
            }
            Err(e) => {
                log::warn!("Failed to pre-load sync state: {}", e);
            }
        }

        Ok(())
    }
}
