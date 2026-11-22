// src-tauri/src/state/profile_cache.rs
use super::contacts_repo::enrich_contact_with_wot;
use super::{
    app_state_err, AppState, StoredPostState, StoredSyncAdmin, StoredSyncState, APP_NOTICES_KEY,
    CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY, MARKET_OWN_PROFILE_KEY, MARKET_POST_STATE_KEY,
    MARKET_SYNC_ADMIN_KEY, MARKET_SYNC_KEY, OUTGOING_BUNDLES_KEY, RECOVERY_GUARDIAN_VAULT_KEY,
    RECOVERY_OWNER_STATE_KEY, TRANSACTION_HISTORY_KEY, WOT_DATA_NAME,
};
use crate::models::{FrontendAddressBook, FrontendContact, TransactionRecord};
use crate::session::{ActiveProfileCaches, SessionToken};
use crate::settings::{
    AppConfigDto, InternalLifecycleState, SecurityPolicy, UiPreferences, SECURITY_POLICY_KEY,
    UI_PREFERENCES_KEY,
};
use human_money_core::app_service::{AppFacadeError, AppService};

/// Typed atomic persistence builder for encrypted profile stores (`generic_<name>.enc`).
///
/// Serializes all staged stores upfront, commits them in a single `AppService::save_encrypted_batch`
/// transaction, and updates all corresponding [`ActiveProfileCaches`] slots under one mutex
/// acquisition strictly after disk persistence succeeds.
#[derive(Default)]
pub(crate) struct EncryptedBatchCommit {
    pub security_policy: Option<SecurityPolicy>,
    pub ui_preferences: Option<UiPreferences>,
    pub own_market_profile: Option<crate::models::StoredOwnMarketProfile>,
    pub sync_admin: Option<Option<StoredSyncAdmin>>,
    pub wot: Option<humoco_wot::TrustRegister>,
    pub contacts: Option<FrontendAddressBook>,
    pub card_store: Option<humoco_market::CardStore>,
    pub sync_state: Option<StoredSyncState>,
    pub post_state: Option<StoredPostState>,
    pub app_notices: Option<crate::models::StoredAppNotices>,
    pub outgoing_bundles: Option<crate::models::StoredOutgoingBundles>,
    pub history: Option<Vec<TransactionRecord>>,
    pub recovery_owner: Option<crate::models::recovery::RecoveryOwnerState>,
    pub guardian_vault: Option<humoco_market::GuardianVaultStore>,
    pub server_offenders: Option<humoco_wot::ServerOffenderList>,
    pub own_media_blobs: Vec<(String, Vec<u8>)>,
}

impl EncryptedBatchCommit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.security_policy.is_none()
            && self.ui_preferences.is_none()
            && self.own_market_profile.is_none()
            && self.sync_admin.is_none()
            && self.wot.is_none()
            && self.contacts.is_none()
            && self.card_store.is_none()
            && self.sync_state.is_none()
            && self.post_state.is_none()
            && self.app_notices.is_none()
            && self.outgoing_bundles.is_none()
            && self.history.is_none()
            && self.recovery_owner.is_none()
            && self.guardian_vault.is_none()
            && self.server_offenders.is_none()
            && self.own_media_blobs.is_empty()
    }

    pub fn security_policy(&mut self, value: SecurityPolicy) -> &mut Self {
        self.security_policy = Some(value);
        self
    }

    pub fn ui_preferences(&mut self, value: UiPreferences) -> &mut Self {
        self.ui_preferences = Some(value);
        self
    }

    pub fn own_market_profile(
        &mut self,
        value: crate::models::StoredOwnMarketProfile,
    ) -> &mut Self {
        self.own_market_profile = Some(value);
        self
    }

    pub fn sync_admin(&mut self, value: StoredSyncAdmin) -> &mut Self {
        self.sync_admin = Some(Some(value));
        self
    }

    pub fn clear_sync_admin(&mut self) -> &mut Self {
        self.sync_admin = Some(None);
        self
    }

    pub fn wot(&mut self, value: humoco_wot::TrustRegister) -> &mut Self {
        self.wot = Some(value);
        self
    }

    pub fn contacts(&mut self, value: FrontendAddressBook) -> &mut Self {
        self.contacts = Some(value);
        self
    }

    pub fn card_store(&mut self, value: humoco_market::CardStore) -> &mut Self {
        self.card_store = Some(value);
        self
    }

    pub fn sync_state(&mut self, value: StoredSyncState) -> &mut Self {
        self.sync_state = Some(value);
        self
    }

    pub fn post_state(&mut self, value: StoredPostState) -> &mut Self {
        self.post_state = Some(value);
        self
    }

    pub fn app_notices(&mut self, value: crate::models::StoredAppNotices) -> &mut Self {
        self.app_notices = Some(value);
        self
    }

    pub fn outgoing_bundles(&mut self, value: crate::models::StoredOutgoingBundles) -> &mut Self {
        self.outgoing_bundles = Some(value);
        self
    }

    pub fn history(&mut self, value: Vec<TransactionRecord>) -> &mut Self {
        self.history = Some(value);
        self
    }

    pub fn recovery_owner(
        &mut self,
        value: crate::models::recovery::RecoveryOwnerState,
    ) -> &mut Self {
        self.recovery_owner = Some(value);
        self
    }

    #[allow(dead_code)]
    pub fn guardian_vault(&mut self, value: humoco_market::GuardianVaultStore) -> &mut Self {
        self.guardian_vault = Some(value);
        self
    }

    #[allow(dead_code)]
    pub fn server_offenders(&mut self, value: humoco_wot::ServerOffenderList) -> &mut Self {
        self.server_offenders = Some(value);
        self
    }

    pub fn own_media_blob(&mut self, storage_key: String, bytes: Vec<u8>) -> &mut Self {
        self.own_media_blobs.push((storage_key, bytes));
        self
    }

    pub fn commit(
        self,
        state: &AppState,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        use std::borrow::Cow;

        if self.is_empty() {
            return Ok(());
        }

        fn serialize_slot<T: serde::Serialize>(
            key: &'static str,
            value: &T,
        ) -> Result<(Cow<'static, str>, Vec<u8>), AppFacadeError> {
            let bytes = serde_json::to_vec(value)
                .map_err(|e| AppFacadeError::Json(format!("Failed to serialize {}: {}", key, e)))?;
            Ok((Cow::Borrowed(key), bytes))
        }

        let mut owned_blobs: Vec<(Cow<'static, str>, Vec<u8>)> =
            Vec::with_capacity(15 + self.own_media_blobs.len());
        if let Some(ref val) = self.security_policy {
            owned_blobs.push(serialize_slot(SECURITY_POLICY_KEY, val)?);
        }
        if let Some(ref val) = self.ui_preferences {
            owned_blobs.push(serialize_slot(UI_PREFERENCES_KEY, val)?);
        }
        if let Some(ref val) = self.own_market_profile {
            owned_blobs.push(serialize_slot(MARKET_OWN_PROFILE_KEY, val)?);
        }
        if let Some(ref val) = self.sync_admin {
            match val {
                Some(admin) => owned_blobs.push(serialize_slot(MARKET_SYNC_ADMIN_KEY, admin)?),
                None => owned_blobs.push(serialize_slot(
                    MARKET_SYNC_ADMIN_KEY,
                    &Option::<StoredSyncAdmin>::None,
                )?),
            }
        }
        if let Some(ref val) = self.wot {
            owned_blobs.push(serialize_slot(WOT_DATA_NAME, val)?);
        }
        if let Some(ref val) = self.contacts {
            owned_blobs.push(serialize_slot(CONTACTS_DATA_NAME, val)?);
        }
        if let Some(ref val) = self.card_store {
            owned_blobs.push(serialize_slot(MARKET_CARD_STORE_KEY, val)?);
        }
        if let Some(ref val) = self.sync_state {
            owned_blobs.push(serialize_slot(MARKET_SYNC_KEY, val)?);
        }
        if let Some(ref val) = self.post_state {
            owned_blobs.push(serialize_slot(MARKET_POST_STATE_KEY, val)?);
        }
        if let Some(ref val) = self.app_notices {
            owned_blobs.push(serialize_slot(APP_NOTICES_KEY, val)?);
        }
        if let Some(ref val) = self.outgoing_bundles {
            owned_blobs.push(serialize_slot(OUTGOING_BUNDLES_KEY, val)?);
        }
        if let Some(ref val) = self.history {
            owned_blobs.push(serialize_slot(TRANSACTION_HISTORY_KEY, val)?);
        }
        if let Some(ref val) = self.recovery_owner {
            owned_blobs.push(serialize_slot(RECOVERY_OWNER_STATE_KEY, val)?);
        }
        if let Some(ref val) = self.guardian_vault {
            owned_blobs.push(serialize_slot(RECOVERY_GUARDIAN_VAULT_KEY, val)?);
        }
        if let Some(ref val) = self.server_offenders {
            owned_blobs.push(serialize_slot(
                crate::l2_lifecycle::SERVER_OFFENDER_LIST_KEY,
                val,
            )?);
        }
        for (key, bytes) in self.own_media_blobs {
            debug_assert!(
                key.starts_with("market_own_media_"),
                "own_media_blobs must only carry market_own_media_* keys"
            );
            owned_blobs.push((Cow::Owned(key), bytes));
        }

        let batch_refs: Vec<(&str, &[u8])> = owned_blobs
            .iter()
            .map(|(k, v)| (k.as_ref(), v.as_slice()))
            .collect();
        if let Err(err) = service.save_encrypted_batch(&batch_refs, password) {
            state.sync_if_core_locked(service);
            return Err(err);
        }

        state.write_caches_for_service(service, |caches| {
            if let Some(val) = self.security_policy {
                caches.security_policy = Some(val);
            }
            if let Some(val) = self.ui_preferences {
                caches.ui_preferences = Some(val);
            }
            if let Some(val) = self.own_market_profile {
                caches.own_market_profile = Some(val);
            }
            if let Some(val) = self.sync_admin {
                caches.own_sync_descriptor = Some(val.map(|a| a.descriptor));
            }
            if let Some(val) = self.wot {
                caches.wot = Some(val);
            }
            if let Some(val) = self.contacts {
                caches.contacts = Some(val);
            }
            if let Some(val) = self.card_store {
                caches.card_store = Some(val);
            }
            if let Some(val) = self.sync_state {
                caches.sync_state = Some(val);
            }
            if let Some(val) = self.post_state {
                caches.post_state = Some(val);
            }
            if let Some(val) = self.app_notices {
                caches.app_notices = Some(val);
            }
            if let Some(val) = self.outgoing_bundles {
                caches.outgoing_bundles = Some(val);
            }
            if let Some(val) = self.history {
                caches.history = Some(val);
            }
            if let Some(val) = self.recovery_owner {
                caches.recovery_owner = Some(Some(val));
            }
            if let Some(val) = self.guardian_vault {
                caches.guardian_vault = Some(val);
            }
            if let Some(val) = self.server_offenders {
                caches.server_offenders = Some(val);
            }
        });
        Ok(())
    }
}

impl AppState {
    pub fn is_media_negatively_cached(
        &self,
        session: &SessionToken,
        blob_id: &str,
        read_url: &str,
        now_secs: u64,
    ) -> bool {
        if self.current_session_generation() != session.generation {
            return false;
        }
        let key = (
            blob_id.to_string(),
            read_url.trim_end_matches('/').to_string(),
        );
        let guard = self.profile_caches.lock().unwrap();
        let Some(ref caches) = *guard else {
            return false;
        };
        if caches.generation != session.generation || caches.owner_did != session.owner_did {
            return false;
        }
        caches
            .media_negative_cache
            .get(&key)
            .is_some_and(|&expires_at| now_secs < expires_at)
    }

    pub fn record_media_negative_cache(
        &self,
        session: &SessionToken,
        blob_id: String,
        read_url: String,
        now_secs: u64,
        ttl_secs: u64,
    ) {
        if self.current_session_generation() != session.generation {
            return;
        }
        let key = (blob_id, read_url.trim_end_matches('/').to_string());
        let mut guard = self.profile_caches.lock().unwrap();
        let needs_init = guard
            .as_ref()
            .is_none_or(|c| c.generation != session.generation || c.owner_did != session.owner_did);
        if needs_init {
            *guard = Some(ActiveProfileCaches::new(
                session.generation,
                session.owner_did.clone(),
            ));
        }
        if let Some(ref mut caches) = *guard {
            if caches.media_negative_cache.len() >= 1024 {
                caches
                    .media_negative_cache
                    .retain(|_, &mut expires_at| expires_at > now_secs);
                if caches.media_negative_cache.len() >= 1024 {
                    caches.media_negative_cache.clear();
                }
            }
            caches
                .media_negative_cache
                .insert(key, now_secs.saturating_add(ttl_secs));
        }
    }

    // 2. Settings & Cache Helpers (Unified ActiveProfileCaches under single mutex)
    pub(super) fn get_cached_store<T: Clone>(
        &self,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        name: &str,
    ) -> Result<T, AppFacadeError> {
        let current_gen = self.current_session_generation();
        let mut guard = self.profile_caches.lock().unwrap();
        guard
            .as_mut()
            .filter(|c| c.generation == current_gen)
            .and_then(|c| field(c).clone())
            .ok_or_else(|| app_state_err(format!("{} not loaded", name)))
    }

    pub(crate) fn read_cache_slot_for_service<T: Clone>(
        &self,
        service: &AppService,
        field: fn(&ActiveProfileCaches) -> &Option<T>,
    ) -> Option<T> {
        let current_gen = self.current_session_generation();
        let owner_did = service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap_or_default();
        let guard = self.profile_caches.lock().unwrap();
        let caches = guard.as_ref()?;
        if caches.generation != current_gen
            || (!owner_did.is_empty() && caches.owner_did != owner_did)
        {
            return None;
        }
        field(caches).clone()
    }

    fn read_cache_slot_current_gen<T: Clone>(
        &self,
        field: fn(&ActiveProfileCaches) -> &Option<T>,
    ) -> Option<T> {
        let current_gen = self.current_session_generation();
        let guard = self.profile_caches.lock().unwrap();
        guard
            .as_ref()
            .filter(|c| c.generation == current_gen)
            .and_then(|c| field(c).clone())
    }

    pub(crate) fn write_caches_for_service(
        &self,
        service: &AppService,
        update: impl FnOnce(&mut ActiveProfileCaches),
    ) {
        let current_gen = self.current_session_generation();
        let owner_did = service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap_or_default();
        let mut guard = self.profile_caches.lock().unwrap();
        let needs_reset = guard.as_ref().is_none_or(|c| {
            c.generation != current_gen || (!owner_did.is_empty() && c.owner_did != owner_did)
        });
        if needs_reset {
            *guard = Some(ActiveProfileCaches::new(current_gen, owner_did));
        }
        if let Some(ref mut caches) = *guard {
            update(caches);
        }
    }

    pub(crate) fn write_cache_slot_for_service<T>(
        &self,
        service: &AppService,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        value: Option<T>,
    ) {
        self.write_caches_for_service(service, |caches| {
            *field(caches) = value;
        });
    }

    pub fn get_or_load_server_offender_list(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<humoco_wot::ServerOffenderList, crate::models::FrontendError> {
        if let Some(cached) = self.read_cache_slot_for_service(service, |c| &c.server_offenders) {
            return Ok(cached);
        }
        let list = crate::l2_lifecycle::load_server_offender_list(service, password)?;
        self.write_cache_slot_for_service(service, |c| &mut c.server_offenders, Some(list.clone()));
        Ok(list)
    }

    pub fn get_or_load_own_sync_descriptor(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<Option<humoco_market::DropDescriptor>, crate::models::FrontendError> {
        if let Some(cached) = self.read_cache_slot_for_service(service, |c| &c.own_sync_descriptor)
        {
            return Ok(cached);
        }
        let desc =
            crate::services::sync_engine::admin::load_stored_sync_admin_opt(service, password)?
                .map(|a| a.descriptor);
        self.write_cache_slot_for_service(
            service,
            |c| &mut c.own_sync_descriptor,
            Some(desc.clone()),
        );
        Ok(desc)
    }

    pub fn cache_contacts(&self) -> Option<FrontendAddressBook> {
        self.read_cache_slot_current_gen(|c| &c.contacts)
    }

    pub fn set_cached_contacts(&self, value: Option<FrontendAddressBook>) {
        self.set_cache_slot_direct(|c| &mut c.contacts, value);
    }

    pub fn set_cached_contacts_for_service(
        &self,
        service: &AppService,
        value: Option<FrontendAddressBook>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.contacts, value);
    }

    pub fn with_cached_contacts_mut<R>(
        &self,
        f: impl FnOnce(&mut Option<FrontendAddressBook>) -> R,
    ) -> R {
        let current_gen = self.current_session_generation();
        let mut guard = self.profile_caches.lock().unwrap();
        if let Some(ref mut caches) = *guard {
            if caches.generation == current_gen {
                return f(&mut caches.contacts);
            }
        }
        let mut none = None;
        f(&mut none)
    }

    pub fn cache_events(&self) -> Option<Vec<human_money_core::models::wallet_event::WalletEvent>> {
        self.read_cache_slot_current_gen(|c| &c.events)
    }

    pub fn set_cached_events(
        &self,
        value: Option<Vec<human_money_core::models::wallet_event::WalletEvent>>,
    ) {
        self.set_cache_slot_direct(|c| &mut c.events, value);
    }

    pub fn cache_wot(&self) -> Option<humoco_wot::TrustRegister> {
        self.read_cache_slot_current_gen(|c| &c.wot)
    }

    pub fn cache_ui_preferences(&self) -> Option<UiPreferences> {
        self.read_cache_slot_current_gen(|c| &c.ui_preferences)
    }

    pub fn set_cached_ui_preferences(&self, value: Option<UiPreferences>) {
        self.set_cache_slot_direct(|c| &mut c.ui_preferences, value);
    }

    pub fn set_cached_ui_preferences_for_service(
        &self,
        service: &AppService,
        value: Option<UiPreferences>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.ui_preferences, value);
    }

    pub fn set_cached_wot_for_service(
        &self,
        service: &AppService,
        value: Option<humoco_wot::TrustRegister>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.wot, value);
    }

    pub fn cache_security_policy(&self) -> Option<SecurityPolicy> {
        self.read_cache_slot_current_gen(|c| &c.security_policy)
    }

    pub fn set_cached_security_policy(&self, value: Option<SecurityPolicy>) {
        self.set_cache_slot_direct(|c| &mut c.security_policy, value);
    }

    pub fn cache_internal_lifecycle(&self) -> Option<InternalLifecycleState> {
        self.read_cache_slot_current_gen(|c| &c.internal_lifecycle)
    }

    pub fn set_cached_internal_lifecycle(&self, value: Option<InternalLifecycleState>) {
        self.set_cache_slot_direct(|c| &mut c.internal_lifecycle, value);
    }

    pub fn cache_history(&self) -> Option<Vec<TransactionRecord>> {
        self.read_cache_slot_current_gen(|c| &c.history)
    }

    pub fn set_cached_history(&self, value: Option<Vec<TransactionRecord>>) {
        self.set_cache_slot_direct(|c| &mut c.history, value);
    }

    pub fn cache_own_market_profile(&self) -> Option<crate::models::StoredOwnMarketProfile> {
        self.read_cache_slot_current_gen(|c| &c.own_market_profile)
    }

    pub fn set_cached_own_market_profile_for_service(
        &self,
        service: &AppService,
        value: Option<crate::models::StoredOwnMarketProfile>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.own_market_profile, value);
    }

    pub fn cache_recovery_owner(
        &self,
    ) -> Option<Option<crate::models::recovery::RecoveryOwnerState>> {
        self.read_cache_slot_current_gen(|c| &c.recovery_owner)
    }

    pub fn set_cached_recovery_owner_for_service(
        &self,
        service: &AppService,
        value: Option<Option<crate::models::recovery::RecoveryOwnerState>>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.recovery_owner, value);
    }

    pub fn cache_guardian_vault(&self) -> Option<humoco_market::GuardianVaultStore> {
        self.read_cache_slot_current_gen(|c| &c.guardian_vault)
    }

    pub fn set_cached_guardian_vault_for_service(
        &self,
        service: &AppService,
        value: Option<humoco_market::GuardianVaultStore>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.guardian_vault, value);
    }

    pub fn cache_app_notices(&self) -> Option<crate::models::StoredAppNotices> {
        self.read_cache_slot_current_gen(|c| &c.app_notices)
    }

    pub fn set_cached_card_store_for_service(
        &self,
        service: &AppService,
        value: Option<humoco_market::CardStore>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.card_store, value);
    }

    pub fn set_cached_sync_state_for_service(
        &self,
        service: &AppService,
        value: Option<StoredSyncState>,
    ) {
        self.write_cache_slot_for_service(service, |c| &mut c.sync_state, value);
    }

    fn set_cache_slot_direct<T>(
        &self,
        field: fn(&mut ActiveProfileCaches) -> &mut Option<T>,
        value: Option<T>,
    ) {
        let current_gen = self.current_session_generation();
        let mut guard = self.profile_caches.lock().unwrap();
        if value.is_some() && guard.is_none() {
            *guard = Some(ActiveProfileCaches::new(current_gen, String::new()));
        }
        if let Some(ref mut caches) = *guard {
            if caches.generation == current_gen {
                *field(caches) = value;
            }
        }
    }

    pub fn get_cached_ui_preferences(&self) -> Result<UiPreferences, AppFacadeError> {
        self.get_cached_store(|c| &mut c.ui_preferences, "UiPreferences")
    }

    pub fn get_cached_security_policy(&self) -> Result<SecurityPolicy, AppFacadeError> {
        self.get_cached_store(|c| &mut c.security_policy, "SecurityPolicy")
    }

    pub fn get_cached_internal_lifecycle(&self) -> Result<InternalLifecycleState, AppFacadeError> {
        self.get_cached_store(|c| &mut c.internal_lifecycle, "InternalLifecycleState")
    }

    pub fn get_cached_app_config(&self) -> Result<AppConfigDto, AppFacadeError> {
        let current_gen = self.current_session_generation();
        let guard = self.profile_caches.lock().unwrap();
        let caches = guard
            .as_ref()
            .filter(|c| c.generation == current_gen)
            .ok_or_else(|| app_state_err("AppConfig caches not loaded".to_string()))?;
        Ok(AppConfigDto {
            ui_preferences: caches.ui_preferences.clone().unwrap_or_default(),
            security_policy: caches
                .security_policy
                .clone()
                .ok_or_else(|| app_state_err("SecurityPolicy not loaded".to_string()))?,
            lifecycle: caches
                .internal_lifecycle
                .clone()
                .unwrap_or(InternalLifecycleState {
                    pending_restore_l2_reconciliation: true,
                    ..Default::default()
                })
                .into(),
        })
    }

    pub fn get_cached_contacts(&self) -> Result<Vec<FrontendContact>, AppFacadeError> {
        let current_gen = self.current_session_generation();
        let guard = self.profile_caches.lock().unwrap();
        let Some(caches) = guard.as_ref().filter(|c| c.generation == current_gen) else {
            return Err(app_state_err("Contacts cache not initialized".to_string()));
        };
        if let Some(book) = caches.contacts.as_ref() {
            let mut contacts: Vec<FrontendContact> = book.contacts.values().cloned().collect();
            for c in contacts.iter_mut() {
                enrich_contact_with_wot(c, caches.wot.as_ref());
            }
            contacts.sort_by(|a, b| {
                let name_a = a.profile.first_name.as_deref().unwrap_or("");
                let name_b = b.profile.first_name.as_deref().unwrap_or("");
                name_a.to_lowercase().cmp(&name_b.to_lowercase())
            });
            Ok(contacts)
        } else {
            Err(app_state_err("Contacts cache not initialized".to_string()))
        }
    }

    pub fn get_cached_history(&self) -> Result<Vec<TransactionRecord>, AppFacadeError> {
        self.get_cached_store(|c| &mut c.history, "History cache")
    }
}
