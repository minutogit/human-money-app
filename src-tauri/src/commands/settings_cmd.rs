use crate::{
    models::{FrontendError, ReachPresetDto},
    services::settings_service::{
        apply_reach_preset_impl, get_app_config_impl, get_reach_preset_impl,
        patch_security_policy_impl, patch_ui_preferences_impl, set_conflict_local_override_impl,
    },
    settings::{
        AppConfigDto, SecurityPolicy, SecurityPolicyPatch, UiPreferences, UiPreferencesPatch,
    },
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
pub fn get_app_config(state: tauri::State<AppState>) -> Result<AppConfigDto, FrontendError> {
    get_app_config_impl(&state)
}

#[tauri::command]
pub fn patch_ui_preferences(
    patch: UiPreferencesPatch,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<UiPreferences, FrontendError> {
    patch_ui_preferences_impl(patch, password, &state)
}

#[tauri::command]
pub fn patch_security_policy(
    patch: SecurityPolicyPatch,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<SecurityPolicy, FrontendError> {
    patch_security_policy_impl(patch, password, &state)
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
