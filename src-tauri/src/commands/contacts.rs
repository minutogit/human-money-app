// src-tauri/src/commands/contacts.rs
use crate::models::{FrontendContact, FrontendAddressBook};
use crate::AppState;
use log::{info, error};

const CONTACTS_DATA_NAME: &str = "address_book";

#[tauri::command]
pub fn get_contacts(state: tauri::State<AppState>) -> Result<Vec<FrontendContact>, String> {
    info!("Fetching all contacts from address book...");
    let mut service = state.service.lock().unwrap();
    
    match service.load_encrypted_data(CONTACTS_DATA_NAME, None) {
        Ok(data) => {
            let address_book: FrontendAddressBook = serde_json::from_slice(&data)
                .map_err(|e| {
                    error!("Failed to parse address book: {}", e);
                    format!("Failed to parse address book: {}", e)
                })?;
            // Konvertiere HashMap-Werte in einen Vektor
            let mut contacts: Vec<FrontendContact> = address_book.contacts.into_values().collect();
            // Sortiere nach Name (optional, aber User-freundlich)
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
                Ok(vec![])
            } else {
                error!("Error loading address book: {}", e);
                // Wenn das Wallet gesperrt ist, geben wir eine leere Liste zurück, 
                // statt den User mit Fehlermeldungen zu nerven (die UI fragt sowieso nach Login).
                Ok(vec![])
            }
        }
    }
}

#[tauri::command]
pub fn save_contact(
    contact: FrontendContact,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Saving contact for DID: {}", contact.did);
    let mut service = state.service.lock().unwrap();
    
    // 1. Lade bestehendes Adressbuch
    let mut address_book = match service.load_encrypted_data(CONTACTS_DATA_NAME, None) {
        Ok(data) => serde_json::from_slice::<FrontendAddressBook>(&data)
            .unwrap_or_default(),
        Err(_) => FrontendAddressBook::default(),
    };

    // 2. Kontakt hinzufügen oder aktualisieren
    address_book.contacts.insert(contact.did.clone(), contact);

    // 3. Serialisieren und speichern
    let data = serde_json::to_vec(&address_book)
        .map_err(|e| format!("Failed to serialize address book: {}", e))?;
    
    service.save_encrypted_data(CONTACTS_DATA_NAME, &data, None)
        .map_err(|e| format!("Failed to save address book: {}", e))
}

#[tauri::command]
pub fn delete_contact(
    did: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Deleting contact for DID: {}", did);
    let mut service = state.service.lock().unwrap();
    
    // 1. Lade bestehendes Adressbuch
    let mut address_book = match service.load_encrypted_data(CONTACTS_DATA_NAME, None) {
        Ok(data) => serde_json::from_slice::<FrontendAddressBook>(&data)
            .unwrap_or_default(),
        Err(_) => return Ok(()), // Nichts zu löschen, wenn kein Buch existiert
    };

    // 2. Entfernen
    if address_book.contacts.remove(&did).is_some() {
        // 3. Wenn gelöscht, Speichern
        let data = serde_json::to_vec(&address_book)
            .map_err(|e| format!("Failed to serialize address book: {}", e))?;
        
        service.save_encrypted_data(CONTACTS_DATA_NAME, &data, None)
            .map_err(|e| format!("Failed to save address book: {}", e))?;
    }

    Ok(())
}
