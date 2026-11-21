use humoco_market::RecoveryKdfParams;
use tauri::{Manager, State};

use crate::models::{
    ActiveRecoverySessionDto, ApproveGuardianRecoveryResultDto, ConfirmEmergencySheetResultDto,
    ExportedMnemonicKitDto, FrontendError, GuardianRecoveryRequestInspectionDto,
    InspectedEmergencyKitDto, RecoveryCompletionResultDto, RecoveryProtectionStatusDto,
    RehearsalCheckResultDto, RevokeSocialRecoveryResultDto, SetupSocialRecoveryResultDto,
};
use crate::services::recovery_service;
use crate::state::AppState;

#[tauri::command]
pub fn inspect_emergency_kit_uri(uri: String) -> Result<InspectedEmergencyKitDto, FrontendError> {
    recovery_service::inspect_emergency_kit_uri_impl(uri)
}

#[tauri::command]
pub fn export_profile_mnemonic(
    password: String,
    state: State<'_, AppState>,
) -> Result<Option<ExportedMnemonicKitDto>, FrontendError> {
    recovery_service::export_profile_mnemonic_impl(password, &state)
}

#[tauri::command]
pub async fn confirm_emergency_sheet_saved(
    password: String,
    word_index: u8,
    control_word: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ConfirmEmergencySheetResultDto, FrontendError> {
    recovery_service::confirm_emergency_sheet_saved_impl(
        password,
        word_index,
        control_word,
        &state,
        Some(&app),
    )
    .await
}

#[tauri::command]
pub async fn verify_recovery_rehearsal(
    answers: [String; 3],
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<RehearsalCheckResultDto, FrontendError> {
    let now_secs = crate::sync_client::current_unix_timestamp();
    recovery_service::verify_recovery_rehearsal_with_password_impl(
        answers,
        now_secs,
        password.as_deref(),
        &state,
    )
    .await
}

#[tauri::command]
pub fn snooze_recovery_rehearsal(
    hours: Option<u64>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<u64, FrontendError> {
    let now_secs = crate::sync_client::current_unix_timestamp();
    recovery_service::snooze_recovery_rehearsal_with_password_impl(
        hours,
        now_secs,
        password.as_deref(),
        &state,
    )
}

#[tauri::command]
pub fn get_recovery_protection_status(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<RecoveryProtectionStatusDto, FrontendError> {
    recovery_service::get_recovery_protection_status_impl(password.as_deref(), &state)
}

#[tauri::command]
pub async fn setup_social_recovery_guardians(
    password: String,
    guardian_dids: Vec<String>,
    threshold_m: u8,
    question_ids: [u8; 3],
    answers: [String; 3],
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<SetupSocialRecoveryResultDto, FrontendError> {
    recovery_service::setup_social_recovery_guardians_with_params(
        password,
        guardian_dids,
        threshold_m,
        question_ids,
        answers,
        RecoveryKdfParams::new_production(),
        &state,
        Some(&app),
    )
    .await
}

#[tauri::command]
pub async fn revoke_social_recovery(
    password: Option<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RevokeSocialRecoveryResultDto, FrontendError> {
    recovery_service::revoke_social_recovery_impl(password, &state, Some(&app)).await
}

#[tauri::command]
pub async fn start_social_recovery_session(
    name_hint: Option<String>,
    session_mode: Option<String>,
    target_folder_name: Option<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let _recovery_guard = state.recovery_op_lock.lock().await;
    let session_path = recovery_service::get_session_file_path(&app)?;
    recovery_service::start_social_recovery_session_impl(
        name_hint,
        session_mode,
        target_folder_name,
        &session_path,
    )
}

#[tauri::command]
pub async fn get_active_recovery_session(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Option<ActiveRecoverySessionDto>, FrontendError> {
    let _recovery_guard = state.recovery_op_lock.lock().await;
    let session_path = recovery_service::get_session_file_path(&app)?;
    recovery_service::get_active_recovery_session_impl(&session_path)
}

#[tauri::command]
pub async fn cancel_social_recovery_session(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), FrontendError> {
    let _recovery_guard = state.recovery_op_lock.lock().await;
    let session_path = recovery_service::get_session_file_path(&app)?;
    recovery_service::cancel_social_recovery_session_impl(&session_path)
}

#[tauri::command]
pub async fn import_guardian_recovery_reply(
    reply_uri: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let _recovery_guard = state.recovery_op_lock.lock().await;
    let session_path = recovery_service::get_session_file_path(&app)?;
    recovery_service::import_guardian_recovery_reply_impl(reply_uri, &session_path)
}

#[tauri::command]
pub fn inspect_guardian_recovery_request(
    request_uri: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<GuardianRecoveryRequestInspectionDto, FrontendError> {
    recovery_service::inspect_guardian_recovery_request_impl(
        request_uri,
        password.as_deref(),
        &state,
    )
}

#[tauri::command]
pub fn approve_guardian_recovery_request(
    request_uri: String,
    owner_did: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<ApproveGuardianRecoveryResultDto, FrontendError> {
    recovery_service::approve_guardian_recovery_request_impl(
        request_uri,
        owner_did,
        password.as_deref(),
        &state,
    )
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn complete_social_recovery(
    owner_did: String,
    rotation_epoch: u32,
    answers: [String; 3],
    bip39_passphrase: Option<String>,
    mode: String,
    new_password: String,
    profile_name: Option<String>,
    target_folder_name: Option<String>,
    local_instance_id: String,
    allow_empty_fallback: Option<bool>,
    replace_folder_name: Option<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RecoveryCompletionResultDto, FrontendError> {
    let session_path = recovery_service::get_session_file_path(&app)?;
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let wallet_data_dir = app_dir.join("wallet_data");
    recovery_service::complete_social_recovery_impl(
        owner_did,
        rotation_epoch,
        answers,
        bip39_passphrase,
        mode,
        new_password,
        profile_name,
        target_folder_name,
        local_instance_id,
        allow_empty_fallback,
        replace_folder_name,
        &session_path,
        &state,
        &app_dir,
        &wallet_data_dir,
        Some(&app),
    )
    .await
}
