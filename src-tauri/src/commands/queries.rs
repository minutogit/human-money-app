use crate::AppState;
use voucher_lib::{
    models::voucher::Voucher,
    wallet::{AggregatedBalance, VoucherSummary},
};

#[tauri::command]
pub fn get_user_id(state: tauri::State<AppState>) -> Result<String, String> {
    let service = state.service.lock().unwrap();
    service.get_user_id()
}

#[tauri::command]
pub fn get_total_balance_by_currency(state: tauri::State<AppState>) -> Result<Vec<AggregatedBalance>, String> {
    let service = state.service.lock().unwrap();
    service.get_total_balance_by_currency()
}

#[tauri::command]
pub fn get_voucher_summaries(state: tauri::State<AppState>) -> Result<Vec<VoucherSummary>, String> {
    let service = state.service.lock().unwrap();
    // The robust filtering is handled in the frontend; the backend's job is to fetch all summaries.
    service.get_voucher_summaries(None, None)
}

#[tauri::command]
pub fn get_voucher_details(local_id: String, state: tauri::State<AppState>) -> Result<Voucher, String> {
    let service = state.service.lock().unwrap();
    service.get_voucher_details(&local_id).map(|details| details.voucher)
}
