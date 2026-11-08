// src-tauri/src/state.rs
use std::sync::{Mutex, MutexGuard};
use log::{info, error};
use chrono::{Duration, Utc};
use human_money_core::app_service::{AppService, AppFacadeError};
use crate::models::{TransactionRecord, FrontendAddressBook, FrontendContact};
use crate::settings::{AppSettings, SETTINGS_KEY};

pub const TRANSACTION_HISTORY_KEY: &str = "transaction_history";
pub const CONTACTS_DATA_NAME: &str = "address_book";
pub const WOT_DATA_NAME: &str = "wot_register";
pub const MARKET_CARD_STORE_KEY: &str = "market_card_store";
pub const MARKET_OWN_CARD_KEY: &str = "market_own_card";
pub const MARKET_DROP_ADMIN_KEY: &str = "market_drop_admin";
pub const MARKET_DROP_SYNC_KEY: &str = "market_drop_sync";
pub const MARKET_POST_STATE_KEY: &str = "market_post_state";
pub const BUNDLE_RETENTION_DAYS: i64 = 90;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PostOutboxEntry {
    pub msg_id: String,
    pub recipient_did: String,
    pub tag16_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    pub entry: humoco_market::PostBoxEntry,
    pub created_at: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredPostState {
    #[serde(default)]
    pub outbox_entries: Vec<PostOutboxEntry>,
    #[serde(default)]
    pub slot_etags_by_write_url: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub processed_msg_ids: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub pending_sign_requests: Vec<crate::models::FrontendPendingSignRequest>,
    #[serde(default)]
    pub pending_wot_handshakes: Vec<crate::models::FrontendPendingWotHandshake>,
    #[serde(default)]
    pub pending_wot_packages_by_msg_id: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub seller_inquiries: Vec<crate::models::FrontendMarketInquiryItem>,
    #[serde(default)]
    pub buyer_inquiries: Vec<crate::models::FrontendMarketInquiryItem>,
    #[serde(default)]
    pub inquiry_rate_tracker: humoco_market::InquiryRateTracker,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct StoredOwnCard {
    pub card: humoco_market::ContactCard,
    pub card_jws: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StoredDropAdmin {
    pub descriptor: humoco_market::DropDescriptor,
    #[serde(default)]
    pub drop_index: u32,
    #[serde(default)]
    pub generation: u32,
    #[serde(default)]
    pub previous_generation: Option<u32>,
    #[serde(default)]
    pub rotation_retire_at: Option<u64>,
    pub data_dir: String,
    pub max_members: usize,
    #[serde(default = "default_max_total_storage_mb")]
    pub max_total_storage_mb: usize,
    #[serde(default)]
    pub register_url: Option<String>,
    pub policies: std::collections::BTreeMap<String, humoco_market::MemberPolicy>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub blocked_pubkeys: Vec<String>,
}

fn default_max_total_storage_mb() -> usize {
    256
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredDropSync {
    pub last_upload_at: Option<u64>,
    pub last_readers_hash: Option<String>,
    pub last_uploaded_card_v: Option<u64>,
    #[serde(default)]
    pub etags_by_read_url: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub last_modified_by_read_url: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub slot_mtimes: std::collections::BTreeMap<String, u64>,
    #[serde(default)]
    pub drop_sources_by_did: std::collections::BTreeMap<String, String>,
}

pub struct AppState {
    pub service: Mutex<AppService>,
    pub history: Mutex<Option<Vec<TransactionRecord>>>,
    pub events: Mutex<Option<Vec<human_money_core::models::wallet_event::WalletEvent>>>,
    pub contacts: Mutex<Option<crate::models::FrontendAddressBook>>,
    pub settings: Mutex<Option<AppSettings>>,
    pub(crate) regional_views: Mutex<Option<crate::checkout::RegionalViewCache>>,
    pub wot: Mutex<Option<humoco_wot::TrustRegister>>,
}

impl AppState {
    // 1. Central helper method to access the locked service
    pub fn service(&self) -> MutexGuard<'_, AppService> {
        self.service.lock().unwrap()
    }

    // 2. Settings Helpers
    pub fn get_cached_settings(&self) -> Result<AppSettings, AppFacadeError> {
        let cache = self.settings.lock().unwrap();
        cache
            .as_ref()
            .cloned()
            .ok_or_else(|| AppFacadeError::Generic("Settings not loaded".to_string()))
    }

    pub fn load_settings(&self, service: &mut AppService, password: Option<&str>) -> Result<AppSettings, AppFacadeError> {
        let settings: AppSettings = match service.load_encrypted_data(SETTINGS_KEY, password) {
            Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
            Err(_) => {
                info!("No settings file found, creating default settings.");
                let default_settings = AppSettings::default();
                let bytes = serde_json::to_vec(&default_settings).unwrap();
                service
                    .save_encrypted_data(SETTINGS_KEY, &bytes, password)
                    .map_err(|e| AppFacadeError::Generic(format!("Failed to save default settings: {}", e)))?;
                default_settings
            }
        };

        *self.settings.lock().unwrap() = Some(settings.clone());
        Ok(settings)
    }

    pub fn save_settings(&self, service: &mut AppService, settings: AppSettings, password: Option<&str>) -> Result<(), AppFacadeError> {
        let bytes = serde_json::to_vec(&settings).map_err(|e| AppFacadeError::Json(e.to_string()))?;
        service.save_encrypted_data(SETTINGS_KEY, &bytes, password)?;
        *self.settings.lock().unwrap() = Some(settings);
        Ok(())
    }

    // 3. Contacts Helpers
    pub fn get_cached_contacts(&self) -> Result<Vec<FrontendContact>, AppFacadeError> {
        let cache = self.contacts.lock().unwrap();
        if let Some(book) = cache.as_ref() {
            let wot_guard = self.wot.lock().unwrap();
            let mut contacts: Vec<FrontendContact> = book.contacts.values().cloned().collect();
            for c in contacts.iter_mut() {
                enrich_contact_with_wot(c, wot_guard.as_ref());
            }
            contacts.sort_by(|a, b| {
                let name_a = a.profile.first_name.as_deref().unwrap_or("");
                let name_b = b.profile.first_name.as_deref().unwrap_or("");
                name_a.to_lowercase().cmp(&name_b.to_lowercase())
            });
            Ok(contacts)
        } else {
            Err(AppFacadeError::Generic("Contacts cache not initialized".to_string()))
        }
    }

    pub fn load_contacts(&self, service: &mut AppService, password: Option<&str>) -> Result<Vec<FrontendContact>, AppFacadeError> {
        info!("Fetching all contacts from address book from disk...");
        match service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
            Ok(data) => {
                let address_book: FrontendAddressBook = serde_json::from_slice(&data)
                    .map_err(|e| {
                        error!("Failed to parse address book: {}", e);
                        AppFacadeError::Json(format!("Failed to parse address book: {}", e))
                    })?;

                *self.contacts.lock().unwrap() = Some(address_book.clone());

                let wot_guard = self.wot.lock().unwrap();
                let card_store: Option<humoco_market::CardStore> = service
                    .load_encrypted_data(MARKET_CARD_STORE_KEY, password)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok());

                let mut contacts: Vec<FrontendContact> = address_book.contacts.into_values().collect();
                for c in contacts.iter_mut() {
                    enrich_contact_with_wot(c, wot_guard.as_ref());
                    if let Some(ref store) = card_store {
                        if let Ok(can_did) = canonicalize_did(&c.did) {
                            if let Some(stored) = store
                                .cards
                                .values()
                                .find(|sc| canonicalize_did(&sc.card.did).ok().as_deref() == Some(&can_did))
                            {
                                let card_drops: Vec<crate::models::FrontendDropDescriptor> =
                                    stored.card.drops.iter().cloned().map(Into::into).collect();
                                c.drops = merge_contact_drops(&c.drops, &card_drops);
                            }
                        }
                    }
                    c.drops.truncate(3);
                }
                contacts.sort_by(|a, b| {
                    let name_a = a.profile.first_name.as_deref().unwrap_or("");
                    let name_b = b.profile.first_name.as_deref().unwrap_or("");
                    name_a.to_lowercase().cmp(&name_b.to_lowercase())
                });
                Ok(contacts)
            }
            Err(e) => {
                if matches!(&e, AppFacadeError::Storage(human_money_core::StorageError::NotFound)) {
                    info!("No address book found, returning empty list.");
                    let default_book = FrontendAddressBook::default();
                    *self.contacts.lock().unwrap() = Some(default_book);
                    Ok(vec![])
                } else if matches!(&e, AppFacadeError::WalletLocked | AppFacadeError::SessionExpired(_) | AppFacadeError::SessionNotActive(_)) {
                    error!("Error loading address book (locked/session issue): {:?}", e);
                    Err(e)
                } else {
                    error!("Error loading address book: {:?}", e);
                    Ok(vec![])
                }
            }
        }
    }

    pub fn save_contact(&self, service: &mut AppService, mut contact: FrontendContact, password: Option<&str>) -> Result<(), AppFacadeError> {
        let mut address_book = match service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
            Ok(data) => serde_json::from_slice::<FrontendAddressBook>(&data).unwrap_or_default(),
            Err(e) => {
                if matches!(&e, AppFacadeError::WalletLocked | AppFacadeError::SessionExpired(_) | AppFacadeError::SessionNotActive(_)) {
                    return Err(e);
                }
                FrontendAddressBook::default()
            }
        };

        let can_did = canonicalize_did(&contact.did)
            .map_err(|e| AppFacadeError::Generic(format!("Invalid contact DID: {}", e)))?;

        // Deduplicate address_book.contacts by canonical DID
        let keys_to_remove: Vec<String> = address_book.contacts.keys()
            .filter(|k| {
                if *k == &contact.did {
                    false
                } else if let Ok(k_can) = canonicalize_did(k) {
                    k_can == can_did
                } else {
                    false
                }
            })
            .cloned()
            .collect();

        for k in keys_to_remove {
            if let Some(old) = address_book.contacts.remove(&k) {
                if contact.added_at.is_empty() {
                    contact.added_at = old.added_at;
                }
            }
        }

        // Synchronize TrustRegister if wallet is unlocked
        if service.with_wallet(|_| ()).is_ok() {
            let mut reg = self.get_or_load_wot_register(service, password)?;
            let now_secs = Utc::now().timestamp() as u64;

            if let Some(level) = contact.trust_level {
                if level > 3 {
                    return Err(AppFacadeError::Generic(format!("Invalid trust level: {}", level)));
                }
                let hops = if level == 0 { 0 } else { reg.policy.own_list_max_hops };
                reg.set_contact_trust(&can_did, level, hops, now_secs)
                    .map_err(|e| AppFacadeError::Generic(e.to_string()))?;
                reg.prune_unreachable_lists();
                self.ensure_wot_list_signed(service, &mut reg)?;
                self.save_wot_register(service, &reg, password)?;
                contact.trust_level = Some(level);
                contact.desired_hops = Some(hops);
            } else {
                let removed = reg.remove_contact_trust(&can_did)
                    .map_err(|e| AppFacadeError::Generic(e.to_string()))?;
                if removed {
                    reg.prune_unreachable_lists();
                    if reg.own_list_dirty {
                        self.ensure_wot_list_signed(service, &mut reg)?;
                    }
                    self.save_wot_register(service, &reg, password)?;
                }
            }
        }

        address_book.contacts.insert(contact.did.clone(), contact);

        let data = serde_json::to_vec(&address_book)
            .map_err(|e| AppFacadeError::Json(format!("Failed to serialize address book: {}", e)))?;

        service.save_encrypted_data(CONTACTS_DATA_NAME, &data, password)
            .map_err(|e| AppFacadeError::Generic(format!("Failed to save address book: {}", e)))?;

        *self.contacts.lock().unwrap() = Some(address_book);
        Ok(())
    }

    pub fn delete_contact(&self, service: &mut AppService, did: &str, password: Option<&str>) -> Result<(), AppFacadeError> {
        let mut address_book = match service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
            Ok(data) => serde_json::from_slice::<FrontendAddressBook>(&data).unwrap_or_default(),
            Err(e) => {
                if matches!(&e, AppFacadeError::WalletLocked | AppFacadeError::SessionExpired(_) | AppFacadeError::SessionNotActive(_)) {
                    return Err(e);
                }
                return Ok(());
            }
        };

        let can_target = canonicalize_did(did).ok();
        let keys_to_remove: Vec<String> = address_book.contacts.keys()
            .filter(|k| *k == did || (can_target.is_some() && canonicalize_did(k).ok() == can_target))
            .cloned()
            .collect();

        let mut changed = false;
        for k in keys_to_remove {
            if address_book.contacts.remove(&k).is_some() {
                changed = true;
            }
        }

        if changed {
            let data = serde_json::to_vec(&address_book)
                .map_err(|e| AppFacadeError::Json(format!("Failed to serialize address book: {}", e)))?;

            service.save_encrypted_data(CONTACTS_DATA_NAME, &data, password)
                .map_err(|e| AppFacadeError::Generic(format!("Failed to save address book: {}", e)))?;

            *self.contacts.lock().unwrap() = Some(address_book);
        }

        if service.with_wallet(|_| ()).is_ok() {
            if let Ok(mut reg) = self.get_or_load_wot_register(service, password) {
                let target_key = can_target.as_deref().unwrap_or(did);
                if let Ok(removed) = reg.remove_contact_trust(target_key) {
                    if removed {
                        reg.prune_unreachable_lists();
                        if reg.own_list_dirty {
                            self.ensure_wot_list_signed(service, &mut reg)?;
                        }
                        self.save_wot_register(service, &reg, password)?;
                    }
                }
            }
        }

        Ok(())
    }

    // 4. History Helpers
    pub fn get_cached_history(&self) -> Result<Vec<TransactionRecord>, AppFacadeError> {
        let cache = self.history.lock().unwrap();
        cache
            .as_ref()
            .cloned()
            .ok_or_else(|| AppFacadeError::Generic("History cache not initialized".to_string()))
    }

    pub fn load_history_from_disk(&self, service: &mut AppService, password: Option<&str>) -> Result<Vec<TransactionRecord>, AppFacadeError> {
        match service.load_encrypted_data(TRANSACTION_HISTORY_KEY, password) {
            Ok(data) => {
                let records: Result<Vec<TransactionRecord>, _> = serde_json::from_slice(&data);
                match records {
                    Ok(parsed_records) => {
                        *self.history.lock().unwrap() = Some(parsed_records.clone());
                        Ok(parsed_records)
                    }
                    Err(e) => {
                        let msg = format!("Failed to parse transaction history: {}", e);
                        error!("{}", msg);
                        Err(AppFacadeError::Json(msg))
                    }
                }
            }
            Err(e) => {
                if matches!(&e, AppFacadeError::WalletLocked | AppFacadeError::SessionExpired(_) | AppFacadeError::SessionNotActive(_)) {
                    return Err(e);
                }
                info!("Keine Transaktionshistorie auf Festplatte gefunden. Erstelle neue.");
                let empty = Vec::new();
                *self.history.lock().unwrap() = Some(empty.clone());
                Ok(empty)
            }
        }
    }

    pub fn save_history(&self, service: &mut AppService, history: Vec<TransactionRecord>, password: Option<&str>) -> Result<(), AppFacadeError> {
        let history_bytes = serde_json::to_vec(&history).map_err(|e| AppFacadeError::Json(e.to_string()))?;
        service.save_encrypted_data(TRANSACTION_HISTORY_KEY, &history_bytes, password)?;
        *self.history.lock().unwrap() = Some(history);
        Ok(())
    }

    pub fn append_to_history(&self, service: &mut AppService, record: TransactionRecord, password: Option<&str>) -> Result<(), AppFacadeError> {
        let mut history = self.load_history_from_disk(service, password)?;
        history.push(record);
        self.save_history(service, history, password)
    }

    // 5. Events Helpers
    pub fn refresh_events_cache(&self, service: &mut AppService, password: Option<&str>) -> Result<(), AppFacadeError> {
        info!("Refreshing events cache...");
        let events = service.get_event_history(0, 50, password)?;
        *self.events.lock().unwrap() = Some(events);
        Ok(())
    }

    // 6. Logout Helpers
    pub fn clear_all_caches(&self) {
        info!("Clearing all caches on logout...");
        *self.regional_views.lock().unwrap() = None;
        *self.history.lock().unwrap() = None;
        *self.events.lock().unwrap() = None;
        *self.contacts.lock().unwrap() = None;
        *self.settings.lock().unwrap() = None;
        *self.wot.lock().unwrap() = None;
    }

    /// Central session initializer — called after Login, Create Profile, Recovery, and Handover.
    /// Loads all caches (Settings, History, Events, Contacts, WoT) and starts the session timer.
    pub fn initialize_profile_session(
        &self,
        service: &mut AppService,
        password: &str,
    ) -> Result<(), AppFacadeError> {
        info!("Initializing profile session and member caches...");

        // 1. Load or create settings
        let settings = self.load_settings(service, Some(password))?;

        if let Some(policy) = crate::checkout::configured_policy(&settings)
            .map_err(|e| AppFacadeError::Generic(e.message))?
        {
            service
                .configure_regional_mesh_authority(policy, Some(password))
                .map_err(|e| AppFacadeError::Generic(e.to_string()))?;
        }

        // 2. Start session timer if configured
        if settings.session_timeout_seconds > 0 {
            service.unlock_session(password, settings.session_timeout_seconds)?;
        }

        // 3. Load transaction history and clean up expired bundle data
        let mut history = self.load_history_from_disk(service, Some(password))?;
        let mut changed = false;
        let retention_duration = Duration::days(BUNDLE_RETENTION_DAYS);

        for record in history.iter_mut() {
            if record.bundle_data.is_empty() {
                continue;
            }
            if let Ok(timestamp) = record.timestamp.parse::<chrono::DateTime<Utc>>() {
                if Utc::now().signed_duration_since(timestamp) > retention_duration {
                    info!("Clearing bundle data for old transaction record: {}", record.id);
                    record.bundle_data.clear();
                    changed = true;
                }
            }
        }

        if changed {
            info!("Saving cleaned transaction history back to disk...");
            self.save_history(service, history, Some(password))?;
        }

        // 4. Pre-cache the last 50 wallet events
        self.refresh_events_cache(service, Some(password))?;

        // 5. Load the address book
        let _ = self.load_contacts(service, Some(password));

        // 6. Load the Web of Trust register
        self.load_wot_register(service, Some(password))?;

        Ok(())
    }

    // 7. Web of Trust (WoT) Helpers
    pub fn ensure_wot_list_signed(
        &self,
        service: &mut AppService,
        reg: &mut humoco_wot::TrustRegister,
    ) -> Result<bool, AppFacadeError> {
        let now_secs = Utc::now().timestamp() as u64;
        service.with_wallet_and_identity(|_wallet, identity| {
            let signer = CoreIdentitySigner::new(identity);
            reg.ensure_own_list_signed(&signer, now_secs)
                .map_err(|e| AppFacadeError::Generic(format!("Failed to sign WoT trust list: {}", e)))
        })?
    }

    pub fn load_wot_register(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<humoco_wot::TrustRegister, AppFacadeError> {
        info!("Loading WoT register from disk...");
        let own_user_id = service.with_wallet(|w| w.get_user_id().to_string())?;
        let own_can_did = canonicalize_did(&own_user_id)
            .map_err(|e| AppFacadeError::Generic(format!("Invalid own wallet DID: {}", e)))?;
        let mut reg = match service.load_encrypted_data(WOT_DATA_NAME, password) {
            Ok(data) => {
                serde_json::from_slice::<humoco_wot::TrustRegister>(&data)
                    .map_err(|e| AppFacadeError::Json(format!("Failed to deserialize TrustRegister: {}", e)))?
            }
            Err(e) => {
                if matches!(&e, AppFacadeError::Storage(human_money_core::StorageError::NotFound)) {
                    info!("No WoT register found, creating new one for {}", own_can_did);
                    humoco_wot::TrustRegister::new(&own_can_did, humoco_wot::WotPolicy::default())
                        .map_err(|e| AppFacadeError::Generic(format!("Failed to initialize TrustRegister: {}", e)))?
                } else if matches!(&e, AppFacadeError::WalletLocked | AppFacadeError::SessionExpired(_) | AppFacadeError::SessionNotActive(_)) {
                    return Err(e);
                } else {
                    info!("Error loading WoT register ({:?}), creating fallback.", e);
                    humoco_wot::TrustRegister::new(&own_can_did, humoco_wot::WotPolicy::default())
                        .map_err(|e| AppFacadeError::Generic(format!("Failed to initialize TrustRegister: {}", e)))?
                }
            }
        };

        reg.policy.own_list_max_hops = reg.policy.own_list_max_hops.clamp(1, 4);
        reg.policy.max_eval_depth = reg.policy.max_eval_depth.max(5);
        reg.policy.max_import_depth = reg.policy.max_import_depth.max(5);
        for entry in reg.own_contacts.values_mut() {
            if entry.level >= 1 && entry.desired_hops != reg.policy.own_list_max_hops {
                entry.desired_hops = reg.policy.own_list_max_hops;
                reg.own_list_dirty = true;
            }
        }

        *self.wot.lock().unwrap() = Some(reg.clone());
        Ok(reg)
    }

    pub fn get_or_load_wot_register(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<humoco_wot::TrustRegister, AppFacadeError> {
        if let Some(reg) = self.wot.lock().unwrap().as_ref() {
            return Ok(reg.clone());
        }
        self.load_wot_register(service, password)
    }

    pub fn save_wot_register(
        &self,
        service: &mut AppService,
        reg: &humoco_wot::TrustRegister,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        let bytes = serde_json::to_vec(reg)
            .map_err(|e| AppFacadeError::Json(format!("Failed to serialize TrustRegister: {}", e)))?;
        service.save_encrypted_data(WOT_DATA_NAME, &bytes, password)
            .map_err(|e| AppFacadeError::Generic(format!("Failed to save TrustRegister: {}", e)))?;
        *self.wot.lock().unwrap() = Some(reg.clone());
        Ok(())
    }

    pub fn evaluate_voucher_trust_internal(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        creator_id: &str,
        guarantor_ids: &[&str],
    ) -> crate::models::FrontendVoucherTrustDto {
        use human_money_core::models::conflict::TrustStatus;
        use crate::models::FrontendVoucherTrustDto;

        let own_did_can = canonicalize_did(wallet.get_user_id()).ok();
        let creator_can = canonicalize_did(creator_id).ok();

        if let (Some(own), Some(creator)) = (&own_did_can, &creator_can) {
            if own == creator {
                return FrontendVoucherTrustDto {
                    score: 1.0,
                    trust_percent: 100,
                    is_own_voucher: true,
                    shortest_hops: Some(0),
                    ..Default::default()
                };
            }
        }

        let is_offender = matches!(wallet.check_reputation(creator_id), TrustStatus::KnownOffender(_));
        if is_offender {
            return FrontendVoucherTrustDto {
                score: 0.0,
                trust_percent: 0,
                is_own_voucher: false,
                blocked_by_offender: true,
                ..Default::default()
            };
        }

        let valid_guarantors: Vec<&str> = guarantor_ids.iter().copied()
            .filter(|gid| !matches!(wallet.check_reputation(gid), TrustStatus::KnownOffender(_)))
            .collect();

        let guarantor_canonical_set: std::collections::HashSet<String> = valid_guarantors.iter()
            .filter_map(|gid| canonicalize_did(gid).ok())
            .collect();

        let wot_guard = self.wot.lock().unwrap();
        let reg = match wot_guard.as_ref() {
            Some(r) => r,
            None => return FrontendVoucherTrustDto::default(),
        };

        let creator_canonical = creator_can.as_deref().unwrap_or(creator_id);
        let valid_guarantor_canonicals: Vec<String> = valid_guarantors.iter()
            .map(|gid| canonicalize_did(gid).unwrap_or_else(|_| gid.to_string()))
            .collect();
        let valid_guarantor_refs: Vec<&str> = valid_guarantor_canonicals.iter().map(|s| s.as_str()).collect();

        let now_secs = Utc::now().timestamp() as u64;
        let eval = match reg.evaluate_voucher(creator_canonical, &valid_guarantor_refs, false, Some(now_secs)) {
            Ok(e) => e,
            Err(_) => return FrontendVoucherTrustDto::default(),
        };

        let score = eval.score;
        let trust_percent = (score * 100.0).round().clamp(0.0, 100.0) as u8;
        let shortest_hops = eval.shortest_hops;
        let additional_paths_count = eval.paths.len().saturating_sub(1);

        let contacts_cache = self.contacts.lock().unwrap();
        let contacts_ref = contacts_cache.as_ref();

        let mut primary_bridge_did = None;
        let mut primary_bridge_name = None;
        let mut via_guarantor = false;
        let mut best_path_names = Vec::new();

        let resolve_name = |did: &str| -> Option<String> {
            lookup_contact_name(contacts_ref, did).or_else(|| {
                canonicalize_did(did)
                    .ok()
                    .and_then(|can| reg.provider_filters.get(&can))
                    .map(|p| p.payload.name.clone())
            })
        };

        if let Some(best_path) = eval.paths.first() {
            let hops = &best_path.hops;
            if hops.len() >= 2 {
                let hop1 = &hops[1];
                primary_bridge_did = Some(hop1.clone());
                primary_bridge_name = resolve_name(hop1);

                if hops.len() >= 3 {
                    let penultimate = &hops[hops.len() - 2];
                    if let Ok(pen_can) = canonicalize_did(penultimate) {
                        if guarantor_canonical_set.contains(&pen_can) {
                            via_guarantor = true;
                        }
                    }
                }
            }

            if hops.len() > 2 {
                for hop in &hops[1..hops.len() - 1] {
                    let name = resolve_name(hop).unwrap_or_else(|| truncate_did(hop));
                    best_path_names.push(name);
                }
            }
        }

        FrontendVoucherTrustDto {
            score,
            trust_percent,
            is_own_voucher: false,
            blocked_by_offender: false,
            shortest_hops,
            primary_bridge_did,
            primary_bridge_name,
            additional_paths_count,
            via_guarantor,
            best_path_names,
        }
    }

    pub fn evaluate_voucher_trust(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        creator_id: &str,
        signatures: &[human_money_core::models::voucher::VoucherSignature],
    ) -> crate::models::FrontendVoucherTrustDto {
        let guarantor_ids: Vec<&str> = signatures.iter()
            .filter(|s| s.role.eq_ignore_ascii_case("guarantor"))
            .map(|s| s.signer_id.as_str())
            .collect();
        self.evaluate_voucher_trust_internal(wallet, creator_id, &guarantor_ids)
    }

    pub fn evaluate_voucher_trust_preview(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        creator_id: &str,
        guarantor_ids: &[String],
    ) -> crate::models::FrontendVoucherTrustDto {
        let refs: Vec<&str> = guarantor_ids.iter().map(|s| s.as_str()).collect();
        self.evaluate_voucher_trust_internal(wallet, creator_id, &refs)
    }

    pub fn try_import_piggybacked_wot(
        &self,
        service: &mut AppService,
        wot_sync_json: &str,
        bundle_sender_id: &str,
        password: Option<&str>,
    ) -> Option<usize> {
        let pkg: humoco_wot::WotSyncPackage = match serde_json::from_str(wot_sync_json) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Malformed piggybacked WoT sync package: {}", e);
                return None;
            }
        };

        let direct_sender = canonicalize_did(bundle_sender_id).ok();
        let now_secs = Utc::now().timestamp() as u64;

        let mut reg = match self.get_or_load_wot_register(service, password) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Could not load WoT register for piggybacked import: {}", e);
                return None;
            }
        };

        let stats = match reg.import_sync_package(&pkg, direct_sender.as_deref(), now_secs) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to import piggybacked WoT package: {}", e);
                return None;
            }
        };

        if reg.own_list_dirty {
            let _ = self.ensure_wot_list_signed(service, &mut reg);
        }

        if let Err(e) = self.save_wot_register(service, &reg, password) {
            log::warn!("Failed to save WoT register after piggybacked import: {}", e);
        }

        Some(stats.imported + stats.updated + stats.merged)
    }

    pub fn try_import_piggybacked_contact_card(
        &self,
        service: &mut AppService,
        contact_card_json: Option<&str>,
        password: Option<&str>,
    ) {
        let json = match contact_card_json {
            Some(j) if !j.trim().is_empty() => j.trim(),
            _ => return,
        };

        let env: humoco_market::CardEnvelope = match serde_json::from_str(json) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Malformed piggybacked CardEnvelope: {}", e);
                return;
            }
        };

        let settings = self
            .get_cached_settings()
            .or_else(|_| self.load_settings(service, password))
            .unwrap_or_default();
        let max_store_hops = settings.max_store_hops.clamp(1, 3);
        let now = Utc::now().timestamp().max(0) as u64;

        let mut store = match service.load_encrypted_data(MARKET_CARD_STORE_KEY, password) {
            Ok(data) => serde_json::from_slice::<humoco_market::CardStore>(&data).unwrap_or_default(),
            Err(_) => humoco_market::CardStore::default(),
        };

        let outcome = service.with_wallet(|wallet| {
            let is_offender = |did: &str| -> bool {
                use human_money_core::models::conflict::TrustStatus;
                matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_))
            };
            humoco_market::accept_envelope(&mut store, &env, now, max_store_hops, &is_offender)
        });

        match outcome {
            Ok(humoco_market::AcceptOutcome::Accepted { did, replaced }) => {
                log::info!("Accepted piggybacked contact card for did: {}, replaced: {}", did, replaced);
                if let Ok(bytes) = serde_json::to_vec(&store) {
                    if let Err(e) = service.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password) {
                        log::warn!("Failed to save CardStore after accepting piggybacked card: {}", e);
                    }
                }
                if let Some(stored) = store.cards.values().find(|sc| sc.card.did == did) {
                    let card_drops: Vec<crate::models::FrontendDropDescriptor> =
                        stored.card.drops.iter().cloned().map(Into::into).collect();
                    self.update_contact_drops_from_card(service, &did, &card_drops, password);
                }
            }
            Ok(other) => {
                log::debug!("Piggybacked contact card outcome: {:?}", other);
            }
            Err(e) => {
                log::warn!("Error verifying reputation for piggybacked contact card: {}", e);
            }
        }
    }

    pub fn update_contact_drops_from_card(
        &self,
        service: &mut AppService,
        card_did: &str,
        card_drops: &[crate::models::FrontendDropDescriptor],
        password: Option<&str>,
    ) {
        if card_drops.is_empty() {
            return;
        }
        let can_card_did = match canonicalize_did(card_did) {
            Ok(d) => d,
            Err(_) => return,
        };

        let mut address_book = match service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
            Ok(data) => serde_json::from_slice::<crate::models::FrontendAddressBook>(&data).unwrap_or_default(),
            Err(_) => return,
        };

        let mut updated = false;
        for (k, contact) in address_book.contacts.iter_mut() {
            let is_match = k == card_did || canonicalize_did(k).ok().as_deref() == Some(&can_card_did);
            if is_match {
                let merged = merge_contact_drops(&contact.drops, card_drops);
                if contact.drops != merged {
                    contact.drops = merged;
                    updated = true;
                }
            }
        }

        if updated {
            if let Ok(bytes) = serde_json::to_vec(&address_book) {
                let _ = service.save_encrypted_data(CONTACTS_DATA_NAME, &bytes, password);
            }
            *self.contacts.lock().unwrap() = Some(address_book);
        }
    }
}

pub fn canonicalize_did(input: &str) -> Result<String, humoco_wot::WotError> {
    let trimmed = input.trim();
    let did_part = if let Some((_, did)) = trimmed.split_once('@') {
        did
    } else {
        trimmed
    };
    humoco_wot::canonicalize_did(did_part)
}

pub struct CoreIdentitySigner<'a> {
    pub identity: &'a human_money_core::models::profile::UserIdentity,
    pub canonical_did: String,
}

impl<'a> CoreIdentitySigner<'a> {
    pub fn new(identity: &'a human_money_core::models::profile::UserIdentity) -> Self {
        let canonical_did = canonicalize_did(&identity.user_id).unwrap_or_else(|_| {
            identity
                .user_id
                .split_once('@')
                .map(|(_, did)| did.to_string())
                .unwrap_or_else(|| identity.user_id.clone())
        });
        Self {
            identity,
            canonical_did,
        }
    }
}

impl<'a> humoco_wot::WotSigner for CoreIdentitySigner<'a> {
    fn did(&self) -> &str {
        &self.canonical_did
    }

    fn sign(&self, message: &[u8]) -> [u8; 64] {
        human_money_core::services::crypto::sign_ed25519(&self.identity.signing_key, message).to_bytes()
    }
}

fn enrich_contact_with_wot(contact: &mut FrontendContact, wot_opt: Option<&humoco_wot::TrustRegister>) {
    if let Some(reg) = wot_opt {
        if let Ok(can_did) = canonicalize_did(&contact.did) {
            contact.trust_level = reg.own_contacts.get(&can_did).map(|e| e.level);
            contact.desired_hops = reg.own_contacts.get(&can_did).map(|e| e.desired_hops);
            contact.has_peer_list = reg.raw_list_store.contains_key(&can_did);
            contact.peer_list_updated_at = reg.raw_list_store.get(&can_did).map(|l| l.iat);
        }
    }
}

fn lookup_contact_name(contacts_map: Option<&FrontendAddressBook>, target_did: &str) -> Option<String> {
    let book = contacts_map?;
    let can_target = canonicalize_did(target_did).ok()?;
    for (k, c) in &book.contacts {
        if k == target_did || canonicalize_did(k).ok().as_ref() == Some(&can_target) {
            let first = c.profile.first_name.as_deref().unwrap_or("").trim();
            let last = c.profile.last_name.as_deref().unwrap_or("").trim();
            let name = match (first.is_empty(), last.is_empty()) {
                (false, false) => format!("{} {}", first, last),
                (false, true) => first.to_string(),
                (true, false) => last.to_string(),
                (true, true) => c.profile.organization.as_deref().unwrap_or("").trim().to_string(),
            };
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn truncate_did(did: &str) -> String {
    if did.len() > 16 {
        format!("{}…{}", &did[..8], &did[did.len() - 4..])
    } else {
        did.to_string()
    }
}

pub fn merge_contact_drops(
    existing: &[crate::models::FrontendDropDescriptor],
    card_drops: &[crate::models::FrontendDropDescriptor],
) -> Vec<crate::models::FrontendDropDescriptor> {
    let mut result = Vec::with_capacity(3);
    let mut seen = std::collections::HashSet::new();

    // Newest card drops take precedence, preserving priority order
    for d in card_drops {
        if seen.insert(d.read_url.clone()) {
            result.push(d.clone());
            if result.len() == 3 {
                return result;
            }
        }
    }
    for d in existing {
        if seen.insert(d.read_url.clone()) {
            result.push(d.clone());
            if result.len() == 3 {
                return result;
            }
        }
    }
    result
}
