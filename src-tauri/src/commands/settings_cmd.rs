use crate::{
    models::{FrontendError, ReachPresetDto},
    services::settings_service::{
        apply_reach_preset_impl, get_app_settings_impl, get_reach_preset_impl,
        save_app_settings_impl, set_conflict_local_override_impl,
    },
    settings::AppSettings,
    AppState,
};

#[tauri::command]
pub fn get_reach_preset(
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<ReachPresetDto, FrontendError> {
    get_reach_preset_impl(password, &state)
}

#[tauri::command]
pub fn apply_reach_preset(
    preset: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<ReachPresetDto, FrontendError> {
    apply_reach_preset_impl(preset, password, &state)
}

#[tauri::command]
pub fn get_app_settings(state: tauri::State<AppState>) -> Result<AppSettings, FrontendError> {
    get_app_settings_impl(&state)
}

#[tauri::command]
pub async fn save_app_settings(
    settings: AppSettings,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), FrontendError> {
    save_app_settings_impl(settings, password, &state)
}

#[tauri::command]
pub fn set_conflict_local_override(
    proof_id: String,
    value: bool,
    note: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    set_conflict_local_override_impl(proof_id, value, note, password, &state)
}
