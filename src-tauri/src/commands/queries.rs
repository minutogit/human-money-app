use crate::models::FrontendError;
use crate::services::balance_service::{
    check_reputation_impl, get_active_asset_classes_impl, get_double_spend_conflicts_impl,
    get_event_history_impl, get_proof_id_for_voucher_impl, get_proof_of_double_spend_impl,
    get_total_balance_by_currency_impl, get_user_id_impl, get_user_profile_impl,
    get_voucher_details_impl, get_voucher_summaries_impl,
};
use crate::AppState;

#[tauri::command]
pub fn get_active_asset_classes(
    state: tauri::State<AppState>,
) -> Result<Vec<crate::models::FrontendAssetClassSummary>, FrontendError> {
    get_active_asset_classes_impl(&state)
}

#[tauri::command]
pub fn get_user_profile(
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendUserProfile, FrontendError> {
    get_user_profile_impl(&state)
}

#[tauri::command]
pub fn get_user_id(state: tauri::State<AppState>) -> Result<String, FrontendError> {
    get_user_id_impl(&state)
}

#[tauri::command]
pub fn get_total_balance_by_currency(
    state: tauri::State<AppState>,
) -> Result<Vec<crate::models::FrontendAggregatedBalance>, FrontendError> {
    get_total_balance_by_currency_impl(&state)
}

#[tauri::command]
pub fn get_voucher_summaries(
    test_filter: Option<bool>,
    state: tauri::State<AppState>,
) -> Result<Vec<crate::models::FrontendVoucherSummary>, FrontendError> {
    get_voucher_summaries_impl(test_filter, &state)
}

#[tauri::command]
pub fn get_voucher_details(
    local_id: String,
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendVoucherDetails, FrontendError> {
    get_voucher_details_impl(local_id, &state)
}

#[tauri::command]
pub fn get_double_spend_conflicts(
    state: tauri::State<AppState>,
) -> Result<Vec<crate::models::FrontendProofOfDoubleSpendSummary>, FrontendError> {
    get_double_spend_conflicts_impl(&state)
}

#[tauri::command]
pub fn get_proof_of_double_spend(
    proof_id: String,
    state: tauri::State<AppState>,
) -> Result<crate::models::FullProofDetails, FrontendError> {
    get_proof_of_double_spend_impl(proof_id, &state)
}

#[tauri::command]
pub fn get_proof_id_for_voucher(
    local_id: String,
    state: tauri::State<AppState>,
) -> Result<Option<String>, FrontendError> {
    get_proof_id_for_voucher_impl(local_id, &state)
}

#[tauri::command]
pub fn check_reputation(
    offender_id: String,
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendTrustStatus, FrontendError> {
    check_reputation_impl(offender_id, &state)
}

#[tauri::command]
pub fn get_event_history(
    offset: usize,
    limit: usize,
    state: tauri::State<AppState>,
) -> Result<Vec<crate::models::FrontendWalletEvent>, FrontendError> {
    get_event_history_impl(offset, limit, &state)
}
