use crate::{
    models::{
        BackupProtectionMode, FrontendAutoBackupStatusDto, FrontendBackupInspectionDto,
        FrontendBackupProbeDto, FrontendBackupSummaryDto, FrontendBackupSyncResult, FrontendError,
        FrontendRestoreOutcomeDto, FrontendRestoreReconcileResultDto, MnemonicIdentityPreviewDto,
        MnemonicLanguage, SyncBackupProbeResultDto,
    },
    services::backup_service,
    AppState,
};

#[tauri::command]
pub fn probe_wallet_backup(source_path: String) -> Result<FrontendBackupProbeDto, FrontendError> {
    backup_service::probe_wallet_backup_impl(&source_path)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn export_wallet_backup_file(
    destination_path: String,
    protection_mode: BackupProtectionMode,
    backup_password: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendBackupSummaryDto, FrontendError> {
    backup_service::export_wallet_backup_file_impl(
        &destination_path,
        protection_mode,
        backup_password.as_deref(),
        password.as_deref(),
        &state,
        &app,
    )
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn sync_wallet_backup_folder(
    folder_path: String,
    protection_mode: BackupProtectionMode,
    backup_password: Option<String>,
    password: Option<String>,
    force_full: bool,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendBackupSummaryDto, FrontendError> {
    backup_service::sync_wallet_backup_folder_impl(
        folder_path,
        protection_mode,
        backup_password.as_deref(),
        password.as_deref(),
        force_full,
        &state,
        &app,
    )
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn inspect_wallet_backup(
    source_path: String,
    credential_mode: String,
    backup_password: Option<String>,
    mnemonic: Option<String>,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendBackupInspectionDto, FrontendError> {
    backup_service::inspect_wallet_backup_impl(
        &source_path,
        &credential_mode,
        backup_password.as_deref(),
        mnemonic.as_deref(),
        passphrase.as_deref(),
        user_prefix.as_deref(),
        language,
        &state,
        &app,
    )
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn restore_wallet_backup(
    source_path: String,
    credential_mode: String,
    backup_password: Option<String>,
    mnemonic: Option<String>,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    new_password: String,
    local_instance_id: String,
    replace_folder_name: Option<String>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendRestoreOutcomeDto, FrontendError> {
    backup_service::restore_wallet_backup_impl(
        &source_path,
        &credential_mode,
        backup_password.as_deref(),
        mnemonic.as_deref(),
        passphrase.as_deref(),
        user_prefix.as_deref(),
        language,
        &new_password,
        local_instance_id,
        replace_folder_name.as_deref(),
        &state,
        &app,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn reset_local_password_via_backup(
    target_folder_name: String,
    source_path: String,
    credential_mode: String,
    backup_password: Option<String>,
    mnemonic: Option<String>,
    passphrase: Option<String>,
    language: Option<MnemonicLanguage>,
    new_password: String,
    local_instance_id: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), FrontendError> {
    backup_service::reset_local_password_via_backup_for_app_impl(
        &target_folder_name,
        &source_path,
        &credential_mode,
        backup_password.as_deref(),
        mnemonic.as_deref(),
        passphrase.as_deref(),
        language,
        &new_password,
        local_instance_id,
        &state,
        &app,
    )
}

#[tauri::command]
pub fn get_backup_status(
    state: tauri::State<AppState>,
) -> Result<FrontendAutoBackupStatusDto, FrontendError> {
    backup_service::get_backup_status_impl(&state)
}

#[tauri::command]
pub fn run_auto_backup_sync(
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendAutoBackupStatusDto, FrontendError> {
    backup_service::run_auto_backup_sync_impl(&state, &app)
}

#[tauri::command]
pub async fn sync_backup(
    password: Option<String>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendBackupSyncResult, FrontendError> {
    backup_service::sync_backup_impl(password, &state, &app).await
}

#[tauri::command]
pub fn preview_mnemonic_identity(
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<MnemonicIdentityPreviewDto, FrontendError> {
    backup_service::preview_mnemonic_identity_for_app_impl(
        &mnemonic,
        passphrase.as_deref(),
        user_prefix.as_deref(),
        language,
        &state,
        &app,
    )
}

#[tauri::command]
pub fn stage_dropped_backup_bytes(
    bytes: Vec<u8>,
    app: tauri::AppHandle,
) -> Result<String, FrontendError> {
    backup_service::stage_dropped_backup_bytes_impl(&bytes, &app)
}

#[tauri::command]
pub async fn probe_sync_backup(
    sync_inputs: Vec<String>,
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<SyncBackupProbeResultDto, FrontendError> {
    backup_service::probe_sync_backup_for_app_impl(
        sync_inputs,
        mnemonic,
        passphrase,
        user_prefix,
        language,
        &state,
        &app,
    )
    .await
}

#[tauri::command]
pub async fn reconcile_restored_wallet_l2(
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendRestoreReconcileResultDto, FrontendError> {
    backup_service::reconcile_restored_wallet_l2_impl(password, &state).await
}
