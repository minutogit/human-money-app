// src-tauri/src/commands/transfers.rs
use crate::{
    models::{
        CreateBundleResult, FrontendSourceTransfer, FrontendTransactionRecord,
        ReceiveSuccessPayload, TransactionRecord, FrontendError,
    },
    AppState,
};
use chrono::Utc;
use log::{info, error};
use uuid::Uuid;
use std::collections::HashMap;
use human_money_core::{
    archive::VoucherArchive,
    wallet::{MultiTransferRequest, SourceTransfer},
};

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_transfer_bundle(
    recipient_id: String,
    sources: Vec<FrontendSourceTransfer>,
    notes: Option<String>,
    sender_profile_name: Option<String>,
    standard_definitions_toml: HashMap<String, String>,
    use_privacy_mode: Option<bool>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<CreateBundleResult, FrontendError> {
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
        use_privacy_mode,
    };

    let archive: Option<&dyn VoucherArchive> = None;

    match service.create_transfer_bundle(request, &standard_definitions_toml, archive, password.as_deref()) {
        Ok(result) => {
            if let Err(e) = state.refresh_events_cache(&mut service, password.as_deref()) {
                error!("Failed to refresh events cache after create_transfer_bundle: {}", e);
            }

            Ok(CreateBundleResult {
                bundle_data: result.bundle_bytes,
                bundle_id: result.bundle_id,
                involved_sources_details: result.involved_sources_details.into_iter().map(|s| s.into()).collect(),
            })
        }
        Err(e) => Err(FrontendError::from(e)),
    }
}

#[tauri::command]
pub fn receive_bundle(
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    force_accept_tolerance_bundle: bool,
    state: tauri::State<AppState>,
) -> Result<ReceiveSuccessPayload, FrontendError> {
    info!("Attempting to receive and process a transfer bundle...");
    let mut service = state.service.lock().unwrap();
    let archive: Option<&dyn VoucherArchive> = None;

    match service.receive_bundle(&bundle_data, &standard_definitions_toml, archive, password.as_deref(), force_accept_tolerance_bundle) {
        Ok(result) => {
            info!("Bundle processed successfully. Creating transaction record.");

            let conflict_count = result.check_result.verifiable_conflicts.len();
            let warning_count = result.check_result.unverifiable_warnings.len();

            if conflict_count > 0 || warning_count > 0 {
                info!(
                    "FRAUD DETECTION: Processed bundle from {} | Confirmed Conflicts: {} | Gossip Warnings: {}",
                    result.header.sender_id, conflict_count, warning_count
                );
            }

            let transfer_summary = result.transfer_summary.clone();
            let involved_vouchers_details = result.involved_vouchers_details.clone();
            let involved_vouchers_ids: Vec<String> = involved_vouchers_details.iter().map(|d| d.local_instance_id.clone()).collect();
            let notes = result.header.notes.clone();
            let sender_profile_name = result.header.sender_profile_name.clone();
            let bundle_id = result.header.bundle_id.clone();

            let record = TransactionRecord {
                id: Uuid::new_v4().to_string(),
                direction: "received".to_string(),
                recipient_id: service.get_user_id().map_err(FrontendError::from)?,
                sender_id: result.header.sender_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                summable_amounts: transfer_summary.summable_amounts.clone(),
                countable_items: transfer_summary.countable_items.clone(),
                involved_vouchers: involved_vouchers_ids.clone(),
                involved_sources_details: Some(involved_vouchers_details.clone()),
                bundle_data: Vec::new(),
                bundle_id,
                notes: notes.clone(),
                sender_profile_name: sender_profile_name.clone(),
            };

            state.append_to_history(&mut service, record.clone(), password.as_deref()).map_err(FrontendError::from)?;

            if let Err(e) = state.refresh_events_cache(&mut service, password.as_deref()) {
                error!("Failed to refresh events cache after receive_bundle: {}", e);
            }

            Ok(ReceiveSuccessPayload {
                sender_id: result.header.sender_id,
                sender_profile_name,
                notes,
                transfer_summary: result.transfer_summary.into(),
                involved_vouchers: involved_vouchers_ids,
                involved_vouchers_details: involved_vouchers_details.into_iter().map(|d| d.into()).collect(),
                verifiable_conflicts: result.check_result.verifiable_conflicts,
                conflict_summaries: service.list_conflicts().map_err(FrontendError::from)?.into_iter().map(|s| s.into()).collect(),
            })
        }
        Err(e) => {
            error!("Failed to process bundle: {}", e);
            Err(FrontendError::from(e))
        }
    }
}

#[tauri::command]
pub fn save_transaction_record(
    record: FrontendTransactionRecord,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    let record: TransactionRecord = record.into();
    info!("Saving new transaction record with id: {}", record.id);
    let mut service = state.service.lock().unwrap();
    state.append_to_history(&mut service, record, password.as_deref()).map_err(FrontendError::from)
}

#[tauri::command]
pub fn get_transaction_history(state: tauri::State<AppState>) -> Result<Vec<FrontendTransactionRecord>, FrontendError> {
    info!("Loading transaction history...");
    let history = state.get_cached_history().map_err(FrontendError::from)?;
    Ok(history.iter().map(|r| r.into()).collect())
}
