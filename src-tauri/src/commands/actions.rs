// src-tauri/src/commands/actions.rs
use crate::{models::FrontendNewVoucherData, AppState};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use voucher_lib::{
    archive::VoucherArchive, // Korrigierter Importpfad
    models::voucher::Voucher,
    services::voucher_manager::NewVoucherData,
    wallet::{MultiTransferRequest, SourceTransfer}, // Korrigierter Importpfad
};

#[derive(Deserialize)]
pub struct FrontendSourceTransfer {
    local_instance_id: String,
    amount_to_send: String,
}

#[derive(Deserialize, Serialize)]
pub struct TransactionRecord {
    id: String,
    direction: String, // "sent" or "received"
    recipient_id: String,
    sender_id: String,
    timestamp: String, // ISO 8601
    total_amount_by_unit: HashMap<String, String>,
    involved_vouchers: Vec<String>, // local_instance_ids
    bundle_data: Vec<u8>,
}

#[tauri::command]
pub fn create_transfer_bundle(
    recipient_id: String,
    sources: Vec<FrontendSourceTransfer>,
    notes: Option<String>,
    // NEU: Die Standard-Definitionen müssen vom Frontend übergeben werden.
    standard_definitions_toml: HashMap<String, String>,
    password: String,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, String> {
    info!(
        "Attempting to create a transfer bundle for recipient: {}",
        recipient_id
    );
    let mut service = state.0.lock().unwrap();

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

    // ENTFERNT: Das manuelle Laden der Standards. Dies ist nicht die Aufgabe des Befehls.
    // Der Befehl verlässt sich nun auf die vom Frontend bereitgestellten Daten.

    // Das Archiv wird im Prototyp nicht verwendet.
    let archive: Option<&dyn VoucherArchive> = None;

    service.create_transfer_bundle(request, &standard_definitions_toml, archive, &password)
}

const TRANSACTION_HISTORY_KEY: &str = "transaction_history";

#[tauri::command]
pub fn save_transaction_record(
    record: TransactionRecord,
    password: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Saving new transaction record with id: {}", record.id);
    // KORRIGIERT: `service` ist jetzt als `mut` deklariert.
    let mut service = state.0.lock().unwrap();

    // 1. Lade die bestehende History (falls vorhanden)
    let mut history: Vec<TransactionRecord> =
        match service.load_encrypted_data(TRANSACTION_HISTORY_KEY, &password) {
            Ok(data) => serde_json::from_slice(&data).unwrap_or_else(|e| {
                error!(
                    "Failed to parse existing transaction history, starting fresh: {}",
                    e
                );
                Vec::new()
            }),
            Err(_) => {
                info!("No existing transaction history found. Creating a new one.");
                Vec::new()
            }
        };

    // 2. Füge den neuen Record hinzu
    history.push(record);

    // 3. Speichere die aktualisierte History
    let history_bytes = serde_json::to_vec(&history).map_err(|e| e.to_string())?;
    service.save_encrypted_data(TRANSACTION_HISTORY_KEY, &history_bytes, &password)
}

#[tauri::command]
pub fn load_transaction_history(
    password: String,
    state: tauri::State<AppState>,
) -> Result<Vec<TransactionRecord>, String> {
    info!("Loading transaction history...");
    let service = state.0.lock().unwrap();
    let data = service.load_encrypted_data(TRANSACTION_HISTORY_KEY, &password)?;
    serde_json::from_slice(&data).map_err(|e| {
        error!("Failed to parse transaction history data: {}", e);
        e.to_string()
    })
}

#[tauri::command]
pub fn create_new_voucher(
    standard_toml_content: String,
    data: FrontendNewVoucherData,
    password: String,
    state: tauri::State<AppState>,
) -> Result<Voucher, String> {
    info!("Attempting to create a new voucher...");
    let mut service = state.0.lock().unwrap();
    // For the prototype, language preference is hardcoded.
    let lang_preference = "en-US";

    // Get the current user's ID to use as the creator ID. This fixes the "Invalid creator ID" error.
    let user_id = service.get_user_id()?;

    // Convert the frontend data struct to the library's data struct
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