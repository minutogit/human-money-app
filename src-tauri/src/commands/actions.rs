// src-tauri/src/commands/actions.rs
use crate::{models::FrontendNewVoucherData, AppState, settings::{AppSettings, SETTINGS_KEY}};
use chrono::Utc;
use log::{info, error}; // <--- 'debug' wieder entfernt, wir nutzen info!
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use voucher_lib::{
    archive::VoucherArchive,
    app_service::AppService,
    models::voucher::Voucher,
    services::voucher_manager::NewVoucherData,
    wallet::{MultiTransferRequest, SourceTransfer, InvolvedVoucherInfo},
};

#[derive(Deserialize)]
pub struct FrontendSourceTransfer {
    local_instance_id: String,
    amount_to_send: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransactionRecord {
    pub id: String,
    pub direction: String, // "sent" or "received"
    pub recipient_id: String,
    pub sender_id: String,
    pub timestamp: String, // ISO 8601

    // Rust-Feldnamen sind jetzt snake_case, um Warnings zu beheben.
    #[serde(rename = "summableAmounts", alias = "total_amount_by_unit", alias = "summable_amounts", default)] // Serialisiert als camelCase, fängt alle alten Varianten ab
    pub summable_amounts: HashMap<String, String>,

    #[serde(rename = "countableItems", alias = "countable_items", default)] // Serialisiert als camelCase, fängt alle alten Varianten ab
    pub countable_items: HashMap<String, u32>,
    pub involved_vouchers: Vec<String>, // local_instance_ids
    // DAS FEHLENDE FELD:
    #[serde(rename = "involvedSourcesDetails", alias = "involved_sources_details", default)]
    pub involved_sources_details: Option<Vec<InvolvedVoucherInfo>>, // Muss Option<> sein, da es bei "received" fehlt

    pub bundle_data: Vec<u8>,
    pub bundle_id: String,
    pub notes: Option<String>,
    pub sender_profile_name: Option<String>,
}

// NEU: Diese Struktur wird an das Frontend zurückgegeben, wenn der Empfang erfolgreich war.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveSuccessPayload {
    pub sender_id: String,
    pub sender_profile_name: Option<String>,
    pub notes: Option<String>,
    pub transfer_summary: FrontendTransferSummary, // <--- Geändert
    pub involved_vouchers: Vec<String>,
    pub involved_vouchers_details: Vec<InvolvedVoucherInfo>,
}

// NEU: Diese Struktur wird an das Frontend zurückgegeben, wenn ein Bundle ERSTELLT wurde.
// Sie entspricht dem, was SendView.tsx erwartet.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateBundleResult {
    pub bundle_data: Vec<u8>,
    pub bundle_id: String,
    // NEU: Dieses Feld wird jetzt an das Frontend durchgereicht.
    pub involved_sources_details: Vec<InvolvedVoucherInfo>,
}

#[tauri::command]
pub fn create_transfer_bundle(
    recipient_id: String,
    sources: Vec<FrontendSourceTransfer>,
    notes: Option<String>,
    sender_profile_name: Option<String>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>, // <--- GEÄNDERT
    state: tauri::State<AppState>,
) -> Result<CreateBundleResult, String> { // RÜCKGABETYP GEÄNDERT
    info!(
        "Attempting to create a transfer bundle for recipient: {}",
        recipient_id
    );
    let mut service = state.service.lock().unwrap();

    let source_transfers = sources
        .into_iter()
        .map(|s| SourceTransfer {
            local_instance_id: s.local_instance_id,
            amount_to_send: s.amount_to_send,
        })
        .collect();

    let request = MultiTransferRequest {
        recipient_id,
        sources: source_transfers,
        notes,
        sender_profile_name,
    };

    let archive: Option<&dyn VoucherArchive> = None;

    // RUF DIE BIBLIOTHEK AUF UND VERARBEITE DIE CreateBundleResult ANTWORT
    match service.create_transfer_bundle(request, &standard_definitions_toml, archive, password.as_deref()) {
        Ok(result) => {
            // 'result' ist die neue CreateBundleResult aus der voucher_lib
            Ok(CreateBundleResult {
                bundle_data: result.bundle_bytes, // Feld 'bundle_bytes' verwenden
                bundle_id: result.bundle_id,     // Feld 'bundle_id' verwenden
                involved_sources_details: result.involved_sources_details, // NEU: Details durchreichen
            })
        }
        Err(e) => Err(e),
    }
}

// NEU: Der Befehl zum Empfangen von Bundles.
#[tauri::command]
pub fn receive_bundle(
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>, // <--- GEÄNDERT
    state: tauri::State<AppState>,
) -> Result<ReceiveSuccessPayload, String> {
    info!("Attempting to receive and process a transfer bundle...");
    let mut service = state.service.lock().unwrap();
    let archive: Option<&dyn VoucherArchive> = None;

    match service.receive_bundle(&bundle_data, &standard_definitions_toml, archive, password.as_deref()) {
        Ok(result) => {
            info!("Bundle processed successfully. Creating transaction record.");

            if !result.check_result.verifiable_conflicts.is_empty() {
                let msg = "Bundle processing failed due to verifiable double-spend conflicts.".to_string();
                error!("{}", msg);
                return Err(msg);
            }

            // Entferne alte Summenberechnung. Nutze Daten direkt aus dem ProcessBundleResult.
            let transfer_summary = result.transfer_summary.clone();
            let involved_vouchers_details = result.involved_vouchers_details.clone(); // <--- HIER SIND DIE DETAILS
            let involved_vouchers_ids: Vec<String> = involved_vouchers_details.iter().map(|d| d.local_instance_id.clone()).collect();
            let notes = result.header.notes.clone();
            let sender_profile_name = result.header.sender_profile_name.clone();
            let bundle_id = result.header.bundle_id.clone();

            let record = TransactionRecord {
                id: Uuid::new_v4().to_string(),
                direction: "received".to_string(),
                recipient_id: service.get_user_id()?, // The current user is the recipient
                sender_id: result.header.sender_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                summable_amounts: transfer_summary.summable_amounts.clone(), // <-- Feldname zu snake_case geändert
                countable_items: transfer_summary.countable_items.clone(), // <-- Feldname zu snake_case geändert
                involved_vouchers: involved_vouchers_ids.clone(), // <--- KORRIGIERT
                involved_sources_details: Some(involved_vouchers_details.clone()), // <--- KORRIGIERT
                bundle_data: Vec::new(), // Bundle-Daten werden nur für "sent" gespeichert
                bundle_id,
                notes: notes.clone(),
                sender_profile_name: sender_profile_name.clone(),
            };

            // Erstelle die Frontend-kompatible TransferSummary
            let fe_transfer_summary = FrontendTransferSummary {
                summable_amounts: transfer_summary.summable_amounts.clone(),
                countable_items: transfer_summary.countable_items.clone(),
            };

            // Speichere den neuen Eintrag atomar in der Historie.
            let mut history = load_history_from_disk(&mut service, password.as_deref())?;
            let new_record = record.clone();
            history.push(record);
            let history_bytes = serde_json::to_vec(&history).map_err(|e| e.to_string())?;
            service.save_encrypted_data(TRANSACTION_HISTORY_KEY, &history_bytes, password.as_deref())?;

            let mut history_cache = state.history.lock().unwrap();
            if let Some(cache) = history_cache.as_mut() {
                cache.push(new_record);
            } else {
                *history_cache = Some(history);
            }

            info!("Transaction record for received bundle saved successfully.");

            Ok(ReceiveSuccessPayload {
                sender_id: result.header.sender_id,
                sender_profile_name,
                notes,
                transfer_summary: fe_transfer_summary, // <--- Geändert
                involved_vouchers: involved_vouchers_ids, // <--- KORRIGIERT
                involved_vouchers_details, // <--- KORRIGIERT
            })
        }
        Err(e) => {
            error!("Failed to process bundle: {}", e);
            Err(e)
        }
    }
}

// NEU: Eine lokale Struktur, die #[serde(rename_all = "camelCase")] erzwingt,
// da die TransferSummary aus der voucher_lib dies nicht garantiert.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTransferSummary {
    pub summable_amounts: HashMap<String, String>,
    pub countable_items: HashMap<String, u32>,
}

pub const TRANSACTION_HISTORY_KEY: &str = "transaction_history";

#[tauri::command]
pub fn save_transaction_record(
    record: TransactionRecord,
    password: Option<String>, // <--- GEÄNDERT
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Saving new transaction record with id: {}", record.id);
    let mut service = state.service.lock().unwrap();

    let mut history = load_history_from_disk(&mut service, password.as_deref())?;
    let new_record = record.clone();
    history.push(record);

    let history_bytes = serde_json::to_vec(&history).map_err(|e| e.to_string())?;
    service.save_encrypted_data(TRANSACTION_HISTORY_KEY, &history_bytes, password.as_deref())?;

    let mut history_cache = state.history.lock().unwrap();
    if let Some(cache) = history_cache.as_mut() {
        cache.push(new_record);
    } else {
        *history_cache = Some(history);
    }
    Ok(())
}

pub fn load_history_from_disk(
    service: &mut AppService,
    password: Option<&str>, // <--- GEÄNDERT
) -> Result<Vec<TransactionRecord>, String> {
    match service.load_encrypted_data(TRANSACTION_HISTORY_KEY, password) {
        Ok(data) => {
            // NEU: Debug-Ausgabe des Roh-JSONs
            // let raw_json = String::from_utf8_lossy(&data); // <-- Entfernt, da es zu groß sein kann (bundle_data)
            // info!("Lade Transaktionshistorie (Roh-JSON): {}", raw_json);

            let records: Result<Vec<TransactionRecord>, _> = serde_json::from_slice(&data);
            match records {
                 Ok(parsed_records) => {
                    // Debug-Ausgaben entfernt
                    // info!("Transaktionshistorie erfolgreich geparst. {} Einträge geladen.", parsed_records.len());
                    // for (i, record) in parsed_records.iter().enumerate() {
                    //     info!("  Record[{}]: ID={}, summableAmounts={:?}, countableItems={:?}", // <-- Feldnamen geändert
                    //         i, record.id, record.summableAmounts, record.countableItems
                    //     );
                    // }
                    Ok(parsed_records)
                }
                Err(e) => {
                     let msg = format!("Failed to parse transaction history: {}", e);
                     error!("{}", msg); // Extra loggen, da der map_err das sonst verbirgt
                     Err(msg)
                }
            }
        },
        Err(_) => {
            info!("Keine Transaktionshistorie auf Festplatte gefunden. Erstelle neue.");
            Ok(Vec::new())
        }
    }
}

#[tauri::command]
pub fn get_transaction_history(state: tauri::State<AppState>) -> Result<Vec<TransactionRecord>, String> {
    info!("Getting transaction history from cache...");
    let history_cache = state.history.lock().unwrap();
    history_cache
        .as_ref()
        .cloned()
        .ok_or_else(|| "Transaction history not available. User might be logged out.".to_string())
}

#[tauri::command]
pub fn get_app_settings(state: tauri::State<AppState>) -> Result<AppSettings, String> {
    info!("Getting app settings from cache...");
    let settings_cache = state.settings.lock().unwrap();
    settings_cache
        .as_ref()
        .cloned()
        .ok_or_else(|| "Settings not available. User might be logged out.".to_string())
}

#[tauri::command]
pub fn save_app_settings(
    settings: AppSettings,
    password: Option<String>, // <--- GEÄNDERT
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Saving app settings to disk...");
    let mut service = state.service.lock().unwrap();
    let bytes = serde_json::to_vec(&settings).map_err(|e| e.to_string())?;
    service.save_encrypted_data(SETTINGS_KEY, &bytes, password.as_deref())?;

    *state.settings.lock().unwrap() = Some(settings);
    info!("App settings saved and cache updated successfully.");
    Ok(())
}

#[tauri::command]
pub fn create_new_voucher(
    standard_toml_content: String,
    data: FrontendNewVoucherData,
    password: Option<String>, // <--- GEÄNDERT
    state: tauri::State<AppState>,
) -> Result<Voucher, String> {
    info!("Attempting to create a new voucher...");
    let mut service = state.service.lock().unwrap();
    let lang_preference = "en-US";
    let user_id = service.get_user_id()?;

    let voucher_data = NewVoucherData {
        nominal_value: voucher_lib::models::voucher::ValueDefinition {
            amount: data.nominal_value.amount,
            unit: data.nominal_value.unit,
            ..Default::default()
        },
        collateral: Some(voucher_lib::models::voucher::Collateral {
            value: voucher_lib::models::voucher::ValueDefinition {
                amount: data.collateral.amount,
                unit: data.collateral.unit,
                ..Default::default()
            },
            // collateral_type and redeem_condition are covered by ..Default::default()
            ..Default::default()
        }),
        creator_profile: voucher_lib::models::profile::PublicProfile {
            id: Some(user_id),
            first_name: Some(data.creator.first_name),
            last_name: Some(data.creator.last_name),
            address: Some(voucher_lib::models::voucher::Address {
                street: data.creator.address.street,
                house_number: data.creator.address.house_number,
                zip_code: data.creator.address.zip_code,
                city: data.creator.address.city,
                country: data.creator.address.country,
                full_address: data.creator.address.full_address,
            }),
            organization: data.creator.organization,
            community: data.creator.community,
            phone: data.creator.phone,
            email: data.creator.email,
            url: data.creator.url,
            gender: Some(data.creator.gender),
            coordinates: Some(data.creator.coordinates),
            service_offer: data.creator.service_offer,
            needs: data.creator.needs,
            ..Default::default()
        },
        validity_duration: data.validity_duration,
        non_redeemable_test_voucher: data.non_redeemable_test_voucher,
    };

    service.create_new_voucher(&standard_toml_content, lang_preference, voucher_data, password.as_deref())
}