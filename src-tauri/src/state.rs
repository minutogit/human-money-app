// src-tauri/src/state.rs
use crate::models::{FrontendAddressBook, FrontendContact, TransactionRecord};
use crate::settings::{AppSettings, SETTINGS_KEY};
use chrono::{Duration, Utc};
use human_money_core::app_service::{AppFacadeError, AppService};
use log::{error, info};
use std::sync::{Mutex, MutexGuard};

pub const TRANSACTION_HISTORY_KEY: &str = "transaction_history";
pub const CONTACTS_DATA_NAME: &str = "address_book";
pub const WOT_DATA_NAME: &str = "wot_register";
pub const MARKET_CARD_STORE_KEY: &str = "market_card_store";
pub const MARKET_OWN_PROFILE_KEY: &str = "market_own_profile";
pub const MARKET_SYNC_ADMIN_KEY: &str = "market_sync_admin";
pub const MARKET_SYNC_KEY: &str = "market_sync_state";
pub const BACKUP_SYNC_STATE_KEY: &str = "backup_sync_state";
pub const MARKET_POST_STATE_KEY: &str = "market_post_state";
pub const APP_NOTICES_KEY: &str = "app_notices";
pub const MAX_APP_NOTICES: usize = 50;
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
    pub inquiries: Vec<crate::models::FrontendMarketInquiryItem>,
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
    pub max_total_storage_mb: usize,
    pub max_user_storage_mb: usize,
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
    pub wot_tombstone_fp_by_write_url: std::collections::BTreeMap<String, String>,
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

/// Central application state shared across Tauri commands and background workers.
///
/// # Lock Ordering Invariant
/// Whenever multiple mutexes on `AppState` must be acquired simultaneously,
/// acquire them strictly in the following total order to prevent deadlocks:
/// `service -> settings -> contacts -> wot -> history -> events -> media_negative_cache`.
///
/// Never hold any `MutexGuard` across an `.await` point.
pub struct AppState {
    pub service: Mutex<AppService>,
    pub history: Mutex<Option<Vec<TransactionRecord>>>,
    pub events: Mutex<Option<Vec<human_money_core::models::wallet_event::WalletEvent>>>,
    pub contacts: Mutex<Option<crate::models::FrontendAddressBook>>,
    pub settings: Mutex<Option<AppSettings>>,
    pub wot: Mutex<Option<humoco_wot::TrustRegister>>,
    pub media_negative_cache: Mutex<std::collections::HashMap<String, u64>>,
}

/// Loads and deserializes an encrypted JSON block, returning `Ok(None)` only on `StorageError::NotFound`.
///
/// Propagates lock, session, authentication, I/O, and JSON parse errors without silent defaults.
pub fn load_encrypted_json_opt<T: serde::de::DeserializeOwned>(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
) -> Result<Option<T>, AppFacadeError> {
    match service.load_encrypted_data(key, password) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| AppFacadeError::Json(format!("Failed to parse {}: {}", key, e))),
        Err(AppFacadeError::Storage(human_money_core::StorageError::NotFound)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Loads and deserializes an encrypted JSON block, falling back to `T::default()` only on `StorageError::NotFound`.
///
/// Propagates lock, session, authentication, I/O, and JSON parse errors so corrupt or locked
/// storage is never silently treated as empty state.
pub fn load_encrypted_json_or_default<T: serde::de::DeserializeOwned + Default>(
    service: &mut AppService,
    key: &str,
    password: Option<&str>,
) -> Result<T, AppFacadeError> {
    Ok(load_encrypted_json_opt(service, key, password)?.unwrap_or_default())
}

/// Serializes `value` as JSON and saves it to the encrypted store under `key`.
pub fn save_encrypted_json<T: serde::Serialize>(
    service: &mut AppService,
    key: &str,
    value: &T,
    password: Option<&str>,
) -> Result<(), AppFacadeError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| AppFacadeError::Json(format!("Failed to serialize {}: {}", key, e)))?;
    service.save_encrypted_data(key, &bytes, password)
}

fn endpoints_overlap(a_read: &str, a_write: &str, b_read: &str, b_write: &str) -> bool {
    let ar = crate::services::common::normalize_sync_url(a_read);
    let aw = crate::services::common::normalize_sync_url(a_write);
    let br = crate::services::common::normalize_sync_url(b_read);
    let bw = crate::services::common::normalize_sync_url(b_write);
    (!ar.is_empty() && (ar == br || ar == bw)) || (!aw.is_empty() && (aw == br || aw == bw))
}

impl AppState {
    pub fn new(service: AppService) -> Self {
        Self {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(None),
            wot: Mutex::new(None),
            media_negative_cache: Mutex::new(std::collections::HashMap::new()),
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

    // 2. Settings Helpers
    pub fn get_cached_settings(&self) -> Result<AppSettings, AppFacadeError> {
        let cache = self.settings.lock().unwrap();
        cache
            .as_ref()
            .cloned()
            .ok_or_else(|| AppFacadeError::Generic("Settings not loaded".to_string()))
    }

    pub fn load_settings(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<AppSettings, AppFacadeError> {
        let settings: AppSettings =
            match load_encrypted_json_opt::<AppSettings>(service, SETTINGS_KEY, password)? {
                Some(loaded) => loaded,
                None => {
                    info!("No settings file found, creating default settings.");
                    let default_settings = AppSettings::default();
                    save_encrypted_json(service, SETTINGS_KEY, &default_settings, password)
                        .map_err(|e| {
                            AppFacadeError::Generic(format!(
                                "Failed to save default settings: {}",
                                e
                            ))
                        })?;
                    default_settings
                }
            };

        *self.settings.lock().unwrap() = Some(settings.clone());
        Ok(settings)
    }

    pub fn save_settings(
        &self,
        service: &mut AppService,
        settings: AppSettings,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        let prev_min = {
            let guard = self.settings.lock().unwrap();
            guard.as_ref().map(|s| s.share_min_level)
        }
        .or_else(|| {
            load_encrypted_json_opt::<AppSettings>(service, SETTINGS_KEY, password)
                .ok()
                .flatten()
                .map(|s| s.share_min_level)
        });
        let new_min = settings.share_min_level;
        save_encrypted_json(service, SETTINGS_KEY, &settings, password)?;
        *self.settings.lock().unwrap() = Some(settings);
        if prev_min.is_some_and(|prev| new_min > prev) {
            crate::services::market_service::bump_own_card_version_for_eviction(service, password);
        }
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
            Err(AppFacadeError::Generic(
                "Contacts cache not initialized".to_string(),
            ))
        }
    }

    pub fn load_contacts(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<Vec<FrontendContact>, AppFacadeError> {
        info!("Fetching all contacts from address book from disk...");
        let address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password).map_err(|e| {
                error!("Failed to load address book: {:?}", e);
                e
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

    pub fn save_contact(
        &self,
        service: &mut AppService,
        mut contact: FrontendContact,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        contact.did = contact.did.trim().to_string();
        let mut address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)?;

        let can_did = canonicalize_did(&contact.did)
            .map_err(|e| AppFacadeError::Generic(format!("Invalid contact DID: {}", e)))?;

        let own_ids: Option<(String, String)> = service
            .with_wallet(|w| {
                let uid = w.get_user_id().trim().to_string();
                let can = canonicalize_did(&uid).unwrap_or_else(|_| uid.clone());
                (uid, can)
            })
            .ok();

        if own_ids
            .as_ref()
            .is_some_and(|(own_uid, _)| contact.did == *own_uid)
        {
            return Err(AppFacadeError::Generic(
                "Cannot add your own active profile as a contact".to_string(),
            ));
        }

        let is_own_root = own_ids
            .as_ref()
            .is_some_and(|(_, own_can)| *own_can == can_did);

        if is_own_root && contact.trust_level.is_some() {
            return Err(AppFacadeError::Generic(
                "Self trust forbidden: cannot assign a trust level to your own subaccount"
                    .to_string(),
            ));
        }

        // Deduplicate address_book.contacts: for own subaccounts keep distinct prefixes separate;
        // for external contacts deduplicate by canonical DID.
        let keys_to_remove: Vec<String> = address_book
            .contacts
            .keys()
            .filter(|k| {
                if *k == &contact.did {
                    false
                } else if is_own_root {
                    k.trim() == contact.did
                } else if let Ok(k_can) = canonicalize_did(k) {
                    k_can == can_did
                } else {
                    false
                }
            })
            .cloned()
            .collect();

        let is_existing =
            address_book.contacts.contains_key(&contact.did) || !keys_to_remove.is_empty();

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
            let card_syncs: Vec<crate::models::FrontendSyncDescriptor> =
                match load_encrypted_json_or_default::<humoco_market::CardStore>(
                    service,
                    MARKET_CARD_STORE_KEY,
                    password,
                ) {
                    Ok(store) => store
                        .cards
                        .values()
                        .find(|sc| {
                            canonicalize_did(&sc.card.payload.did).ok().as_deref() == Some(&can_did)
                        })
                        .map(|sc| {
                            sc.card
                                .payload
                                .drops
                                .iter()
                                .cloned()
                                .map(Into::into)
                                .collect()
                        })
                        .unwrap_or_default(),
                    Err(e) => {
                        log::warn!("Failed to load CardStore during save_contact: {}", e);
                        Vec::new()
                    }
                };

            let mut all_syncs = Vec::new();
            all_syncs.extend(contact.syncs);
            all_syncs.extend(card_syncs);
            contact.syncs = merge_contact_syncs(&[], &all_syncs);
            contact.syncs.truncate(2);
        }

        // Synchronize TrustRegister if wallet is unlocked (skip for own subaccounts: self-trust is forbidden)
        if is_own_root {
            contact.trust_level = None;
            contact.desired_hops = None;
        } else if service.with_wallet(|_| ()).is_ok() {
            let mut reg = self.get_or_load_wot_register(service, password)?;
            let now_secs = Utc::now().timestamp() as u64;
            let prev_level = reg.own_contacts.get(&can_did).map(|e| e.level).unwrap_or(0);
            let new_eff_level = contact.trust_level.unwrap_or(0);

            if let Some(level) = contact.trust_level {
                if level > 3 {
                    return Err(AppFacadeError::Generic(format!(
                        "Invalid trust level: {}",
                        level
                    )));
                }
                let hops = if level == 0 {
                    0
                } else {
                    reg.policy.own_list_max_hops
                };
                reg.set_contact_trust(&can_did, level, hops, now_secs)
                    .map_err(|e| AppFacadeError::Generic(e.to_string()))?;
                if level == 0 {
                    self.prune_local_data_on_trust_withdrawal(
                        service,
                        &mut reg,
                        Some(&can_did),
                        password,
                    );
                } else {
                    reg.prune_unreachable_lists();
                }
                self.ensure_wot_list_signed(service, &mut reg)?;
                self.save_wot_register(service, &reg, password)?;
                contact.trust_level = Some(level);
                contact.desired_hops = Some(hops);
            } else {
                let removed = reg
                    .remove_contact_trust(&can_did)
                    .map_err(|e| AppFacadeError::Generic(e.to_string()))?;
                if removed {
                    self.prune_local_data_on_trust_withdrawal(
                        service,
                        &mut reg,
                        Some(&can_did),
                        password,
                    );
                    if reg.own_list_dirty {
                        self.ensure_wot_list_signed(service, &mut reg)?;
                    }
                    self.save_wot_register(service, &reg, password)?;
                }
            }

            if new_eff_level < prev_level {
                crate::services::market_service::bump_own_card_version_for_eviction(
                    service, password,
                );
            }
        }

        let removed_endpoints: Vec<crate::models::FrontendSyncDescriptor> = old_contact_syncs
            .into_iter()
            .filter(|old_ep| {
                !contact.syncs.iter().any(|new_ep| {
                    endpoints_overlap(
                        &old_ep.read_url,
                        &old_ep.write_url,
                        &new_ep.read_url,
                        &new_ep.write_url,
                    )
                })
            })
            .collect();

        address_book.contacts.insert(contact.did.clone(), contact);

        save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password)
            .map_err(|e| AppFacadeError::Generic(format!("Failed to save address book: {}", e)))?;

        *self.contacts.lock().unwrap() = Some(address_book.clone());
        self.prune_orphaned_market_selected_syncs(
            service,
            &address_book,
            &removed_endpoints,
            password,
        );
        Ok(())
    }

    pub fn delete_contact(
        &self,
        service: &mut AppService,
        did: &str,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        let did = did.trim();
        let mut address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)?;

        let can_target = canonicalize_did(did).ok();
        let own_can_did = service
            .with_wallet(|w| canonicalize_did(w.get_user_id()).ok())
            .ok()
            .flatten();
        let is_own_root = can_target.is_some() && own_can_did == can_target;

        let keys_to_remove: Vec<String> = address_book
            .contacts
            .keys()
            .filter(|k| {
                if is_own_root {
                    k.trim() == did
                } else {
                    *k == did || (can_target.is_some() && canonicalize_did(k).ok() == can_target)
                }
            })
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
            save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password).map_err(
                |e| AppFacadeError::Generic(format!("Failed to save address book: {}", e)),
            )?;

            *self.contacts.lock().unwrap() = Some(address_book.clone());
            self.prune_orphaned_market_selected_syncs(
                service,
                &address_book,
                &removed_endpoints,
                password,
            );
        }

        if !is_own_root && service.with_wallet(|_| ()).is_ok() {
            if let Ok(mut reg) = self.get_or_load_wot_register(service, password) {
                let target_key = can_target.as_deref().unwrap_or(did);
                if let Ok(removed) = reg.remove_contact_trust(target_key) {
                    if removed {
                        self.prune_local_data_on_trust_withdrawal(
                            service,
                            &mut reg,
                            Some(target_key),
                            password,
                        );
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

    pub fn load_history_from_disk(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<Vec<TransactionRecord>, AppFacadeError> {
        let records: Vec<TransactionRecord> =
            load_encrypted_json_or_default(service, TRANSACTION_HISTORY_KEY, password).map_err(
                |e| {
                    error!("Failed to load transaction history: {}", e);
                    e
                },
            )?;
        *self.history.lock().unwrap() = Some(records.clone());
        Ok(records)
    }

    pub fn save_history(
        &self,
        service: &mut AppService,
        history: Vec<TransactionRecord>,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        save_encrypted_json(service, TRANSACTION_HISTORY_KEY, &history, password)?;
        *self.history.lock().unwrap() = Some(history);
        Ok(())
    }

    pub fn append_to_history(
        &self,
        service: &mut AppService,
        record: TransactionRecord,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        let mut history = self.load_history_from_disk(service, password)?;
        history.push(record);
        self.save_history(service, history, password)
    }

    // 5. Events Helpers
    pub fn refresh_events_cache(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        info!("Refreshing events cache...");
        let events = service.get_event_history(0, 50, password)?;
        *self.events.lock().unwrap() = Some(events);
        Ok(())
    }

    // 6. Logout Helpers
    pub fn clear_all_caches(&self) {
        info!("Clearing all caches on logout...");
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
                    log::debug!("Clearing bundle data for expired transaction record");
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
        if let Err(e) = self.load_contacts(service, Some(credential)) {
            log::warn!("Failed to pre-load contacts during session init: {}", e);
        }

        // 6. Load the Web of Trust register
        self.load_wot_register(service, Some(credential))?;

        Ok(())
    }

    // 7. Web of Trust (WoT) Helpers
    pub fn prune_local_data_on_trust_withdrawal(
        &self,
        service: &mut AppService,
        reg: &mut humoco_wot::TrustRegister,
        revoked_did: Option<&str>,
        password: Option<&str>,
    ) {
        let can_revoked = revoked_did.and_then(|d| canonicalize_did(d).ok());
        if let Some(ref can) = can_revoked {
            reg.raw_list_store.remove(can);
            reg.invalidate_graph_cache();
        }
        reg.prune_unreachable_lists();
        reg.clear_drop_sent_log(None);

        match load_encrypted_json_or_default::<humoco_market::CardStore>(
            service,
            MARKET_CARD_STORE_KEY,
            password,
        ) {
            Ok(mut store) => {
                let before = store.cards.len();
                if let Some(ref can) = can_revoked {
                    if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(can) {
                        store.cards.remove(pk.as_bytes());
                    }
                }
                store.cards.retain(|_, sc| {
                    if sc.hops < 2 {
                        return true;
                    }
                    reg.evaluate_did(&sc.card.payload.did, None)
                        .is_ok_and(|ev| ev.shortest_hops.is_some())
                });
                if store.cards.len() != before {
                    let _ = save_encrypted_json(service, MARKET_CARD_STORE_KEY, &store, password);
                }
            }
            Err(e) => {
                log::warn!(
                    "Failed to load CardStore during trust withdrawal prune: {}",
                    e
                );
            }
        }
    }

    pub fn ensure_wot_list_signed(
        &self,
        service: &mut AppService,
        reg: &mut humoco_wot::TrustRegister,
    ) -> Result<bool, AppFacadeError> {
        let now_secs = Utc::now().timestamp() as u64;
        service.with_wallet_and_identity(|_wallet, identity| {
            let signer = DidWotSigner::from_identity(identity);
            reg.ensure_own_list_signed(&signer, now_secs).map_err(|e| {
                AppFacadeError::Generic(format!("Failed to sign WoT trust list: {}", e))
            })
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
        let mut reg = match load_encrypted_json_opt::<humoco_wot::TrustRegister>(
            service,
            WOT_DATA_NAME,
            password,
        )? {
            Some(loaded) => loaded,
            None => {
                info!("No WoT register found, creating new one");
                humoco_wot::TrustRegister::new(&own_can_did, humoco_wot::WotPolicy::default())
                    .map_err(|e| {
                        AppFacadeError::Generic(format!(
                            "Failed to initialize TrustRegister: {}",
                            e
                        ))
                    })?
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
        save_encrypted_json(service, WOT_DATA_NAME, reg, password)
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
        use crate::models::FrontendVoucherTrustDto;
        use human_money_core::models::conflict::TrustStatus;

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

        let is_offender = matches!(
            wallet.check_reputation(creator_id),
            TrustStatus::KnownOffender(_)
        );
        if is_offender {
            return FrontendVoucherTrustDto {
                score: 0.0,
                trust_percent: 0,
                is_own_voucher: false,
                blocked_by_offender: true,
                ..Default::default()
            };
        }

        let valid_guarantors: Vec<&str> = guarantor_ids
            .iter()
            .copied()
            .filter(|gid| !matches!(wallet.check_reputation(gid), TrustStatus::KnownOffender(_)))
            .collect();

        let guarantor_canonical_set: std::collections::HashSet<String> = valid_guarantors
            .iter()
            .filter_map(|gid| canonicalize_did(gid).ok())
            .collect();

        let (eval, provider_filter_names) = {
            let wot_guard = self.wot.lock().unwrap();
            let reg = match wot_guard.as_ref() {
                Some(r) => r,
                None => return FrontendVoucherTrustDto::default(),
            };

            let creator_canonical = creator_can.as_deref().unwrap_or(creator_id);
            let valid_guarantor_canonicals: Vec<String> = valid_guarantors
                .iter()
                .map(|gid| canonicalize_did(gid).unwrap_or_else(|_| gid.to_string()))
                .collect();
            let valid_guarantor_refs: Vec<&str> = valid_guarantor_canonicals
                .iter()
                .map(|s| s.as_str())
                .collect();

            let now_secs = Utc::now().timestamp() as u64;
            let eval = match reg.evaluate_voucher(
                creator_canonical,
                &valid_guarantor_refs,
                false,
                Some(now_secs),
            ) {
                Ok(e) => e,
                Err(_) => return FrontendVoucherTrustDto::default(),
            };

            let mut provider_filter_names = std::collections::HashMap::new();
            if let Some(best_path) = eval.paths.first() {
                for hop in &best_path.hops {
                    if let Ok(can) = canonicalize_did(hop) {
                        if let Some(pf) = reg.provider_filters.get(&can) {
                            provider_filter_names.insert(can, pf.payload.name.clone());
                        }
                    }
                }
            }
            (eval, provider_filter_names)
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
                    .and_then(|can| provider_filter_names.get(&can))
                    .cloned()
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
        let guarantor_ids: Vec<&str> = signatures
            .iter()
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
            log::warn!(
                "Failed to save WoT register after piggybacked import: {}",
                e
            );
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

        let mut store: humoco_market::CardStore =
            match load_encrypted_json_or_default(service, MARKET_CARD_STORE_KEY, password) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!(
                        "Failed to load CardStore for piggybacked contact card: {}",
                        e
                    );
                    return;
                }
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
                log::info!("Accepted piggybacked contact card (replaced={})", replaced);
                if let Err(e) =
                    save_encrypted_json(service, MARKET_CARD_STORE_KEY, &store, password)
                {
                    log::warn!(
                        "Failed to save CardStore after accepting piggybacked card: {}",
                        e
                    );
                }
                if let Some(stored) = store.cards.values().find(|sc| sc.card.payload.did == did) {
                    let card_syncs: Vec<crate::models::FrontendSyncDescriptor> = stored
                        .card
                        .payload
                        .drops
                        .iter()
                        .cloned()
                        .map(Into::into)
                        .collect();
                    let card_profile = humoco_market::extract_public_profile(&stored.card);
                    self.update_contact_from_card(
                        service,
                        &did,
                        &card_syncs,
                        Some(&card_profile),
                        password,
                    );
                }
            }
            Ok(other) => {
                log::debug!("Piggybacked contact card outcome: {:?}", other);
            }
            Err(e) => {
                log::warn!(
                    "Error verifying reputation for piggybacked contact card: {}",
                    e
                );
            }
        }
    }

    pub fn update_contact_from_card(
        &self,
        service: &mut AppService,
        card_did: &str,
        card_syncs: &[crate::models::FrontendSyncDescriptor],
        card_profile: Option<&human_money_core::models::profile::PublicProfile>,
        password: Option<&str>,
    ) {
        if card_syncs.is_empty() && card_profile.is_none() {
            return;
        }
        let can_card_did = match canonicalize_did(card_did) {
            Ok(d) => d,
            Err(_) => return,
        };

        let mut address_book: FrontendAddressBook =
            match load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password) {
                Ok(ab) => ab,
                Err(e) => {
                    log::warn!(
                        "Failed to load address book in update_contact_from_card: {}",
                        e
                    );
                    return;
                }
            };

        let mut updated = false;
        for (k, contact) in address_book.contacts.iter_mut() {
            let is_match =
                k == card_did || canonicalize_did(k).ok().as_deref() == Some(&can_card_did);
            if is_match {
                if !card_syncs.is_empty() {
                    let merged = merge_contact_syncs(&contact.syncs, card_syncs);
                    if contact.syncs != merged {
                        contact.syncs = merged;
                        updated = true;
                    }
                }
                if let Some(prof) = card_profile {
                    if apply_card_profile_to_contact(contact, prof) {
                        updated = true;
                    }
                }
            }
        }

        if updated {
            let _ = save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password);
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

        let mut address_book: FrontendAddressBook =
            match load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password) {
                Ok(ab) => ab,
                Err(e) => {
                    log::warn!(
                        "Failed to load address book in record_contact_sync_health_batch: {}",
                        e
                    );
                    return false;
                }
            };

        let settings_res = self.load_settings(service, password);
        let settings_opt = settings_res.ok();

        let mut updated = false;
        for (url, success) in results {
            let norm_target = crate::services::common::normalize_sync_url(url);
            if norm_target.is_empty() {
                continue;
            }
            for contact in address_book.contacts.values_mut() {
                for sync_ep in &mut contact.syncs {
                    let norm_read = crate::services::common::normalize_sync_url(&sync_ep.read_url);
                    let norm_write =
                        crate::services::common::normalize_sync_url(&sync_ep.write_url);
                    if norm_read == norm_target || norm_write == norm_target {
                        if sync_ep.status == "deprecating"
                            && sync_ep.sunset_at.is_some_and(|eff| now_secs >= eff)
                        {
                            sync_ep.status = "revoked".to_string();
                        }
                        let is_revocation_state =
                            sync_ep.status == "deprecating" || sync_ep.status == "revoked";
                        if *success {
                            sync_ep.last_seen_at = Some(now_secs);
                            sync_ep.fail_count = 0;
                            if !is_revocation_state {
                                if sync_ep.status == "active" {
                                    // remains active
                                } else if sync_ep.status == "unreachable" {
                                    let is_selected = settings_opt
                                        .as_ref()
                                        .map(|s| {
                                            s.market_selected_syncs.iter().any(|sel| {
                                                endpoints_overlap(
                                                    &sel.read_url,
                                                    &sel.write_url,
                                                    &norm_read,
                                                    &norm_write,
                                                )
                                            })
                                        })
                                        .unwrap_or(false);
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
                            if !is_revocation_state
                                && (sync_ep.status != "pending_auth" || sync_ep.fail_count >= 3)
                            {
                                sync_ep.status = "unreachable".to_string();
                            }
                        }
                        updated = true;
                    }
                }
            }
        }

        if updated {
            let _ = save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password);
            *self.contacts.lock().unwrap() = Some(address_book);
        }

        if let Some(mut settings) = settings_opt {
            let mut settings_updated = false;
            for (url, success) in results {
                let norm_target = crate::services::common::normalize_sync_url(url);
                if norm_target.is_empty() {
                    continue;
                }
                for sel in &mut settings.market_selected_syncs {
                    let norm_read = crate::services::common::normalize_sync_url(&sel.read_url);
                    let norm_write = crate::services::common::normalize_sync_url(&sel.write_url);
                    if norm_read == norm_target || norm_write == norm_target {
                        if sel.status == "deprecating"
                            && sel.sunset_at.is_some_and(|eff| now_secs >= eff)
                        {
                            sel.status = "revoked".to_string();
                        }
                        let is_revocation_state =
                            sel.status == "deprecating" || sel.status == "revoked";
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

        let admin_opt: Option<StoredSyncAdmin> = match load_encrypted_json_opt::<StoredSyncAdmin>(
            service,
            MARKET_SYNC_ADMIN_KEY,
            password,
        ) {
            Ok(opt) => opt,
            Err(e) => {
                log::warn!(
                    "Failed to load {} during prune_orphaned_market_selected_syncs: {}",
                    MARKET_SYNC_ADMIN_KEY,
                    e
                );
                return;
            }
        };

        let initial_count = settings.market_selected_syncs.len();
        settings.market_selected_syncs.retain(|sel| {
            let sr = crate::services::common::normalize_sync_url(&sel.read_url);
            let sw = crate::services::common::normalize_sync_url(&sel.write_url);
            if sr.is_empty() && sw.is_empty() {
                return false;
            }

            let was_removed = removed_endpoints
                .iter()
                .any(|rem| endpoints_overlap(&rem.read_url, &rem.write_url, &sr, &sw));

            if !was_removed {
                return true;
            }

            // 1. Matches StoredSyncAdmin?
            let matches_admin = admin_opt.as_ref().is_some_and(|a| {
                endpoints_overlap(&a.descriptor.read_url, &a.descriptor.write_url, &sr, &sw)
            });

            if matches_admin {
                return true;
            }

            // 2. Matches any remaining contact's syncs?
            let matches_contact = address_book.contacts.values().any(|c| {
                c.syncs
                    .iter()
                    .any(|ep| endpoints_overlap(&ep.read_url, &ep.write_url, &sr, &sw))
            });

            if matches_contact {
                return true;
            }

            // 3. Matches any settings.public_syncs?
            settings
                .public_syncs
                .iter()
                .any(|ps| endpoints_overlap(&ps.read_url, &ps.write_url, &sr, &sw))
        });

        if settings.market_selected_syncs.len() != initial_count {
            let updated_syncs = settings.market_selected_syncs.clone();
            let _ = self.save_settings(service, settings, password);
            crate::services::market_service::sync_own_card_drops_on_settings_change(
                service,
                &updated_syncs,
                password,
            );
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
        self.record_contact_sync_health_batch(
            service,
            &[(endpoint_url.to_string(), success)],
            now_secs,
            password,
        )
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

pub struct DidWotSigner {
    pub did: String,
    pub signing_key: ed25519_dalek::SigningKey,
}

impl DidWotSigner {
    pub fn from_identity(identity: &human_money_core::models::profile::UserIdentity) -> Self {
        let did = canonicalize_did(&identity.user_id).unwrap_or_else(|_| {
            identity
                .user_id
                .split_once('@')
                .map(|(_, d)| d.to_string())
                .unwrap_or_else(|| identity.user_id.clone())
        });
        Self {
            did,
            signing_key: identity.signing_key.clone(),
        }
    }
}

impl humoco_wot::WotSigner for DidWotSigner {
    fn did(&self) -> &str {
        &self.did
    }

    fn sign(&self, message: &[u8]) -> [u8; 64] {
        human_money_core::services::crypto::sign_ed25519(&self.signing_key, message).to_bytes()
    }
}

fn enrich_contact_with_wot(
    contact: &mut FrontendContact,
    wot_opt: Option<&humoco_wot::TrustRegister>,
) {
    if let Some(reg) = wot_opt {
        if let Ok(can_did) = canonicalize_did(&contact.did) {
            if reg.self_did == can_did {
                contact.trust_level = None;
                contact.desired_hops = None;
                contact.has_peer_list = false;
                contact.peer_list_updated_at = None;
                return;
            }
            contact.trust_level = reg.own_contacts.get(&can_did).map(|e| e.level);
            contact.desired_hops = reg.own_contacts.get(&can_did).map(|e| e.desired_hops);
            contact.has_peer_list = reg.raw_list_store.contains_key(&can_did);
            contact.peer_list_updated_at = reg.raw_list_store.get(&can_did).map(|l| l.iat);
        }
    }
}

fn format_contact_display_name(c: &FrontendContact) -> Option<String> {
    let first = c.profile.first_name.as_deref().unwrap_or("").trim();
    let last = c.profile.last_name.as_deref().unwrap_or("").trim();
    let name = match (first.is_empty(), last.is_empty()) {
        (false, false) => format!("{} {}", first, last),
        (false, true) => first.to_string(),
        (true, false) => last.to_string(),
        (true, true) => c
            .profile
            .organization
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string(),
    };
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn lookup_contact_name(
    contacts_map: Option<&FrontendAddressBook>,
    target_did: &str,
) -> Option<String> {
    let book = contacts_map?;
    let trimmed_target = target_did.trim();
    if let Some((_, exact_contact)) = book
        .contacts
        .iter()
        .find(|(k, _)| k.trim() == trimmed_target)
    {
        if let Some(name) = format_contact_display_name(exact_contact) {
            return Some(name);
        }
    }
    let can_target = canonicalize_did(trimmed_target).ok()?;
    for c in book.contacts.values() {
        if canonicalize_did(&c.did).ok().as_ref() == Some(&can_target) {
            if let Some(name) = format_contact_display_name(c) {
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
    let w = crate::services::common::normalize_sync_url(&d.write_url);
    if !w.is_empty() {
        w
    } else {
        crate::services::common::normalize_sync_url(&d.read_url)
    }
}

/// Applies disclosed business card profile fields to an existing address-book contact.
/// Always updates `picture_url` when present on the card, and populates missing names
/// without overwriting custom names already set by the user.
pub fn apply_card_profile_to_contact(
    contact: &mut FrontendContact,
    card_profile: &human_money_core::models::profile::PublicProfile,
) -> bool {
    let mut changed = false;

    if let Some(ref pic) = card_profile.picture_url {
        let trimmed = pic.trim();
        if !trimmed.is_empty() && contact.profile.picture_url.as_deref() != Some(trimmed) {
            contact.profile.picture_url = Some(trimmed.to_string());
            changed = true;
        }
    }

    let has_existing_name = contact
        .profile
        .first_name
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
        || contact
            .profile
            .last_name
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
        || contact
            .profile
            .organization
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());

    if !has_existing_name {
        if let Some(ref first) = card_profile.first_name {
            if !first.trim().is_empty() {
                contact.profile.first_name = Some(first.trim().to_string());
                changed = true;
            }
        }
        if let Some(ref last) = card_profile.last_name {
            if !last.trim().is_empty() {
                contact.profile.last_name = Some(last.trim().to_string());
                changed = true;
            }
        }
        if let Some(ref org) = card_profile.organization {
            if !org.trim().is_empty() {
                contact.profile.organization = Some(org.trim().to_string());
                changed = true;
            }
        }
    }

    changed
}

impl AppState {
    pub fn load_app_notices(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> crate::models::StoredAppNotices {
        load_encrypted_json_or_default(service, APP_NOTICES_KEY, password).unwrap_or_else(|e| {
            log::warn!("Failed to load {}: {}", APP_NOTICES_KEY, e);
            Default::default()
        })
    }

    pub fn save_app_notices(
        &self,
        service: &mut AppService,
        notices: &crate::models::StoredAppNotices,
        password: Option<&str>,
    ) -> Result<(), crate::models::FrontendError> {
        save_encrypted_json(service, APP_NOTICES_KEY, notices, password)
            .map_err(crate::models::FrontendError::from)
    }

    /// Updates `last_sync_at` and prepends new notices, truncating to `MAX_APP_NOTICES` (50).
    pub fn append_app_notices(
        &self,
        last_sync_at: Option<u64>,
        mut new_items: Vec<crate::models::FrontendAppNotice>,
        password: Option<&str>,
    ) {
        if last_sync_at.is_none() && new_items.is_empty() {
            return;
        }
        let mut service = self.service.lock().unwrap();
        let mut store: crate::models::StoredAppNotices =
            match load_encrypted_json_or_default(&mut service, APP_NOTICES_KEY, password) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("Failed to load app notices before append: {}", e);
                    return;
                }
            };
        if let Some(ts) = last_sync_at {
            store.last_sync_at = Some(store.last_sync_at.map_or(ts, |prev| prev.max(ts)));
        }
        if !new_items.is_empty() {
            new_items.extend(store.notices);
            new_items.truncate(MAX_APP_NOTICES);
            store.notices = new_items;
        }
        if let Err(e) = self.save_app_notices(&mut service, &store, password) {
            log::warn!("Failed to persist app notices: {:?}", e);
        }
    }
}

pub fn merge_contact_syncs(
    existing: &[crate::models::FrontendSyncDescriptor],
    card_syncs: &[crate::models::FrontendSyncDescriptor],
) -> Vec<crate::models::FrontendSyncDescriptor> {
    let mut result: Vec<crate::models::FrontendSyncDescriptor> = Vec::with_capacity(2);

    let merge_entry = |res: &mut Vec<crate::models::FrontendSyncDescriptor>,
                       d: &crate::models::FrontendSyncDescriptor| {
        let w = crate::services::common::normalize_sync_url(&d.write_url);
        let r = crate::services::common::normalize_sync_url(&d.read_url);
        if w.is_empty() && r.is_empty() {
            return;
        }

        if let Some(target) = res.iter_mut().find(|existing_entry| {
            let ew = crate::services::common::normalize_sync_url(&existing_entry.write_url);
            let er = crate::services::common::normalize_sync_url(&existing_entry.read_url);
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

pub use crate::models::SyncRevocationFields;

pub fn apply_revocation_notice<T: SyncRevocationFields>(
    ep: &mut T,
    notice: &crate::sync_client::SyncRevocationNotice,
    now_secs: u64,
) {
    *ep.status_mut() = if now_secs >= notice.effective_at {
        "revoked".to_string()
    } else {
        "deprecating".to_string()
    };
    *ep.sunset_at_mut() = Some(notice.effective_at);
    *ep.revocation_reason_mut() = notice.reason.clone();
    *ep.replacement_mut() = notice.replacement.clone().map(Into::into);
}

pub fn clear_revocation<T: SyncRevocationFields>(ep: &mut T, fallback_status: &str) -> bool {
    let is_rev_status = {
        let s = ep.status_mut().as_str();
        s == "deprecating" || s == "revoked"
    };
    if ep.sunset_at_mut().is_some()
        || ep.revocation_reason_mut().is_some()
        || ep.replacement_mut().is_some()
        || is_rev_status
    {
        if is_rev_status {
            *ep.status_mut() = fallback_status.to_string();
        }
        *ep.sunset_at_mut() = None;
        *ep.revocation_reason_mut() = None;
        *ep.replacement_mut() = None;
        true
    } else {
        false
    }
}

pub fn apply_revocation_notice_to_address_book(
    address_book: &mut crate::models::FrontendAddressBook,
    old_read_url: &str,
    notice: &crate::sync_client::SyncRevocationNotice,
    now_secs: u64,
) -> bool {
    let old_norm = crate::services::common::normalize_sync_url(old_read_url);
    let notice_pub = notice.drop_pub.trim().to_lowercase();
    let is_revoked_now = now_secs >= notice.effective_at;
    let mut updated = false;

    for contact in address_book.contacts.values_mut() {
        let syncs_len = contact.syncs.len();
        let existing_endpoints: Vec<(String, String)> = contact
            .syncs
            .iter()
            .map(|o| {
                (
                    crate::services::common::normalize_sync_url(&o.read_url),
                    crate::services::common::normalize_sync_url(&o.write_url),
                )
            })
            .collect();
        let mut idx_to_replace: Option<(usize, crate::models::FrontendSyncDescriptor)> = None;
        let mut idx_to_append: Option<crate::models::FrontendSyncDescriptor> = None;

        for (idx, ep) in contact.syncs.iter_mut().enumerate() {
            if crate::services::common::normalize_sync_url(&ep.read_url) != old_norm {
                continue;
            }
            if !ep.drop_pub.trim().eq_ignore_ascii_case(&notice_pub) {
                continue;
            }

            let prev_status = ep.status.clone();
            let prev_sunset = ep.sunset_at;
            let prev_reason = ep.revocation_reason.clone();
            let prev_rep = ep.replacement.clone();

            apply_revocation_notice(ep, notice, now_secs);
            if ep.status != prev_status
                || ep.sunset_at != prev_sunset
                || ep.revocation_reason != prev_reason
                || ep.replacement != prev_rep
            {
                updated = true;
            }

            if let Some(ref rep) = notice.replacement {
                let rep_r = crate::services::common::normalize_sync_url(&rep.read_url);
                let rep_w = crate::services::common::normalize_sync_url(&rep.write_url);
                let ep_r = crate::services::common::normalize_sync_url(&ep.read_url);
                let ep_w = crate::services::common::normalize_sync_url(&ep.write_url);

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
    use crate::models::{FrontendContact, FrontendSyncDescriptor, FrontendUserProfile};
    use chrono::Utc;

    #[test]
    fn test_save_contact_authoritative_syncs_deletion() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_state_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::test_support::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::test_support::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let state = crate::test_support::create_test_state(alice_service);
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

        state
            .save_contact(&mut service, initial_contact, Some(password))
            .unwrap();

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

        state
            .save_contact(&mut service, updated_one_sync, Some(password))
            .unwrap();

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

        state
            .save_contact(&mut service, updated_zero_syncs, Some(password))
            .unwrap();

        // 5. Verify all syncs deleted
        let loaded3 = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob3 = loaded3.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(
            bob3.syncs.len(),
            0,
            "All syncs must be removed authoritatively"
        );
    }

    #[test]
    fn test_record_contact_sync_health_batch() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_health_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::test_support::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::test_support::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let state = crate::test_support::create_test_state(alice_service);
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
        state
            .save_contact(&mut service, contact, Some(password))
            .unwrap();

        // 1. Success update preserves "unverified" (does NOT overwrite with "active")
        let updated = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            true,
            1000,
            Some(password),
        );
        assert!(updated);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_seen_at, Some(1000));
        assert_eq!(bob.syncs[0].fail_count, 0);
        assert_eq!(
            bob.syncs[0].status, "unverified",
            "unverified status must not be overwritten with active on static read success"
        );

        // 1b. Test that "pending_auth" is also preserved on read success
        let mut bob_pending = bob.clone();
        bob_pending.syncs[0].status = "pending_auth".to_string();
        state
            .save_contact(&mut service, bob_pending, Some(password))
            .unwrap();
        let updated_pending = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            true,
            1050,
            Some(password),
        );
        assert!(updated_pending);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(
            bob.syncs[0].status, "pending_auth",
            "pending_auth status must not be overwritten with active on static read success"
        );

        // 2. Transient failure on pending_auth increments fail_count without immediately flipping to unreachable
        let updated2 = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            false,
            1100,
            Some(password),
        );
        assert!(updated2);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_error_at, Some(1100));
        assert_eq!(bob.syncs[0].fail_count, 1);
        assert_eq!(bob.syncs[0].status, "pending_auth");

        // 3. Second failure still keeps pending_auth; third consecutive failure transitions to unreachable
        let updated3 = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            false,
            1200,
            Some(password),
        );
        assert!(updated3);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_error_at, Some(1200));
        assert_eq!(bob.syncs[0].fail_count, 2);
        assert_eq!(bob.syncs[0].status, "pending_auth");

        let updated3b = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            false,
            1250,
            Some(password),
        );
        assert!(updated3b);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_error_at, Some(1250));
        assert_eq!(bob.syncs[0].fail_count, 3);
        assert_eq!(bob.syncs[0].status, "unreachable");

        // 4. Recovery success resets fail_count; since not in market_selected_syncs, transitions to "unverified"
        let updated4 = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            true,
            1300,
            Some(password),
        );
        assert!(updated4);
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob.syncs[0].last_seen_at, Some(1300));
        assert_eq!(bob.syncs[0].fail_count, 0);
        assert_eq!(
            bob.syncs[0].status, "unverified",
            "unreachable contact sync transitions to unverified on read success when not in market_selected_syncs"
        );

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
        state
            .save_settings(&mut service, settings, Some(password))
            .unwrap();

        // Mark contact as unreachable again
        let _ = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            false,
            1400,
            Some(password),
        );
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(
            contacts.iter().find(|c| c.did == bob_did).unwrap().syncs[0].status,
            "unreachable"
        );

        // Now recover: should transition to "active" because it's in market_selected_syncs
        let _ = state.update_contact_sync_health(
            &mut service,
            "https://sync-endpoint.org/sync.php",
            true,
            1500,
            Some(password),
        );
        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(
            contacts.iter().find(|c| c.did == bob_did).unwrap().syncs[0].status,
            "active"
        );
    }

    #[test]
    fn test_prune_orphaned_market_selected_syncs() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_prune_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::test_support::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::test_support::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let state = crate::test_support::create_test_state(alice_service);
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
        state
            .save_contact(&mut service, contact.clone(), Some(password))
            .unwrap();

        // Put Bob's sync into settings.market_selected_syncs
        let mut settings = state.load_settings(&mut service, Some(password)).unwrap();
        settings.market_selected_syncs = vec![bob_sync];
        state
            .save_settings(&mut service, settings, Some(password))
            .unwrap();

        // 1. Updating contact to remove sync should prune it from settings.market_selected_syncs
        let mut updated_contact = contact.clone();
        updated_contact.syncs = vec![];
        state
            .save_contact(&mut service, updated_contact, Some(password))
            .unwrap();

        let s_after = state.load_settings(&mut service, Some(password)).unwrap();
        assert_eq!(
            s_after.market_selected_syncs.len(),
            0,
            "Orphaned contact sync must be pruned from market_selected_syncs on save_contact"
        );

        // 2. Add contact back with sync, add to market_selected_syncs, then delete_contact
        state
            .save_contact(&mut service, contact, Some(password))
            .unwrap();
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
        state
            .save_settings(&mut service, settings2, Some(password))
            .unwrap();

        state
            .delete_contact(&mut service, &bob_did, Some(password))
            .unwrap();
        let s_after_del = state.load_settings(&mut service, Some(password)).unwrap();
        assert_eq!(
            s_after_del.market_selected_syncs.len(),
            0,
            "Orphaned contact sync must be pruned from market_selected_syncs on delete_contact"
        );
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
        let merged =
            super::merge_contact_syncs(std::slice::from_ref(&ep1), std::slice::from_ref(&ep2));
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
        assert_eq!(
            merged2.len(),
            1,
            "Matching write_url should be merged into single entry"
        );
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
        assert_eq!(
            bob_after.syncs[1].read_url,
            "https://keep.example.org/d-keep"
        );
    }

    #[test]
    fn test_record_contact_sync_health_preserves_deprecating_and_revoked() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_health_rev_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let bob_service = crate::test_support::setup_test_app(
            &temp_dir.join("bob"),
            "Test Bob",
            crate::test_support::BOB_PHRASE,
            password,
        );
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let state = crate::test_support::create_test_state(alice_service);
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
        state
            .save_contact(&mut service, contact, Some(password))
            .unwrap();

        let mut settings = state
            .load_settings(&mut service, Some(password))
            .unwrap_or_default();
        settings.market_selected_syncs = vec![dep_ep.clone(), rev_ep.clone()];
        state
            .save_settings(&mut service, settings, Some(password))
            .unwrap();

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

    #[test]
    fn test_save_and_delete_own_subaccounts_and_reject_self_profile_and_self_trust() {
        let temp_dir = std::env::temp_dir().join(format!(
            "humoco_subaccount_contact_test_{}",
            uuid::Uuid::new_v4()
        ));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let (own_uid, own_vk) = alice_service
            .with_wallet_and_identity(|w, id| {
                (w.get_user_id().to_string(), id.signing_key.verifying_key())
            })
            .unwrap();
        let laden_uid = human_money_core::crypto::create_user_id(&own_vk, Some("laden")).unwrap();
        let privat_uid = human_money_core::crypto::create_user_id(&own_vk, Some("privat")).unwrap();

        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.service.lock().unwrap();

        // 1. Exact own active profile must be rejected
        let self_contact = FrontendContact {
            did: own_uid.clone(),
            profile: FrontendUserProfile {
                id: Some(own_uid.clone()),
                first_name: Some("Ich".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        let self_err = state
            .save_contact(&mut service, self_contact, Some(password))
            .unwrap_err();
        assert!(self_err.to_string().contains("active profile"));

        // 2. Own subaccount with trust_level must be rejected (Self trust forbidden)
        let rated_sub = FrontendContact {
            did: laden_uid.clone(),
            profile: FrontendUserProfile {
                id: Some(laden_uid.clone()),
                first_name: Some("Mein Laden".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(1),
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        let trust_err = state
            .save_contact(&mut service, rated_sub, Some(password))
            .unwrap_err();
        assert!(trust_err.to_string().contains("Self trust forbidden"));

        // 3. Two distinct own subaccounts (trust_level: None) co-exist without overwriting each other
        let laden_contact = FrontendContact {
            did: laden_uid.clone(),
            profile: FrontendUserProfile {
                id: Some(laden_uid.clone()),
                first_name: Some("Mein Laden".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        let privat_contact = FrontendContact {
            did: privat_uid.clone(),
            profile: FrontendUserProfile {
                id: Some(privat_uid.clone()),
                first_name: Some("Mein Privat".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        state
            .save_contact(&mut service, laden_contact, Some(password))
            .unwrap();
        state
            .save_contact(&mut service, privat_contact, Some(password))
            .unwrap();

        let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(
            contacts.len(),
            2,
            "Both own subaccounts must stay in address book"
        );
        assert!(contacts
            .iter()
            .any(|c| c.did == laden_uid && c.trust_level.is_none()));
        assert!(contacts
            .iter()
            .any(|c| c.did == privat_uid && c.trust_level.is_none()));

        // 4. lookup_contact_name resolves exact subaccount name rather than sibling
        let book = state.contacts.lock().unwrap().clone();
        assert_eq!(
            super::lookup_contact_name(book.as_ref(), &laden_uid).as_deref(),
            Some("Mein Laden")
        );
        assert_eq!(
            super::lookup_contact_name(book.as_ref(), &privat_uid).as_deref(),
            Some("Mein Privat")
        );

        // 5. Deleting one subaccount must leave sibling intact
        state
            .delete_contact(&mut service, &privat_uid, Some(password))
            .unwrap();
        let remaining = state.load_contacts(&mut service, Some(password)).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].did, laden_uid);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_encrypted_json_helpers_reject_corrupt_and_locked_state() {
        use human_money_core::app_service::AppFacadeError;

        let temp_dir =
            std::env::temp_dir().join(format!("humoco_json_hardening_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let alice_service = crate::test_support::setup_test_app(
            &temp_dir.join("alice"),
            "Test Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let state = crate::test_support::create_test_state(alice_service);
        let mut service = state.lock_service().unwrap();

        // Missing keys cleanly return defaults or initialize defaults
        let book: crate::models::FrontendAddressBook = super::load_encrypted_json_or_default(
            &mut service,
            super::CONTACTS_DATA_NAME,
            Some(password),
        )
        .unwrap();
        assert!(book.contacts.is_empty());
        assert!(state.load_settings(&mut service, Some(password)).is_ok());

        // Missing session / wrong auth propagates error instead of silent fallback
        assert!(matches!(
            state.load_settings(&mut service, None),
            Err(AppFacadeError::SessionNotActive(_))
        ));
        assert!(matches!(
            state.load_contacts(&mut service, Some("wrong-password")),
            Err(AppFacadeError::Storage(
                human_money_core::StorageError::AuthenticationFailed
            ))
        ));

        // Corrupt JSON on disk MUST return AppFacadeError::Json (never silent default)
        for key in [
            crate::settings::SETTINGS_KEY,
            super::CONTACTS_DATA_NAME,
            super::WOT_DATA_NAME,
            super::TRANSACTION_HISTORY_KEY,
        ] {
            service
                .save_encrypted_data(key, b"{corrupt-json", Some(password))
                .unwrap();
        }
        assert!(matches!(
            state.load_settings(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));
        assert!(matches!(
            state.load_contacts(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));
        assert!(matches!(
            state.load_wot_register(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));
        assert!(matches!(
            state.load_history_from_disk(&mut service, Some(password)),
            Err(AppFacadeError::Json(_))
        ));

        drop(service);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_generic_revocation_helpers_for_descriptor_and_public_sync() {
        let notice = crate::sync_client::SyncRevocationNotice {
            drop_pub: "aa".repeat(32),
            issued_at: 1000,
            effective_at: 2000,
            reason: Some("Umzug".to_string()),
            replacement: Some(crate::sync_client::SyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: "https://new.example.org/d".to_string(),
                drop_pub: "bb".repeat(32),
            }),
            signature: "00".repeat(64),
        };

        let mut desc = FrontendSyncDescriptor::default();
        super::apply_revocation_notice(&mut desc, &notice, 1500);
        assert_eq!(desc.status, "deprecating");
        assert_eq!(desc.sunset_at, Some(2000));
        assert_eq!(desc.revocation_reason.as_deref(), Some("Umzug"));
        assert!(desc.replacement.is_some());
        assert!(super::clear_revocation(&mut desc, "active"));
        assert_eq!(desc.status, "active");
        assert!(desc.sunset_at.is_none());
        assert!(desc.revocation_reason.is_none());
        assert!(desc.replacement.is_none());
        assert!(!super::clear_revocation(&mut desc, "active"));

        let mut pub_sync = crate::settings::StoredPublicSync::default();
        super::apply_revocation_notice(&mut pub_sync, &notice, 2500);
        assert_eq!(pub_sync.status, "revoked");
        assert_eq!(pub_sync.sunset_at, Some(2000));
        assert!(super::clear_revocation(&mut pub_sync, "active"));
        assert_eq!(pub_sync.status, "active");
        assert!(pub_sync.sunset_at.is_none());
    }
}
