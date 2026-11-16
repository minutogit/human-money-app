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
pub const MARKET_OWN_PROFILE_KEY: &str = "market_own_profile";
pub const MARKET_SYNC_ADMIN_KEY: &str = "market_sync_admin";
pub const MARKET_SYNC_KEY: &str = "market_sync_state";
pub const BACKUP_SYNC_STATE_KEY: &str = "backup_sync_state";
pub const MARKET_POST_STATE_KEY: &str = "market_post_state";
pub const RECOVERY_OWNER_STATE_KEY: &str = "recovery_owner_state";
pub const RECOVERY_GUARDIAN_VAULT_KEY: &str = "recovery_guardian_vault";
pub const BUNDLE_RETENTION_DAYS: i64 = 90;

pub fn own_media_storage_key(image_token: &str) -> String {
    format!("market_own_media_{}", image_token)
}

pub fn cached_media_storage_key(blob_id: &str) -> String {
    format!("market_media_cache_{}", blob_id)
}

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

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StoredSyncAdmin {
    pub descriptor: humoco_market::DropDescriptor,
    #[serde(default)]
    pub sync_index: u32,
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
    pub last_members_hash: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<crate::sync_client::SyncRevocationNotice>,
}

fn default_max_total_storage_mb() -> usize {
    256
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredSyncState {
    pub last_upload_at: Option<u64>,
    #[serde(default)]
    pub last_poll_at: Option<u64>,
    pub last_readers_hash: Option<String>,
    #[serde(default)]
    pub etags_by_read_url: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub last_modified_by_read_url: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub slot_etags: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub channel_max_bytes_by_write_url: std::collections::BTreeMap<String, usize>,
    #[serde(default)]
    pub card_base_uploaded_at_by_write_url: std::collections::BTreeMap<String, u64>,
    #[serde(default)]
    pub card_base_size_by_write_url: std::collections::BTreeMap<String, usize>,
    #[serde(default)]
    pub card_base_versions_by_write_url:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, u64>>,
    #[serde(default)]
    pub card_delta_active_by_write_url: std::collections::BTreeMap<String, bool>,
    #[serde(default)]
    pub card_last_uploaded_versions_by_write_url:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, u64>>,
    #[serde(default)]
    pub last_revocation_check_at: std::collections::BTreeMap<String, u64>,
    #[serde(default)]
    pub sync_sources_by_did: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub uploaded_media_blobs: std::collections::HashSet<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupMirrorState {
    pub base_manifest_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_slot_etag: Option<String>,
    pub base_uploaded_at: u64,
    pub base_padded_bytes: usize,
    pub delta_active: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredBackupSyncState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_manifest_bytes: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_manifest_sha256: Option<String>,
    #[serde(default)]
    pub mirrors_by_write_url: std::collections::BTreeMap<String, BackupMirrorState>,
}

pub struct AppState {
    pub service: Mutex<AppService>,
    pub history: Mutex<Option<Vec<TransactionRecord>>>,
    pub events: Mutex<Option<Vec<human_money_core::models::wallet_event::WalletEvent>>>,
    pub contacts: Mutex<Option<crate::models::FrontendAddressBook>>,
    pub settings: Mutex<Option<AppSettings>>,
    pub(crate) regional_views: Mutex<Option<crate::checkout::RegionalViewCache>>,
    pub wot: Mutex<Option<humoco_wot::TrustRegister>>,
    pub media_negative_cache: Mutex<std::collections::HashMap<String, u64>>,
}

impl AppState {
    pub fn new(service: AppService) -> Self {
        Self {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(None),
            regional_views: Mutex::new(None),
            wot: Mutex::new(None),
            media_negative_cache: Mutex::new(std::collections::HashMap::new()),
        }
    }

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
                let mut contacts: Vec<FrontendContact> = address_book.contacts.into_values().collect();
                for c in contacts.iter_mut() {
                    enrich_contact_with_wot(c, wot_guard.as_ref());
                    c.syncs.truncate(2);
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

        let is_existing = address_book.contacts.contains_key(&contact.did) || !keys_to_remove.is_empty();

        let mut old_contact_syncs: Vec<crate::models::FrontendSyncDescriptor> = address_book
            .contacts
            .get(&contact.did)
            .map(|c| c.syncs.clone())
            .unwrap_or_default();

        if let Some(old) = address_book.contacts.get(&contact.did) {
            if contact.added_at.is_empty() {
                contact.added_at = old.added_at.clone();
            }
        }

        for k in keys_to_remove {
            if let Some(old) = address_book.contacts.remove(&k) {
                if old_contact_syncs.is_empty() {
                    old_contact_syncs = old.syncs;
                }
                if contact.added_at.is_empty() {
                    contact.added_at = old.added_at;
                }
            }
        }

        // Authoritative sync endpoints:
        // For existing contacts, contact.syncs passed from caller/UI is authoritative.
        // We do NOT revive old sync endpoints.
        // For new contacts, if contact.syncs is empty, populate from CardStore if a card exists.
        if is_existing {
            contact.syncs = merge_contact_syncs(&[], &contact.syncs);
            contact.syncs.truncate(2);
        } else {
            let card_syncs: Vec<crate::models::FrontendSyncDescriptor> = service
                .load_encrypted_data(MARKET_CARD_STORE_KEY, password)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<humoco_market::CardStore>(&bytes).ok())
                .and_then(|store| {
                    store.cards.values()
                        .find(|sc| canonicalize_did(&sc.card.payload.did).ok().as_deref() == Some(&can_did))
                        .map(|sc| sc.card.payload.drops.iter().cloned().map(Into::into).collect())
                })
                .unwrap_or_default();

            let mut all_syncs = Vec::new();
            all_syncs.extend(contact.syncs);
            all_syncs.extend(card_syncs);
            contact.syncs = merge_contact_syncs(&[], &all_syncs);
            contact.syncs.truncate(2);
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

        let removed_endpoints: Vec<crate::models::FrontendSyncDescriptor> = old_contact_syncs
            .into_iter()
            .filter(|old_ep| {
                let old_r = old_ep.read_url.trim().trim_end_matches('/').to_lowercase();
                let old_w = old_ep.write_url.trim().trim_end_matches('/').to_lowercase();
                !contact.syncs.iter().any(|new_ep| {
                    let new_r = new_ep.read_url.trim().trim_end_matches('/').to_lowercase();
                    let new_w = new_ep.write_url.trim().trim_end_matches('/').to_lowercase();
                    (!old_r.is_empty() && (new_r == old_r || new_w == old_r))
                        || (!old_w.is_empty() && (new_r == old_w || new_w == old_w))
                })
            })
            .collect();

        address_book.contacts.insert(contact.did.clone(), contact);

        let data = serde_json::to_vec(&address_book)
            .map_err(|e| AppFacadeError::Json(format!("Failed to serialize address book: {}", e)))?;

        service.save_encrypted_data(CONTACTS_DATA_NAME, &data, password)
            .map_err(|e| AppFacadeError::Generic(format!("Failed to save address book: {}", e)))?;

        *self.contacts.lock().unwrap() = Some(address_book.clone());
        self.prune_orphaned_market_selected_syncs(service, &address_book, &removed_endpoints, password);
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

        let mut removed_endpoints = Vec::new();
        let mut changed = false;
        for k in keys_to_remove {
            if let Some(removed_contact) = address_book.contacts.remove(&k) {
                removed_endpoints.extend(removed_contact.syncs);
                changed = true;
            }
        }

        if changed {
            let data = serde_json::to_vec(&address_book)
                .map_err(|e| AppFacadeError::Json(format!("Failed to serialize address book: {}", e)))?;

            service.save_encrypted_data(CONTACTS_DATA_NAME, &data, password)
                .map_err(|e| AppFacadeError::Generic(format!("Failed to save address book: {}", e)))?;

            *self.contacts.lock().unwrap() = Some(address_book.clone());
            self.prune_orphaned_market_selected_syncs(service, &address_book, &removed_endpoints, password);
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

    fn initialize_profile_session_inner(
        &self,
        service: &mut AppService,
        credential: &str,
        is_secondary: bool,
    ) -> Result<(), AppFacadeError> {
        info!("Initializing profile session and member caches...");

        // 1. Load or create settings
        let settings = self.load_settings(service, Some(credential))?;

        if let Some(policy) = crate::checkout::configured_policy(&settings)
            .map_err(|e| AppFacadeError::Generic(e.message))?
        {
            service
                .configure_regional_mesh_authority(policy, Some(credential))
                .map_err(|e| AppFacadeError::Generic(e.to_string()))?;
        }

        // 2. Start session timer if configured
        if settings.session_timeout_seconds > 0 {
            if is_secondary {
                service.unlock_secondary_session(credential, settings.session_timeout_seconds)?;
            } else {
                service.unlock_session(credential, settings.session_timeout_seconds)?;
            }
        }

        // 3. Load transaction history and clean up expired bundle data
        let mut history = self.load_history_from_disk(service, Some(credential))?;
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
            self.save_history(service, history, Some(credential))?;
        }

        // 4. Pre-cache the last 50 wallet events
        self.refresh_events_cache(service, Some(credential))?;

        // 5. Load the address book
        let _ = self.load_contacts(service, Some(credential));

        // 6. Load the Web of Trust register
        self.load_wot_register(service, Some(credential))?;

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

        reg.policy.own_list_max_hops = reg.policy.own_list_max_hops.clamp(1, 3);
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
        let max_stored_cards = settings.max_stored_cards.clamp(100, 10_000);
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
            let own_coords = wallet
                .profile
                .coordinates
                .as_deref()
                .and_then(humoco_market::parse_coordinates_pair);
            humoco_market::accept_envelope(
                &mut store,
                &env,
                now,
                max_stored_cards,
                own_coords,
                &is_offender,
            )
        });

        match outcome {
            Ok(humoco_market::AcceptOutcome::Accepted { did, replaced }) => {
                log::info!("Accepted piggybacked contact card for did: {}, replaced: {}", did, replaced);
                if let Ok(bytes) = serde_json::to_vec(&store) {
                    if let Err(e) = service.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password) {
                        log::warn!("Failed to save CardStore after accepting piggybacked card: {}", e);
                    }
                }
                if let Some(stored) = store.cards.values().find(|sc| sc.card.payload.did == did) {
                    let card_syncs: Vec<crate::models::FrontendSyncDescriptor> =
                        stored.card.payload.drops.iter().cloned().map(Into::into).collect();
                    self.update_contact_syncs_from_card(service, &did, &card_syncs, password);
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

    pub fn update_contact_syncs_from_card(
        &self,
        service: &mut AppService,
        card_did: &str,
        card_syncs: &[crate::models::FrontendSyncDescriptor],
        password: Option<&str>,
    ) {
        if card_syncs.is_empty() {
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
                let merged = merge_contact_syncs(&contact.syncs, card_syncs);
                if contact.syncs != merged {
                    contact.syncs = merged;
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

    pub fn record_contact_sync_health_batch(
        &self,
        service: &mut AppService,
        results: &[(String, bool)],
        now_secs: u64,
        password: Option<&str>,
    ) -> bool {
        if results.is_empty() {
            return false;
        }

        let mut address_book = match service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
            Ok(data) => serde_json::from_slice::<crate::models::FrontendAddressBook>(&data).unwrap_or_default(),
            Err(_) => return false,
        };

        let settings_res = self.load_settings(service, password);
        let settings_opt = settings_res.ok();

        let mut updated = false;
        for (url, success) in results {
            let norm_target = url.trim().trim_end_matches('/').to_lowercase();
            if norm_target.is_empty() {
                continue;
            }
            for contact in address_book.contacts.values_mut() {
                for sync_ep in &mut contact.syncs {
                    let norm_read = sync_ep.read_url.trim().trim_end_matches('/').to_lowercase();
                    let norm_write = sync_ep.write_url.trim().trim_end_matches('/').to_lowercase();
                    if norm_read == norm_target || norm_write == norm_target {
                        if sync_ep.status == "deprecating" && sync_ep.sunset_at.is_some_and(|eff| now_secs >= eff) {
                            sync_ep.status = "revoked".to_string();
                        }
                        let is_revocation_state = sync_ep.status == "deprecating" || sync_ep.status == "revoked";
                        if *success {
                            sync_ep.last_seen_at = Some(now_secs);
                            sync_ep.fail_count = 0;
                            if !is_revocation_state {
                                if sync_ep.status == "active" {
                                    // remains active
                                } else if sync_ep.status == "unreachable" {
                                    let is_selected = settings_opt.as_ref().map(|s| {
                                        s.market_selected_syncs.iter().any(|sel| {
                                            let sr = sel.read_url.trim().trim_end_matches('/').to_lowercase();
                                            let sw = sel.write_url.trim().trim_end_matches('/').to_lowercase();
                                            (!sr.is_empty() && (sr == norm_read || sr == norm_write))
                                                || (!sw.is_empty() && (sw == norm_read || sw == norm_write))
                                        })
                                    }).unwrap_or(false);
                                    if is_selected {
                                        sync_ep.status = "active".to_string();
                                    } else {
                                        sync_ep.status = "unverified".to_string();
                                    }
                                } else if sync_ep.status.is_empty() {
                                    sync_ep.status = "unverified".to_string();
                                }
                                // pending_auth and unverified remain unchanged on list_index read success!
                            }
                        } else {
                            sync_ep.last_error_at = Some(now_secs);
                            sync_ep.fail_count = sync_ep.fail_count.saturating_add(1);
                            if !is_revocation_state {
                                sync_ep.status = "unreachable".to_string();
                            }
                        }
                        updated = true;
                    }
                }
            }
        }

        if updated {
            if let Ok(bytes) = serde_json::to_vec(&address_book) {
                let _ = service.save_encrypted_data(CONTACTS_DATA_NAME, &bytes, password);
            }
            *self.contacts.lock().unwrap() = Some(address_book);
        }

        if let Some(mut settings) = settings_opt {
            let mut settings_updated = false;
            for (url, success) in results {
                let norm_target = url.trim().trim_end_matches('/').to_lowercase();
                if norm_target.is_empty() {
                    continue;
                }
                for sel in &mut settings.market_selected_syncs {
                    let norm_read = sel.read_url.trim().trim_end_matches('/').to_lowercase();
                    let norm_write = sel.write_url.trim().trim_end_matches('/').to_lowercase();
                    if norm_read == norm_target || norm_write == norm_target {
                        if sel.status == "deprecating" && sel.sunset_at.is_some_and(|eff| now_secs >= eff) {
                            sel.status = "revoked".to_string();
                        }
                        let is_revocation_state = sel.status == "deprecating" || sel.status == "revoked";
                        if *success {
                            sel.last_seen_at = Some(now_secs);
                            sel.fail_count = 0;
                            if !is_revocation_state {
                                sel.status = "active".to_string();
                            }
                        } else {
                            sel.last_error_at = Some(now_secs);
                            sel.fail_count = sel.fail_count.saturating_add(1);
                            if !is_revocation_state {
                                sel.status = "unreachable".to_string();
                            }
                        }
                        settings_updated = true;
                    }
                }
            }
            if settings_updated {
                let _ = self.save_settings(service, settings, password);
            }
        }
        updated
    }

    pub fn prune_orphaned_market_selected_syncs(
        &self,
        service: &mut AppService,
        address_book: &FrontendAddressBook,
        removed_endpoints: &[crate::models::FrontendSyncDescriptor],
        password: Option<&str>,
    ) {
        if removed_endpoints.is_empty() {
            return;
        }

        let mut settings = match self.load_settings(service, password) {
            Ok(s) => s,
            Err(_) => return,
        };

        if settings.market_selected_syncs.is_empty() {
            return;
        }

        let admin_opt: Option<StoredSyncAdmin> = service
            .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());

        let initial_count = settings.market_selected_syncs.len();
        settings.market_selected_syncs.retain(|sel| {
            let sr = sel.read_url.trim().trim_end_matches('/').to_lowercase();
            let sw = sel.write_url.trim().trim_end_matches('/').to_lowercase();
            if sr.is_empty() && sw.is_empty() {
                return false;
            }

            let was_removed = removed_endpoints.iter().any(|rem| {
                let rr = rem.read_url.trim().trim_end_matches('/').to_lowercase();
                let rw = rem.write_url.trim().trim_end_matches('/').to_lowercase();
                (!rr.is_empty() && (rr == sr || rr == sw)) || (!rw.is_empty() && (rw == sr || rw == sw))
            });

            if !was_removed {
                return true;
            }

            // 1. Matches StoredSyncAdmin?
            let matches_admin = admin_opt.as_ref().map(|a| {
                let ar = a.descriptor.read_url.trim().trim_end_matches('/').to_lowercase();
                let aw = a.descriptor.write_url.trim().trim_end_matches('/').to_lowercase();
                (!ar.is_empty() && (ar == sr || ar == sw)) || (!aw.is_empty() && (aw == sr || aw == sw))
            }).unwrap_or(false);

            if matches_admin {
                return true;
            }

            // 2. Matches any remaining contact's syncs?
            let matches_contact = address_book.contacts.values().any(|c| {
                c.syncs.iter().any(|ep| {
                    let er = ep.read_url.trim().trim_end_matches('/').to_lowercase();
                    let ew = ep.write_url.trim().trim_end_matches('/').to_lowercase();
                    (!er.is_empty() && (er == sr || er == sw)) || (!ew.is_empty() && (ew == sr || ew == sw))
                })
            });

            if matches_contact {
                return true;
            }

            // 3. Matches any settings.public_syncs?
            let matches_public = settings.public_syncs.iter().any(|ps| {
                let pr = ps.read_url.trim().trim_end_matches('/').to_lowercase();
                let pw = ps.write_url.trim().trim_end_matches('/').to_lowercase();
                (!pr.is_empty() && (pr == sr || pr == sw)) || (!pw.is_empty() && (pw == sr || pw == sw))
            });

            matches_public
        });

        if settings.market_selected_syncs.len() != initial_count {
            let updated_syncs = settings.market_selected_syncs.clone();
            let _ = self.save_settings(service, settings, password);
            crate::commands::market::sync_own_card_drops_on_settings_change(service, &updated_syncs, password);
        }
    }

    pub fn update_contact_sync_health(
        &self,
        service: &mut AppService,
        endpoint_url: &str,
        success: bool,
        now_secs: u64,
        password: Option<&str>,
    ) -> bool {
        self.record_contact_sync_health_batch(service, &[(endpoint_url.to_string(), success)], now_secs, password)
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

pub fn sync_endpoint_key(d: &crate::models::FrontendSyncDescriptor) -> String {
    let w = d.write_url.trim().trim_end_matches('/');
    if !w.is_empty() {
        w.to_string()
    } else {
        d.read_url.trim().trim_end_matches('/').to_string()
    }
}

pub fn merge_contact_syncs(
    existing: &[crate::models::FrontendSyncDescriptor],
    card_syncs: &[crate::models::FrontendSyncDescriptor],
) -> Vec<crate::models::FrontendSyncDescriptor> {
    let mut result: Vec<crate::models::FrontendSyncDescriptor> = Vec::with_capacity(2);

    let merge_entry = |res: &mut Vec<crate::models::FrontendSyncDescriptor>, d: &crate::models::FrontendSyncDescriptor| {
        let w = d.write_url.trim().trim_end_matches('/');
        let r = d.read_url.trim().trim_end_matches('/');
        if w.is_empty() && r.is_empty() {
            return;
        }

        if let Some(target) = res.iter_mut().find(|existing_entry| {
            let ew = existing_entry.write_url.trim().trim_end_matches('/');
            let er = existing_entry.read_url.trim().trim_end_matches('/');
            (!w.is_empty() && !ew.is_empty() && w == ew)
                || (!r.is_empty() && !er.is_empty() && r == er)
        }) {
            if target.read_url.is_empty() && !d.read_url.is_empty() {
                target.read_url = d.read_url.clone();
            }
            if target.drop_pub.is_empty() && !d.drop_pub.is_empty() {
                target.drop_pub = d.drop_pub.clone();
            }
            if target.write_url.is_empty() && !d.write_url.is_empty() {
                target.write_url = d.write_url.clone();
            }
            if target.slot_id.is_none() && d.slot_id.is_some() {
                target.slot_id = d.slot_id.clone();
            }
            if target.status == "unverified" && d.status != "unverified" {
                target.status = d.status.clone();
                target.last_probed_at = d.last_probed_at;
                target.last_seen_at = d.last_seen_at;
                target.last_error_at = d.last_error_at;
                target.fail_count = d.fail_count;
            }
        } else if res.len() < 2 {
            res.push(d.clone());
        }
    };

    // Newest card sync endpoints take precedence, preserving priority order
    for d in card_syncs {
        merge_entry(&mut result, d);
    }
    for d in existing {
        merge_entry(&mut result, d);
    }

    result
}

pub fn apply_revocation_notice_to_descriptor(
    desc: &mut crate::models::FrontendSyncDescriptor,
    notice: &crate::sync_client::SyncRevocationNotice,
    now_secs: u64,
) {
    desc.status = if now_secs >= notice.effective_at {
        "revoked".to_string()
    } else {
        "deprecating".to_string()
    };
    desc.sunset_at = Some(notice.effective_at);
    desc.revocation_reason = notice.reason.clone();
    desc.replacement = notice.replacement.clone().map(Into::into);
}

pub fn apply_revocation_notice_to_public_sync(
    ps: &mut crate::settings::StoredPublicSync,
    notice: &crate::sync_client::SyncRevocationNotice,
    now_secs: u64,
) {
    ps.status = if now_secs >= notice.effective_at {
        "revoked".to_string()
    } else {
        "deprecating".to_string()
    };
    ps.sunset_at = Some(notice.effective_at);
    ps.revocation_reason = notice.reason.clone();
    ps.replacement = notice.replacement.clone().map(Into::into);
}

pub fn clear_revocation_on_descriptor(
    desc: &mut crate::models::FrontendSyncDescriptor,
    fallback_status: &str,
) -> bool {
    if desc.sunset_at.is_some()
        || desc.revocation_reason.is_some()
        || desc.replacement.is_some()
        || desc.status == "deprecating"
        || desc.status == "revoked"
    {
        if desc.status == "deprecating" || desc.status == "revoked" {
            desc.status = fallback_status.to_string();
        }
        desc.sunset_at = None;
        desc.revocation_reason = None;
        desc.replacement = None;
        true
    } else {
        false
    }
}

pub fn clear_revocation_on_public_sync(
    ps: &mut crate::settings::StoredPublicSync,
    fallback_status: &str,
) -> bool {
    if ps.sunset_at.is_some()
        || ps.revocation_reason.is_some()
        || ps.replacement.is_some()
        || ps.status == "deprecating"
        || ps.status == "revoked"
    {
        if ps.status == "deprecating" || ps.status == "revoked" {
            ps.status = fallback_status.to_string();
        }
        ps.sunset_at = None;
        ps.revocation_reason = None;
        ps.replacement = None;
        true
    } else {
        false
    }
}

fn norm_sync_url(u: &str) -> String {
    u.trim().trim_end_matches('/').to_lowercase()
}

pub fn apply_revocation_notice_to_address_book(
    address_book: &mut crate::models::FrontendAddressBook,
    old_read_url: &str,
    notice: &crate::sync_client::SyncRevocationNotice,
    now_secs: u64,
) -> bool {
    let old_norm = norm_sync_url(old_read_url);
    let notice_pub = notice.drop_pub.trim().to_lowercase();
    let is_revoked_now = now_secs >= notice.effective_at;
    let mut updated = false;

    for contact in address_book.contacts.values_mut() {
        let syncs_len = contact.syncs.len();
        let existing_endpoints: Vec<(String, String)> = contact
            .syncs
            .iter()
            .map(|o| (norm_sync_url(&o.read_url), norm_sync_url(&o.write_url)))
            .collect();
        let mut idx_to_replace: Option<(usize, crate::models::FrontendSyncDescriptor)> = None;
        let mut idx_to_append: Option<crate::models::FrontendSyncDescriptor> = None;

        for (idx, ep) in contact.syncs.iter_mut().enumerate() {
            if norm_sync_url(&ep.read_url) != old_norm {
                continue;
            }
            if !ep.drop_pub.trim().eq_ignore_ascii_case(&notice_pub) {
                continue;
            }

            let prev_status = ep.status.clone();
            let prev_sunset = ep.sunset_at;
            let prev_reason = ep.revocation_reason.clone();
            let prev_rep = ep.replacement.clone();

            apply_revocation_notice_to_descriptor(ep, notice, now_secs);
            if ep.status != prev_status
                || ep.sunset_at != prev_sunset
                || ep.revocation_reason != prev_reason
                || ep.replacement != prev_rep
            {
                updated = true;
            }

            if let Some(ref rep) = notice.replacement {
                let rep_r = norm_sync_url(&rep.read_url);
                let rep_w = norm_sync_url(&rep.write_url);
                let ep_r = norm_sync_url(&ep.read_url);
                let ep_w = norm_sync_url(&ep.write_url);

                // Anti-loop: ignore self-redirects or targets already present on this contact
                let is_self_redirect = rep_r == ep_r || (!rep_w.is_empty() && rep_w == ep_w);
                let already_present = existing_endpoints
                    .iter()
                    .any(|(or, ow)| *or == rep_r || (!rep_w.is_empty() && *ow == rep_w));

                if !is_self_redirect && !already_present {
                    let new_desc = crate::models::FrontendSyncDescriptor {
                        write_url: rep.write_url.clone(),
                        read_url: rep.read_url.clone(),
                        slot_id: None, // Force index resolution on new server until contact's new SdCard arrives
                        drop_pub: rep.drop_pub.clone(),
                        status: "unverified".to_string(),
                        ..Default::default()
                    };
                    if is_revoked_now {
                        idx_to_replace = Some((idx, new_desc));
                    } else if syncs_len < 2 {
                        idx_to_append = Some(new_desc);
                    }
                }
            }
        }

        if let Some((idx, new_desc)) = idx_to_replace {
            contact.syncs[idx] = new_desc;
            updated = true;
        } else if let Some(new_desc) = idx_to_append {
            if contact.syncs.len() < 2 {
                contact.syncs.push(new_desc);
                updated = true;
            }
        }
    }

    updated
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use crate::models::{FrontendContact, FrontendUserProfile, FrontendSyncDescriptor};

    #[test]
    fn test_save_contact_authoritative_syncs_deletion() {
        let temp_dir = std::env::temp_dir().join(format!("humoco_state_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::commands::sync::tests::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::commands::sync::tests::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::commands::sync::tests::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::commands::sync::tests::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let state = crate::commands::sync::tests::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let sync1 = FrontendSyncDescriptor {
            read_url: "https://sync1.example.org/sync.php".to_string(),
            write_url: "https://sync1.example.org/sync.php".to_string(),
            drop_pub: "pubkey1".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let sync2 = FrontendSyncDescriptor {
            read_url: "https://sync2.example.org/sync.php".to_string(),
            write_url: "https://sync2.example.org/sync.php".to_string(),
            drop_pub: "pubkey2".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        // 1. Initial contact with 2 sync endpoints
        let initial_contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![sync1.clone(), sync2.clone()],
        };

        state.save_contact(&mut service, initial_contact, Some(password)).unwrap();

        let loaded = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = loaded.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs.len(), 2);

        // 2. Update contact with sync2 removed (only sync1 remains)
        let updated_one_sync = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: String::new(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![sync1.clone()], // sync2 removed!
        };

        state.save_contact(&mut service, updated_one_sync, Some(password)).unwrap();

        // 3. Verify sync2 is deleted and NOT resurrected from old syncs
        let loaded2 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob2 = loaded2.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob2.syncs.len(), 1, "Deleted sync2 must not be resurrected");
        assert_eq!(bob2.syncs[0].read_url, "https://sync1.example.org/sync.php");

        // 4. Update contact with empty syncs (all syncs deleted)
        let updated_zero_syncs = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: String::new(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![], // All removed!
        };

        state.save_contact(&mut service, updated_zero_syncs, Some(password)).unwrap();

        // 5. Verify all syncs deleted
        let loaded3 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob3 = loaded3.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob3.syncs.len(), 0, "All syncs must be removed authoritatively");
    }

    #[test]
    fn test_record_contact_sync_health_batch() {
        let temp_dir = std::env::temp_dir().join(format!("humoco_health_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::commands::sync::tests::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::commands::sync::tests::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::commands::sync::tests::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::commands::sync::tests::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let state = crate::commands::sync::tests::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let sync_desc = FrontendSyncDescriptor {
            read_url: "https://sync-endpoint.org/sync.php".to_string(),
            write_url: "https://sync-endpoint.org/sync.php".to_string(),
            drop_pub: "pub123".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        let contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![sync_desc],
        };
        state.save_contact(&mut service, contact, Some(password)).unwrap();

        // 1. Success update preserves "unverified" (does NOT overwrite with "active")
        let updated = state.update_contact_sync_health(&mut service, "https://sync-endpoint.org/sync.php", true, 1000, Some(password));
        assert!(updated);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_seen_at, Some(1000));
        assert_eq!(bob.syncs[0].fail_count, 0);
        assert_eq!(bob.syncs[0].status, "unverified", "unverified status must not be overwritten with active on static read success");

        // 1b. Test that "pending_auth" is also preserved on read success
        let mut bob_pending = bob.clone();
        bob_pending.syncs[0].status = "pending_auth".to_string();
        state.save_contact(&mut service, bob_pending, Some(password)).unwrap();
        let updated_pending = state.update_contact_sync_health(&mut service, "https://sync-endpoint.org/sync.php", true, 1050, Some(password));
        assert!(updated_pending);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].status, "pending_auth", "pending_auth status must not be overwritten with active on static read success");

        // 2. Failure update (sets status to unreachable and increments fail_count)
        let updated2 = state.update_contact_sync_health(&mut service, "https://sync-endpoint.org/sync.php", false, 1100, Some(password));
        assert!(updated2);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_error_at, Some(1100));
        assert_eq!(bob.syncs[0].fail_count, 1);
        assert_eq!(bob.syncs[0].status, "unreachable");

        // 3. Second failure
        let updated3 = state.update_contact_sync_health(&mut service, "https://sync-endpoint.org/sync.php", false, 1200, Some(password));
        assert!(updated3);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_error_at, Some(1200));
        assert_eq!(bob.syncs[0].fail_count, 2);
        assert_eq!(bob.syncs[0].status, "unreachable");

        // 4. Recovery success resets fail_count; since not in market_selected_syncs, transitions to "unverified"
        let updated4 = state.update_contact_sync_health(&mut service, "https://sync-endpoint.org/sync.php", true, 1300, Some(password));
        assert!(updated4);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_seen_at, Some(1300));
        assert_eq!(bob.syncs[0].fail_count, 0);
        assert_eq!(bob.syncs[0].status, "unverified", "unreachable contact sync transitions to unverified on read success when not in market_selected_syncs");

        // 5. If sync is in market_selected_syncs, unreachable transitions to "active" on recovery
        let mut settings = state.load_settings(&mut service, Some(password)).unwrap();
        settings.market_selected_syncs = vec![FrontendSyncDescriptor {
            read_url: "https://sync-endpoint.org/sync.php".to_string(),
            write_url: "https://sync-endpoint.org/sync.php".to_string(),
            drop_pub: "pub123".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        }];
        state.save_settings(&mut service, settings, Some(password)).unwrap();

        // Mark contact as unreachable again
        let _ = state.update_contact_sync_health(&mut service, "https://sync-endpoint.org/sync.php", false, 1400, Some(password));
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(contacts.iter().find(|c| c.did == bob_did).unwrap().syncs[0].status, "unreachable");

        // Now recover: should transition to "active" because it's in market_selected_syncs
        let _ = state.update_contact_sync_health(&mut service, "https://sync-endpoint.org/sync.php", true, 1500, Some(password));
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(contacts.iter().find(|c| c.did == bob_did).unwrap().syncs[0].status, "active");
    }

    #[test]
    fn test_prune_orphaned_market_selected_syncs() {
        let temp_dir = std::env::temp_dir().join(format!("humoco_prune_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::commands::sync::tests::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::commands::sync::tests::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::commands::sync::tests::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::commands::sync::tests::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let state = crate::commands::sync::tests::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let bob_sync = FrontendSyncDescriptor {
            read_url: "https://bob-sync.org/sync.php".to_string(),
            write_url: "https://bob-sync.org/sync.php".to_string(),
            drop_pub: "pub_bob".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        let contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![bob_sync.clone()],
        };
        state.save_contact(&mut service, contact.clone(), Some(password)).unwrap();

        // Put Bob's sync into settings.market_selected_syncs
        let mut settings = state.load_settings(&mut service, Some(password)).unwrap();
        settings.market_selected_syncs = vec![bob_sync];
        state.save_settings(&mut service, settings, Some(password)).unwrap();

        // 1. Updating contact to remove sync should prune it from settings.market_selected_syncs
        let mut updated_contact = contact.clone();
        updated_contact.syncs = vec![];
        state.save_contact(&mut service, updated_contact, Some(password)).unwrap();

        let s_after = state.load_settings(&mut service, Some(password)).unwrap();
        assert_eq!(s_after.market_selected_syncs.len(), 0, "Orphaned contact sync must be pruned from market_selected_syncs on save_contact");

        // 2. Add contact back with sync, add to market_selected_syncs, then delete_contact
        state.save_contact(&mut service, contact, Some(password)).unwrap();
        let mut settings2 = state.load_settings(&mut service, Some(password)).unwrap();
        settings2.market_selected_syncs = vec![FrontendSyncDescriptor {
            read_url: "https://bob-sync.org/sync.php".to_string(),
            write_url: "https://bob-sync.org/sync.php".to_string(),
            drop_pub: "pub_bob".to_string(),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        }];
        state.save_settings(&mut service, settings2, Some(password)).unwrap();

        state.delete_contact(&mut service, &bob_did, Some(password)).unwrap();
        let s_after_del = state.load_settings(&mut service, Some(password)).unwrap();
        assert_eq!(s_after_del.market_selected_syncs.len(), 0, "Orphaned contact sync must be pruned from market_selected_syncs on delete_contact");
    }

    #[test]
    fn test_merge_contact_syncs_unprobed_distinct_write_urls() {
        let ep1 = FrontendSyncDescriptor {
            write_url: "https://sync-a.example.com/sync.php".to_string(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let ep2 = FrontendSyncDescriptor {
            write_url: "https://sync-b.example.com/sync.php".to_string(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        // Merge two unprobed syncs with distinct write URLs: neither should be dropped
        let merged = super::merge_contact_syncs(std::slice::from_ref(&ep1), std::slice::from_ref(&ep2));
        assert_eq!(merged.len(), 2, "Both distinct unprobed syncs must be kept");
        assert_eq!(merged[0].write_url, "https://sync-b.example.com/sync.php");
        assert_eq!(merged[1].write_url, "https://sync-a.example.com/sync.php");

        // Merge populated card sync into an existing unprobed sync with same write_url
        let populated_card = FrontendSyncDescriptor {
            write_url: "https://sync-a.example.com/sync.php/".to_string(),
            read_url: "https://sync-a.example.com/read".to_string(),
            slot_id: Some("slot123".to_string()),
            drop_pub: "pubkey_a".to_string(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let merged2 = super::merge_contact_syncs(&[ep1], &[populated_card]);
        assert_eq!(merged2.len(), 1, "Matching write_url should be merged into single entry");
        assert_eq!(merged2[0].read_url, "https://sync-a.example.com/read");
        assert_eq!(merged2[0].drop_pub, "pubkey_a");
        assert_eq!(merged2[0].slot_id, Some("slot123".to_string()));
    }

    #[test]
    fn test_apply_revocation_notice_to_address_book_grace_and_inplace_replace() {
        use std::collections::HashMap;
        let old_pub = "aa".repeat(32);
        let new_pub = "bb".repeat(32);
        let other_pub = "cc".repeat(32);

        let mut contacts = HashMap::new();
        contacts.insert(
            "did:key:zAlice".to_string(),
            FrontendContact {
                did: "did:key:zAlice".to_string(),
                profile: FrontendUserProfile::default(),
                tags: vec![],
                added_at: "100".to_string(),
                notes: None,
                trust_level: None,
                desired_hops: None,
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: "https://old.example.org/sync.php".to_string(),
                    read_url: "https://old.example.org/d-old".to_string(),
                    slot_id: Some("oldslot".to_string()),
                    drop_pub: old_pub.clone(),
                    status: "active".to_string(),
                    ..Default::default()
                }],
            },
        );
        contacts.insert(
            "did:key:zBob".to_string(),
            FrontendContact {
                did: "did:key:zBob".to_string(),
                profile: FrontendUserProfile::default(),
                tags: vec![],
                added_at: "100".to_string(),
                notes: None,
                trust_level: None,
                desired_hops: None,
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![
                    FrontendSyncDescriptor {
                        write_url: "https://old.example.org/sync.php".to_string(),
                        read_url: "https://old.example.org/d-old".to_string(),
                        slot_id: Some("bobold".to_string()),
                        drop_pub: old_pub.clone(),
                        status: "active".to_string(),
                        ..Default::default()
                    },
                    FrontendSyncDescriptor {
                        write_url: "https://keep.example.org/sync.php".to_string(),
                        read_url: "https://keep.example.org/d-keep".to_string(),
                        slot_id: Some("keep".to_string()),
                        drop_pub: other_pub.clone(),
                        status: "active".to_string(),
                        ..Default::default()
                    },
                ],
            },
        );

        let mut ab = crate::models::FrontendAddressBook { contacts };
        let notice = crate::sync_client::SyncRevocationNotice {
            drop_pub: old_pub.clone(),
            issued_at: 1000,
            effective_at: 2000,
            reason: Some("Server-Umzug".to_string()),
            replacement: Some(crate::sync_client::SyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: "https://new.example.org/d-new".to_string(),
                drop_pub: new_pub.clone(),
            }),
            signature: "00".repeat(64),
        };

        // Wrong drop_pub -> no changes (anti-hijacking)
        let mut wrong_notice = notice.clone();
        wrong_notice.drop_pub = "ff".repeat(32);
        assert!(!super::apply_revocation_notice_to_address_book(
            &mut ab,
            "https://old.example.org/d-old",
            &wrong_notice,
            1500
        ));

        // During grace period (now = 1500 < 2000):
        // Alice (1 sync) gets replacement appended with slot_id = None
        // Bob (2 syncs) keeps 2 entries, old marked deprecating with replacement metadata
        assert!(super::apply_revocation_notice_to_address_book(
            &mut ab,
            "https://old.example.org/d-old",
            &notice,
            1500
        ));
        let alice = &ab.contacts["did:key:zAlice"];
        assert_eq!(alice.syncs.len(), 2);
        assert_eq!(alice.syncs[0].status, "deprecating");
        assert_eq!(alice.syncs[1].read_url, "https://new.example.org/d-new");
        assert_eq!(alice.syncs[1].slot_id, None);

        let bob = &ab.contacts["did:key:zBob"];
        assert_eq!(bob.syncs.len(), 2);
        assert_eq!(bob.syncs[0].status, "deprecating");
        assert!(bob.syncs[0].replacement.is_some());

        // After effective_at (now = 2500 >= 2000):
        // Bob's expired entry is replaced in-place with new server (slot_id = None)
        assert!(super::apply_revocation_notice_to_address_book(
            &mut ab,
            "https://old.example.org/d-old",
            &notice,
            2500
        ));
        let bob_after = &ab.contacts["did:key:zBob"];
        assert_eq!(bob_after.syncs.len(), 2);
        assert_eq!(bob_after.syncs[0].read_url, "https://new.example.org/d-new");
        assert_eq!(bob_after.syncs[0].slot_id, None);
        assert_eq!(bob_after.syncs[1].read_url, "https://keep.example.org/d-keep");
    }

    #[test]
    fn test_record_contact_sync_health_preserves_deprecating_and_revoked() {
        let temp_dir = std::env::temp_dir().join(format!("humoco_health_rev_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::commands::sync::tests::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::commands::sync::tests::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::commands::sync::tests::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::commands::sync::tests::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let state = crate::commands::sync::tests::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        let dep_ep = FrontendSyncDescriptor {
            write_url: "https://dep.example.org/sync.php".to_string(),
            read_url: "https://dep.example.org/d-dep".to_string(),
            drop_pub: "aa".repeat(32),
            status: "deprecating".to_string(),
            sunset_at: Some(2000),
            revocation_reason: Some("Umzug".to_string()),
            ..Default::default()
        };
        let rev_ep = FrontendSyncDescriptor {
            write_url: "https://rev.example.org/sync.php".to_string(),
            read_url: "https://rev.example.org/d-rev".to_string(),
            drop_pub: "bb".repeat(32),
            status: "revoked".to_string(),
            sunset_at: Some(1500),
            ..Default::default()
        };

        let contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                id: Some(bob_did.clone()),
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![dep_ep.clone(), rev_ep.clone()],
        };
        state.save_contact(&mut service, contact, Some(password)).unwrap();

        let mut settings = state.load_settings(&mut service, Some(password)).unwrap_or_default();
        settings.market_selected_syncs = vec![dep_ep.clone(), rev_ep.clone()];
        state.save_settings(&mut service, settings, Some(password)).unwrap();

        // 1. Before sunset (now = 1800 < 2000): success=true keeps deprecating, failure=false keeps revoked
        state.record_contact_sync_health_batch(
            &mut service,
            &[
                ("https://dep.example.org/d-dep".to_string(), true),
                ("https://rev.example.org/d-rev".to_string(), false),
            ],
            1800,
            Some(password),
        );

        let contacts_1 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob_1 = contacts_1.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob_1.syncs[0].status, "deprecating");
        assert_eq!(bob_1.syncs[0].last_seen_at, Some(1800));
        assert_eq!(bob_1.syncs[1].status, "revoked");
        assert_eq!(bob_1.syncs[1].fail_count, 1);

        let settings_1 = state.load_settings(&mut service, Some(password)).unwrap();
        assert_eq!(settings_1.market_selected_syncs[0].status, "deprecating");
        assert_eq!(settings_1.market_selected_syncs[1].status, "revoked");

        // 2. Failure on deprecating before sunset preserves deprecating (not "unreachable")
        state.record_contact_sync_health_batch(
            &mut service,
            &[("https://dep.example.org/d-dep".to_string(), false)],
            1900,
            Some(password),
        );
        let contacts_2 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob_2 = contacts_2.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob_2.syncs[0].status, "deprecating");
        assert_eq!(bob_2.syncs[0].fail_count, 1);

        // 3. Past sunset (now = 2100 >= 2000): deprecating is promoted to revoked even on success/fail
        state.record_contact_sync_health_batch(
            &mut service,
            &[("https://dep.example.org/d-dep".to_string(), false)],
            2100,
            Some(password),
        );
        let contacts_3 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob_3 = contacts_3.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob_3.syncs[0].status, "revoked");

        let settings_3 = state.load_settings(&mut service, Some(password)).unwrap();
        assert_eq!(settings_3.market_selected_syncs[0].status, "revoked");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
