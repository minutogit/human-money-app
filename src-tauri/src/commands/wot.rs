use crate::models::{
    FrontendError, FrontendVoucherTrustDto, FrontendWotImportResultDto, FrontendWotProviderDto,
    FrontendWotSummaryDto,
};
use crate::services::wot_service::{
    evaluate_voucher_trust_preview_impl, export_wot_package_impl, get_wot_providers_impl,
    get_wot_summary_impl, import_wot_package_impl, refresh_wot_provider_url_impl,
    remove_wot_provider_impl, set_wot_max_hops_impl, set_wot_provider_trust_impl,
};
use crate::state::AppState;

#[tauri::command]
pub fn get_wot_summary(
    state: tauri::State<AppState>,
) -> Result<FrontendWotSummaryDto, FrontendError> {
    get_wot_summary_impl(&state)
}

#[tauri::command]
pub fn set_wot_max_hops(
    max_hops: u8,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendWotSummaryDto, FrontendError> {
    set_wot_max_hops_impl(max_hops, password, &state)
}

#[tauri::command]
pub fn export_wot_package(
    recipient_did: Option<String>,
    container_password: Option<String>,
    share_with_all_contacts: Option<bool>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    export_wot_package_impl(
        recipient_did,
        container_password,
        share_with_all_contacts,
        password,
        &state,
    )
}

#[tauri::command]
pub fn import_wot_package(
    container_bytes: Vec<u8>,
    container_password: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendWotImportResultDto, FrontendError> {
    import_wot_package_impl(container_bytes, container_password, password, &state)
}

#[tauri::command]
pub fn get_wot_providers(
    state: tauri::State<AppState>,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    get_wot_providers_impl(&state)
}

#[tauri::command]
pub fn set_wot_provider_trust(
    issuer_did: String,
    level: u8,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    set_wot_provider_trust_impl(issuer_did, level, password, &state)
}

#[tauri::command]
pub fn remove_wot_provider(
    issuer_did: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    remove_wot_provider_impl(issuer_did, password, &state)
}

#[tauri::command]
pub async fn refresh_wot_provider_url(
    issuer_did: String,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendWotProviderDto, FrontendError> {
    refresh_wot_provider_url_impl(issuer_did, password, &state).await
}

#[tauri::command]
pub fn evaluate_voucher_trust_preview(
    creator_id: String,
    guarantor_ids: Vec<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendVoucherTrustDto, FrontendError> {
    evaluate_voucher_trust_preview_impl(creator_id, guarantor_ids, &state)
}
