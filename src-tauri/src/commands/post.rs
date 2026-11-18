use tauri::State;

use crate::models::{
    FrontendError, FrontendMarketInquiryItem, FrontendPostDepositResult, FrontendPostInboxSummary,
    FrontendPostSyncResult, FrontendSyncDescriptor,
};
use crate::services::post_service::{
    accept_wot_handshake_impl, deposit_transfer_to_post_impl, dismiss_pending_post_item_impl,
    get_post_inbox_summary_impl, mark_inquiry_read_impl, reply_detached_signature_via_post_impl,
    send_market_inquiry_message_via_post_impl, send_market_inquiry_via_post_impl,
    send_signing_request_via_post_impl, send_wot_handshake_via_post_impl, sync_post_channels_impl,
};
use crate::state::AppState;

#[tauri::command]
pub async fn deposit_transfer_to_post(
    bundle_id: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    deposit_transfer_to_post_impl(bundle_id, password, &state).await
}

#[tauri::command]
pub async fn send_signing_request_via_post(
    recipient_did: String,
    container_bytes: Vec<u8>,
    local_instance_id: Option<String>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    send_signing_request_via_post_impl(
        recipient_did,
        container_bytes,
        local_instance_id,
        password,
        &state,
    )
    .await
}

#[tauri::command]
pub async fn reply_detached_signature_via_post(
    request_msg_id: Option<String>,
    recipient_did: String,
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    reply_detached_signature_via_post_impl(
        request_msg_id,
        recipient_did,
        container_bytes,
        password,
        &state,
    )
    .await
}

#[tauri::command]
pub async fn send_wot_handshake_via_post(
    recipient_did: String,
    proposed_level: u8,
    reply_requested: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    send_wot_handshake_via_post_impl(
        recipient_did,
        proposed_level,
        reply_requested,
        password,
        &state,
    )
    .await
}

#[tauri::command]
pub async fn accept_wot_handshake(
    msg_id: String,
    trust_level: u8,
    send_return_handshake: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    accept_wot_handshake_impl(msg_id, trust_level, send_return_handshake, password, &state).await
}

#[tauri::command]
pub async fn send_market_inquiry_via_post(
    recipient_did: String,
    target: humoco_market::InquiryTarget,
    message: String,
    recipient_syncs: Vec<FrontendSyncDescriptor>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    send_market_inquiry_via_post_impl(
        recipient_did,
        target,
        message,
        recipient_syncs,
        password,
        &state,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn send_market_inquiry_message_via_post(
    inquiry_id: String,
    text: Option<String>,
    decision: Option<String>,
    payment_uri: Option<String>,
    amount_hint: Option<String>,
    standard_uuid: Option<String>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    send_market_inquiry_message_via_post_impl(
        inquiry_id,
        text,
        decision,
        payment_uri,
        amount_hint,
        standard_uuid,
        password,
        &state,
    )
    .await
}

#[tauri::command]
pub fn mark_inquiry_read(
    inquiry_id: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    mark_inquiry_read_impl(inquiry_id, password, &state)
}

#[tauri::command]
pub fn dismiss_pending_post_item(
    item_kind: String,
    id: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    dismiss_pending_post_item_impl(item_kind, id, password, &state)
}

#[tauri::command]
pub fn get_post_inbox_summary(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostInboxSummary, FrontendError> {
    get_post_inbox_summary_impl(password, &state)
}

#[tauri::command]
pub async fn sync_post_channels(
    app: tauri::AppHandle,
    is_wifi: Option<bool>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostSyncResult, FrontendError> {
    sync_post_channels_impl(Some(&app), is_wifi, None, password, &state).await
}
