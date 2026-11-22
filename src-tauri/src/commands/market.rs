use tauri::State;

use crate::models::{
    FrontendAdInput, FrontendError, FrontendForwardCardExport, FrontendImportCardResult,
    FrontendMarketSnapshot, FrontendMyMarketProfile, FrontendSaveMyMarketProfileRequest,
    StoredAppNotices,
};
use crate::services::market_service::{
    clear_app_notices_impl, delete_ad_impl, export_forward_card_impl, get_ad_image_impl,
    get_app_notices_impl, get_market_view_impl, get_my_market_profile_impl,
    import_contact_card_impl, mark_app_notices_read_impl, renew_ad_impl,
    resolve_card_recipient_impl, save_my_market_profile_impl, set_ad_published_impl,
    set_card_published_impl, upsert_ad_impl,
};
use crate::state::AppState;

#[tauri::command]
pub fn get_my_market_profile(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    get_my_market_profile_impl(password, &state)
}

#[tauri::command]
pub fn save_my_market_profile(
    request: FrontendSaveMyMarketProfileRequest,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    save_my_market_profile_impl(request, password, &state)
}

#[tauri::command]
pub async fn set_card_published(
    published: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    set_card_published_impl(published, password, &state).await
}

#[tauri::command]
pub fn upsert_ad(
    ad: FrontendAdInput,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    upsert_ad_impl(ad, password, &state)
}

#[tauri::command]
pub fn set_ad_published(
    id: String,
    published: bool,
    also_publish_card: Option<bool>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    set_ad_published_impl(id, published, also_publish_card, password, &state)
}

#[tauri::command]
pub fn renew_ad(
    id: String,
    duration_days: u64,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    renew_ad_impl(id, duration_days, password, &state)
}

#[tauri::command]
pub fn delete_ad(
    id: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    delete_ad_impl(id, password, &state)
}

#[tauri::command]
pub async fn get_ad_image(
    owner_did: String,
    image_token: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<String>, FrontendError> {
    get_ad_image_impl(owner_did, image_token, password, &state).await
}

#[tauri::command]
pub fn get_market_view(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMarketSnapshot, FrontendError> {
    get_market_view_impl(password, &state)
}

#[tauri::command]
pub fn export_forward_card(
    did: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendForwardCardExport, FrontendError> {
    export_forward_card_impl(did, password, &state)
}

#[tauri::command]
pub fn import_contact_card(
    payload: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendImportCardResult, FrontendError> {
    import_contact_card_impl(payload, password, &state)
}

#[tauri::command]
pub fn resolve_card_recipient(
    payload: String,
    state: State<'_, AppState>,
) -> Result<crate::models::FrontendResolvedCardRecipient, FrontendError> {
    resolve_card_recipient_impl(payload, &state)
}

#[tauri::command]
pub fn get_app_notices(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<StoredAppNotices, FrontendError> {
    get_app_notices_impl(password, &state)
}

#[tauri::command]
pub fn mark_app_notices_read(
    notice_id: Option<String>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<StoredAppNotices, FrontendError> {
    mark_app_notices_read_impl(notice_id, password, &state)
}

#[tauri::command]
pub fn clear_app_notices(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<StoredAppNotices, FrontendError> {
    clear_app_notices_impl(password, &state)
}
