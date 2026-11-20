// src-tauri/src/commands/transfers.rs
use crate::{
    models::{
        CreateBundleResult, FrontendError, FrontendSourceTransfer, FrontendTransferPlan,
        ReceiveCommitMode, ReceiveSuccessPayload, TransactionRecord,
    },
    services::transfer_service::{
        create_transfer_bundle_impl, get_transaction_history_impl, prepare_transfer_plan_impl,
        receive_bundle_impl,
    },
    AppState,
};
use std::collections::HashMap;

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_transfer_bundle(
    app: tauri::AppHandle,
    recipient_id: String,
    sources: Vec<FrontendSourceTransfer>,
    notes: Option<String>,
    sender_profile_name: Option<String>,
    standard_definitions_toml: HashMap<String, String>,
    use_privacy_mode: Option<bool>,
    send_card_override: Option<bool>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<CreateBundleResult, FrontendError> {
    create_transfer_bundle_impl(
        Some(&app),
        recipient_id,
        sources,
        notes,
        sender_profile_name,
        standard_definitions_toml,
        use_privacy_mode,
        send_card_override,
        password,
        &state,
    )
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn receive_bundle(
    app: tauri::AppHandle,
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    force_accept_tolerance_bundle: bool,
    commit_mode: Option<ReceiveCommitMode>,
    purchase_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ReceiveSuccessPayload, FrontendError> {
    receive_bundle_impl(
        Some(&app),
        bundle_data,
        standard_definitions_toml,
        password,
        force_accept_tolerance_bundle,
        commit_mode,
        purchase_id,
        &state,
    )
    .await
}

#[tauri::command]
pub fn get_transaction_history(
    state: tauri::State<AppState>,
) -> Result<Vec<TransactionRecord>, FrontendError> {
    get_transaction_history_impl(&state)
}

#[tauri::command]
pub fn prepare_transfer_plan(
    target_amount: String,
    standard_uuid_filter: Option<String>,
    recipient_id: String,
    is_test_voucher: Option<bool>,
    state: tauri::State<AppState>,
) -> Result<FrontendTransferPlan, FrontendError> {
    prepare_transfer_plan_impl(
        target_amount,
        standard_uuid_filter,
        recipient_id,
        is_test_voucher,
        &state,
    )
}
