// src-tauri/src/commands/actions.rs
use crate::{models::FrontendNewVoucherData, AppState, settings::{AppSettings, SETTINGS_KEY}};
use chrono::Utc;
use log::{info, error};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use voucher_lib::{
    archive::VoucherArchive,
    app_service::AppService,
    models::voucher::Voucher,
    services::voucher_manager::NewVoucherData,
    wallet::{MultiTransferRequest, SourceTransfer},
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
    pub total_amount_by_unit: HashMap<String, String>,
    pub involved_vouchers: Vec<String>, // local_instance_ids
    pub bundle_data: Vec<u8>,
}

// NEU: Diese Struktur wird an das Frontend zurückgegeben, wenn der Empfang erfolgreich war.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveSuccessPayload {
    sender_id: String,
    total_amount_by_unit: HashMap<String, String>,
}


#[tauri::command]
pub fn create_transfer_bundle(
    recipient_id: String,
    sources: Vec<FrontendSourceTransfer>,
    notes: Option<String>,
    standard_definitions_toml: HashMap<String, String>,
    password: String,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, String> {
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
    };

    let archive: Option<&dyn VoucherArchive> = None;

    service.create_transfer_bundle(request, &standard_definitions_toml, archive, &password)
}

// NEU: Der Befehl zum Empfangen von Bundles.
#[tauri::command]
pub fn receive_bundle(
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: String,
    state: tauri::State<AppState>,
) -> Result<ReceiveSuccessPayload, String> {
    info!("Attempting to receive and process a transfer bundle...");
    let mut service = state.service.lock().unwrap();
    let archive: Option<&dyn VoucherArchive> = None;

    match service.receive_bundle(&bundle_data, &standard_definitions_toml, archive, &password) {
        Ok(result) => {
            info!("Bundle processed successfully. Creating transaction record.");

            if !result.check_result.verifiable_conflicts.is_empty() {
                let msg = "Bundle processing failed due to verifiable double-spend conflicts.".to_string();
                error!("{}", msg);
                return Err(msg);
            }

            // KORREKTE LOGIK: Da `VoucherSummary` keine globale `voucher_id` hat, müssen wir
            // über die vollen Details gehen, um die empfangenen Gutscheine zu identifizieren.
            let all_summaries = service.get_voucher_summaries(None, None)?;
            let mut received_summaries = Vec::new();

            for summary in all_summaries {
                // Rufe die Details ab, um an die globale voucher_id zu kommen.
                if let Ok(details) = service.get_voucher_details(&summary.local_instance_id) {
                    if result.header.voucher_ids.contains(&details.voucher.voucher_id) {
                        received_summaries.push(summary);
                    }
                }
            }

            let mut total_amount_by_unit: HashMap<String, String> = HashMap::new();
            for summary in &received_summaries {
                let entry = total_amount_by_unit.entry(summary.unit.clone()).or_insert_with(|| "0".to_string());
                let current_total: f64 = entry.parse().unwrap_or(0.0);
                let amount_to_add: f64 = summary.current_amount.parse().unwrap_or(0.0);
                *entry = (current_total + amount_to_add).to_string();
            }

            let involved_vouchers: Vec<String> = received_summaries
                .into_iter()
                .map(|s| s.local_instance_id.clone())
                .collect();

            let record = TransactionRecord {
                id: Uuid::new_v4().to_string(),
                direction: "received".to_string(),
                recipient_id: service.get_user_id()?, // The current user is the recipient
                sender_id: result.header.sender_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                total_amount_by_unit: total_amount_by_unit.clone(),
                involved_vouchers,
                bundle_data: Vec::new(),
            };

            // Speichere den neuen Eintrag atomar in der Historie.
            let mut history = load_history_from_disk(&service, &password)?;
            let new_record = record.clone();
            history.push(record);
            let history_bytes = serde_json::to_vec(&history).map_err(|e| e.to_string())?;
            service.save_encrypted_data(TRANSACTION_HISTORY_KEY, &history_bytes, &password)?;

            let mut history_cache = state.history.lock().unwrap();
            if let Some(cache) = history_cache.as_mut() {
                cache.push(new_record);
            } else {
                *history_cache = Some(history);
            }

            info!("Transaction record for received bundle saved successfully.");

            Ok(ReceiveSuccessPayload {
                sender_id: result.header.sender_id,
                total_amount_by_unit,
            })
        }
        Err(e) => {
            error!("Failed to process bundle: {}", e);
            Err(e)
        }
    }
}


pub const TRANSACTION_HISTORY_KEY: &str = "transaction_history";

#[tauri::command]
pub fn save_transaction_record(
    record: TransactionRecord,
    password: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Saving new transaction record with id: {}", record.id);
    let mut service = state.service.lock().unwrap();

    let mut history = load_history_from_disk(&service, &password)?;
    let new_record = record.clone();
    history.push(record);

    let history_bytes = serde_json::to_vec(&history).map_err(|e| e.to_string())?;
    service.save_encrypted_data(TRANSACTION_HISTORY_KEY, &history_bytes, &password)?;

    let mut history_cache = state.history.lock().unwrap();
    if let Some(cache) = history_cache.as_mut() {
        cache.push(new_record);
    } else {
        *history_cache = Some(history);
    }
    Ok(())
}

pub fn load_history_from_disk(
    service: &AppService,
    password: &str,
) -> Result<Vec<TransactionRecord>, String> {
    match service.load_encrypted_data(TRANSACTION_HISTORY_KEY, password) {
        Ok(data) => serde_json::from_slice(&data)
            .map_err(|e| format!("Failed to parse transaction history: {}", e)),
        Err(_) => Ok(Vec::new()),
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
    password: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Saving app settings to disk...");
    let mut service = state.service.lock().unwrap();
    let bytes = serde_json::to_vec(&settings).map_err(|e| e.to_string())?;
    service.save_encrypted_data(SETTINGS_KEY, &bytes, &password)?;

    *state.settings.lock().unwrap() = Some(settings);
    info!("App settings saved and cache updated successfully.");
    Ok(())
}

#[tauri::command]
pub fn create_new_voucher(
    standard_toml_content: String,
    data: FrontendNewVoucherData,
    password: String,
    state: tauri::State<AppState>,
) -> Result<Voucher, String> {
    info!("Attempting to create a new voucher...");
    let mut service = state.service.lock().unwrap();
    let lang_preference = "en-US";
    let user_id = service.get_user_id()?;

    let voucher_data = NewVoucherData {
        nominal_value: voucher_lib::models::voucher::NominalValue {
            amount: data.nominal_value.amount,
            unit: data.nominal_value.unit,
            ..Default::default()
        },
        collateral: voucher_lib::models::voucher::Collateral {
            amount: data.collateral.amount,
            unit: data.collateral.unit,
            abbreviation: data.collateral.abbreviation,
            ..Default::default()
        },
        creator: voucher_lib::models::voucher::Creator {
            id: user_id,
            first_name: data.creator.first_name,
            last_name: data.creator.last_name,
            address: voucher_lib::models::voucher::Address {
                street: data.creator.address.street,
                house_number: data.creator.address.house_number,
                zip_code: data.creator.address.zip_code,
                city: data.creator.address.city,
                country: data.creator.address.country,
                full_address: data.creator.address.full_address,
            },
            organization: data.creator.organization,
            community: data.creator.community,
            phone: data.creator.phone,
            email: data.creator.email,
            url: data.creator.url,
            gender: data.creator.gender,
            service_offer: data.creator.service_offer,
            needs: data.creator.needs,
            coordinates: data.creator.coordinates,
            ..Default::default()
        },
        validity_duration: data.validity_duration,
        non_redeemable_test_voucher: data.non_redeemable_test_voucher,
    };

    service.create_new_voucher(&standard_toml_content, lang_preference, voucher_data, &password)
}