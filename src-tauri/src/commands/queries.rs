use crate::{AppState, models::{FrontendUserProfile, FrontendAddressData}};
use human_money_core::{
    models::voucher::Voucher,
    wallet::{AggregatedBalance, VoucherSummary},
};

#[tauri::command]
pub fn get_user_profile(state: tauri::State<AppState>) -> Result<FrontendUserProfile, String> {
    let service = state.service.lock().unwrap();
    let profile = service.get_public_profile()?;
    
    Ok(FrontendUserProfile {
        id: profile.id,
        first_name: profile.first_name,
        last_name: profile.last_name,
        organization: profile.organization,
        community: profile.community,
        address: profile.address.map(|a| FrontendAddressData {
            street: a.street,
            house_number: a.house_number,
            zip_code: a.zip_code,
            city: a.city,
            country: a.country,
            full_address: a.full_address,
        }),
        gender: profile.gender,
        email: profile.email,
        phone: profile.phone,
        coordinates: profile.coordinates,
        url: profile.url,
        service_offer: profile.service_offer,
        needs: profile.needs,
    })
}

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
