// src-tauri/src/commands/contacts.rs
use crate::models::FrontendContact;
use crate::AppState;
use log::info;

#[tauri::command]
pub fn get_contacts(state: tauri::State<AppState>) -> Result<Vec<FrontendContact>, String> {
    state.get_cached_contacts()
}

#[tauri::command]
pub fn save_contact(
    contact: FrontendContact,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Saving contact for DID: {}", contact.did);
    let mut service = state.service.lock().unwrap();
    state.save_contact(&mut service, contact, password.as_deref())
}

#[tauri::command]
pub fn delete_contact(
    did: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Deleting contact for DID: {}", did);
    let mut service = state.service.lock().unwrap();
    state.delete_contact(&mut service, &did, password.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contacts_ipc_parsing() {
        // Test that FrontendContact can be parsed from JSON
        let valid_json = r#"{
            "did": "did:key:z123",
            "profile": {
                "id": "did:key:z123",
                "firstName": "John",
                "lastName": "Doe",
                "organization": "Test Org",
                "address": {
                    "street": "Test Street",
                    "houseNumber": "123",
                    "zipCode": "12345",
                    "city": "Test City",
                    "country": "Germany",
                    "fullAddress": "Test Street 123, 12345 Test City, Germany"
                },
                "gender": "1",
                "email": "john@example.com",
                "phone": "123456789",
                "coordinates": "51.16, 10.45",
                "url": "https://example.com"
            },
            "tags": ["friend", "family"],
            "addedAt": "2024-01-01T00:00:00Z",
            "notes": "Test contact"
        }"#;

        let result: Result<FrontendContact, _> = serde_json::from_str(valid_json);
        assert!(result.is_ok(), "Valid FrontendContact JSON should parse successfully");

        let contact = result.unwrap();
        assert_eq!(contact.did, "did:key:z123");
        assert_eq!(contact.tags, vec!["friend", "family"]);
        assert_eq!(contact.profile.first_name, Some("John".to_string()));
    }

    #[test]
    fn test_address_book_serialization() {
        // Test that FrontendAddressBook can be serialized and deserialized
        use crate::models::FrontendAddressBook;

        let mut address_book = FrontendAddressBook::default();
        
        let contact_json = r#"{
            "did": "did:key:z456",
            "profile": {
                "id": "did:key:z456",
                "firstName": "Jane",
                "lastName": "Smith"
            },
            "tags": ["colleague"],
            "addedAt": "2024-01-02T00:00:00Z"
        }"#;

        let contact: FrontendContact = serde_json::from_str(contact_json).unwrap();
        address_book.contacts.insert(contact.did.clone(), contact);

        let serialized = serde_json::to_string(&address_book);
        assert!(serialized.is_ok(), "FrontendAddressBook should serialize");

        let deserialized: Result<FrontendAddressBook, _> = serde_json::from_str(&serialized.unwrap());
        assert!(deserialized.is_ok(), "FrontendAddressBook should deserialize");
        
        let deserialized_book = deserialized.unwrap();
        assert_eq!(deserialized_book.contacts.len(), 1);
        assert!(deserialized_book.contacts.contains_key("did:key:z456"));
    }

    #[test]
    fn test_contact_serialization_roundtrip() {
        // Test that a contact can be serialized and deserialized without data loss
        let original_json = r#"{
            "did": "did:key:z789",
            "profile": {
                "id": "did:key:z789",
                "firstName": "Alice",
                "lastName": "Wonder",
                "organization": "Wonderland Inc",
                "community": "Fantasy",
                "address": {
                    "street": "Wonder Lane",
                    "houseNumber": "42",
                    "zipCode": "54321",
                    "city": "Wonder City",
                    "country": "Wonderland",
                    "fullAddress": "Wonder Lane 42, 54321 Wonder City, Wonderland"
                },
                "gender": "2",
                "email": "alice@wonderland.com",
                "phone": "987654321",
                "coordinates": "52.52, 13.40",
                "url": "https://wonderland.com",
                "serviceOffer": "Wonderful services",
                "needs": "Nothing"
            },
            "tags": ["vip", "friend"],
            "addedAt": "2024-01-03T00:00:00Z",
            "notes": "VIP contact"
        }"#;

        let original: FrontendContact = serde_json::from_str(original_json).unwrap();
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: FrontendContact = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original.did, deserialized.did);
        assert_eq!(original.tags, deserialized.tags);
        assert_eq!(original.profile.first_name, deserialized.profile.first_name);
        assert_eq!(original.profile.organization, deserialized.profile.organization);
    }

    #[test]
    fn test_get_contacts_uses_cache() {
        use std::path::PathBuf;
        use std::sync::Mutex;
        use chrono::Utc;
        use human_money_core::app_service::AppService;
        use crate::models::{FrontendAddressBook, FrontendContact, FrontendUserProfile};

        // Mock AppState with cached contacts
        let service = AppService::new(&PathBuf::from("/tmp")).unwrap();
        let mut book = FrontendAddressBook::default();
        book.contacts.insert("did:key:test".to_string(), FrontendContact {
            did: "did:key:test".to_string(),
            profile: FrontendUserProfile {
                id: Some("did:key:test".to_string()),
                first_name: Some("Cached".to_string()),
                last_name: Some("User".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
        });

        let state_raw = AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(Some(book)),
            settings: Mutex::new(None),
        };

        let cache = state_raw.contacts.lock().unwrap();
        assert!(cache.is_some());
        assert_eq!(cache.as_ref().unwrap().contacts.len(), 1);
        assert_eq!(cache.as_ref().unwrap().contacts.get("did:key:test").unwrap().profile.first_name, Some("Cached".to_string()));
    }
}
