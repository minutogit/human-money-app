// src-tauri/src/state.rs
use std::sync::{Mutex, MutexGuard};
use log::{info, error};
use chrono::{Duration, Utc};
use human_money_core::app_service::AppService;
use crate::models::{TransactionRecord, FrontendAddressBook, FrontendContact};
use crate::settings::{AppSettings, SETTINGS_KEY};

pub const TRANSACTION_HISTORY_KEY: &str = "transaction_history";
pub const CONTACTS_DATA_NAME: &str = "address_book";

pub struct AppState {
    pub service: Mutex<AppService>,
    pub history: Mutex<Option<Vec<TransactionRecord>>>,
    pub events: Mutex<Option<Vec<human_money_core::models::wallet_event::WalletEvent>>>,
    pub contacts: Mutex<Option<crate::models::FrontendAddressBook>>,
    pub settings: Mutex<Option<AppSettings>>,
}

impl AppState {
    // 1. Central helper method to access the locked service
    pub fn service(&self) -> MutexGuard<'_, AppService> {
        self.service.lock().unwrap()
    }

    // 2. Settings Helpers
    pub fn get_cached_settings(&self) -> Result<AppSettings, String> {
        let cache = self.settings.lock().unwrap();
        cache
            .as_ref()
            .cloned()
            .ok_or_else(|| "Settings not loaded".to_string())
    }

    pub fn load_settings(&self, service: &mut AppService, password: Option<&str>) -> Result<AppSettings, String> {
        let settings: AppSettings = match service.load_encrypted_data(SETTINGS_KEY, password) {
            Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
            Err(_) => {
                info!("No settings file found, creating default settings.");
                let default_settings = AppSettings::default();
                let bytes = serde_json::to_vec(&default_settings).unwrap();
                service
                    .save_encrypted_data(SETTINGS_KEY, &bytes, password)
                    .map_err(|e| format!("Failed to save default settings: {}", e))?;
                default_settings
            }
        };

        *self.settings.lock().unwrap() = Some(settings.clone());
        Ok(settings)
    }

    pub fn save_settings(&self, service: &mut AppService, settings: AppSettings, password: Option<&str>) -> Result<(), String> {
        let bytes = serde_json::to_vec(&settings).map_err(|e| e.to_string())?;
        service.save_encrypted_data(SETTINGS_KEY, &bytes, password)?;
        *self.settings.lock().unwrap() = Some(settings);
        Ok(())
    }

    // 3. Contacts Helpers
    pub fn get_cached_contacts(&self) -> Result<Vec<FrontendContact>, String> {
        let cache = self.contacts.lock().unwrap();
        if let Some(book) = cache.as_ref() {
            let mut contacts: Vec<FrontendContact> = book.contacts.values().cloned().collect();
            contacts.sort_by(|a, b| {
                let name_a = a.profile.first_name.as_deref().unwrap_or("");
                let name_b = b.profile.first_name.as_deref().unwrap_or("");
                name_a.to_lowercase().cmp(&name_b.to_lowercase())
            });
            Ok(contacts)
        } else {
            Err("Contacts cache not initialized".to_string())
        }
    }

    pub fn load_contacts(&self, service: &mut AppService, password: Option<&str>) -> Result<Vec<FrontendContact>, String> {
        info!("Fetching all contacts from address book from disk...");
        match service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
            Ok(data) => {
                let address_book: FrontendAddressBook = serde_json::from_slice(&data)
                    .map_err(|e| {
                        error!("Failed to parse address book: {}", e);
                        format!("Failed to parse address book: {}", e)
                    })?;

                *self.contacts.lock().unwrap() = Some(address_book.clone());

                let mut contacts: Vec<FrontendContact> = address_book.contacts.into_values().collect();
                contacts.sort_by(|a, b| {
                    let name_a = a.profile.first_name.as_deref().unwrap_or("");
                    let name_b = b.profile.first_name.as_deref().unwrap_or("");
                    name_a.to_lowercase().cmp(&name_b.to_lowercase())
                });
                Ok(contacts)
            }
            Err(e) => {
                if e.contains("NotFound") || e.contains("not found") {
                    info!("No address book found, returning empty list.");
                    let default_book = FrontendAddressBook::default();
                    *self.contacts.lock().unwrap() = Some(default_book);
                    Ok(vec![])
                } else {
                    error!("Error loading address book: {}", e);
                    Ok(vec![])
                }
            }
        }
    }

    pub fn save_contact(&self, service: &mut AppService, contact: FrontendContact, password: Option<&str>) -> Result<(), String> {
        let mut address_book = match service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
            Ok(data) => serde_json::from_slice::<FrontendAddressBook>(&data).unwrap_or_default(),
            Err(e) => {
                if e.contains("Password required") || e.contains("Session timed out") || e.contains("Wallet is locked") {
                    return Err(e);
                }
                FrontendAddressBook::default()
            }
        };

        address_book.contacts.insert(contact.did.clone(), contact);

        let data = serde_json::to_vec(&address_book)
            .map_err(|e| format!("Failed to serialize address book: {}", e))?;

        service.save_encrypted_data(CONTACTS_DATA_NAME, &data, password)
            .map_err(|e| format!("Failed to save address book: {}", e))?;

        *self.contacts.lock().unwrap() = Some(address_book);
        Ok(())
    }

    pub fn delete_contact(&self, service: &mut AppService, did: &str, password: Option<&str>) -> Result<(), String> {
        let mut address_book = match service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
            Ok(data) => serde_json::from_slice::<FrontendAddressBook>(&data).unwrap_or_default(),
            Err(e) => {
                if e.contains("Password required") || e.contains("Session timed out") || e.contains("Wallet is locked") {
                    return Err(e);
                }
                return Ok(());
            }
        };

        if address_book.contacts.remove(did).is_some() {
            let data = serde_json::to_vec(&address_book)
                .map_err(|e| format!("Failed to serialize address book: {}", e))?;

            service.save_encrypted_data(CONTACTS_DATA_NAME, &data, password)
                .map_err(|e| format!("Failed to save address book: {}", e))?;

            *self.contacts.lock().unwrap() = Some(address_book);
        }
        Ok(())
    }

    // 4. History Helpers
    pub fn get_cached_history(&self) -> Result<Vec<TransactionRecord>, String> {
        let cache = self.history.lock().unwrap();
        cache
            .as_ref()
            .cloned()
            .ok_or_else(|| "History cache not initialized".to_string())
    }

    pub fn load_history_from_disk(&self, service: &mut AppService, password: Option<&str>) -> Result<Vec<TransactionRecord>, String> {
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
                        Err(msg)
                    }
                }
            }
            Err(_) => {
                info!("Keine Transaktionshistorie auf Festplatte gefunden. Erstelle neue.");
                let empty = Vec::new();
                *self.history.lock().unwrap() = Some(empty.clone());
                Ok(empty)
            }
        }
    }

    pub fn save_history(&self, service: &mut AppService, history: Vec<TransactionRecord>, password: Option<&str>) -> Result<(), String> {
        let history_bytes = serde_json::to_vec(&history).map_err(|e| e.to_string())?;
        service.save_encrypted_data(TRANSACTION_HISTORY_KEY, &history_bytes, password)?;
        *self.history.lock().unwrap() = Some(history);
        Ok(())
    }

    pub fn append_to_history(&self, service: &mut AppService, record: TransactionRecord, password: Option<&str>) -> Result<(), String> {
        let mut history = self.load_history_from_disk(service, password)?;
        history.push(record);
        self.save_history(service, history, password)
    }

    // 5. Events Helpers
    pub fn refresh_events_cache(&self, service: &mut AppService, password: Option<&str>) -> Result<(), String> {
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
    }

    /// Central session initializer — called after Login, Create Profile, Recovery, and Handover.
    /// Loads all caches (Settings, History, Events, Contacts) and starts the session timer.
    pub fn initialize_profile_session(
        &self,
        service: &mut AppService,
        password: &str,
    ) -> Result<(), String> {
        info!("Initializing profile session and member caches...");

        // 1. Load or create settings
        let settings = self.load_settings(service, Some(password))?;

        // 2. Start session timer if configured
        if settings.session_timeout_seconds > 0 {
            service.unlock_session(password, settings.session_timeout_seconds)?;
        }

        // 3. Load transaction history and clean up expired bundle data
        let mut history = self.load_history_from_disk(service, Some(password))?;
        let mut changed = false;
        let retention_duration = Duration::days(settings.bundle_retention_days as i64);

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

        Ok(())
    }
}
