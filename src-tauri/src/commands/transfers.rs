// src-tauri/src/commands/transfers.rs
use crate::{
    models::{
        CreateBundleResult, FrontendCheckoutReleaseVerdict, FrontendError, FrontendSourceTransfer,
        FrontendTransferPlan, ReceiveSuccessPayload, TransactionRecord,
    },
    services::transfer_service::{
        checkout_received_bundle_impl, create_transfer_bundle_impl, evaluate_checkout_release_impl,
        get_transaction_history_impl, prepare_transfer_plan_impl, receive_bundle_impl,
        save_transaction_record_impl,
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
pub fn receive_bundle(
    app: tauri::AppHandle,
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    force_accept_tolerance_bundle: bool,
    state: tauri::State<AppState>,
) -> Result<ReceiveSuccessPayload, FrontendError> {
    receive_bundle_impl(
        Some(&app),
        bundle_data,
        standard_definitions_toml,
        password,
        force_accept_tolerance_bundle,
        &state,
    )
}

#[tauri::command]
pub fn save_transaction_record(
    record: TransactionRecord,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    save_transaction_record_impl(record, password, &state)
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

/// Evaluates an untrusted IPC checkout outcome string (Z01 fail-closed boundary):
/// caller-supplied strings can NEVER mint `Confirmed` or authorize automatic release.
#[tauri::command]
pub fn evaluate_checkout_release(
    outcome: String,
    explicit_merchant_approval: bool,
    _state: tauri::State<AppState>,
) -> Result<FrontendCheckoutReleaseVerdict, FrontendError> {
    evaluate_checkout_release_impl(outcome, explicit_merchant_approval)
}

/// Productive purchase path: Sync Lock committee fan-out + atomic wallet/history commit.
#[tauri::command]
pub async fn checkout_received_bundle(
    purchase_id: String,
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<crate::checkout::PurchaseCompletion, FrontendError> {
    checkout_received_bundle_impl(
        purchase_id,
        bundle_data,
        standard_definitions_toml,
        password,
        &state,
    )
    .await
}
