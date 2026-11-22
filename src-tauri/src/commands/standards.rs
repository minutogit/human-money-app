use crate::models::{FrontendError, FrontendSignatureImpact, VoucherStandardDefinitionDto};
use crate::services::fs_helpers::{is_safe_filename_component, read_regular_file_bounded};
use crate::state::RequiredAccess;
use crate::AppState;
use human_money_core::models::secure_container::ContainerConfig;
use log::info;
use std::path::Path;
use tauri::Manager;

use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;

const MAX_STANDARD_TOML_BYTES: usize = 1024 * 1024;

#[tauri::command]
pub fn parse_standard_toml(
    toml_content: String,
    _state: tauri::State<AppState>,
) -> Result<VoucherStandardDefinitionDto, FrontendError> {
    let (core_standard, _) =
        VoucherStandardDefinition::from_toml(&toml_content).map_err(FrontendError::from)?;
    Ok(core_standard.into())
}

#[tauri::command]
pub fn evaluate_signature_suitability(
    voucher: crate::models::FrontendVoucher,
    role: String,
    standard_toml_content: String,
    state: tauri::State<AppState>,
) -> Result<FrontendSignatureImpact, FrontendError> {
    info!("Evaluating signature suitability for role: {}", role);
    let service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
    let core_voucher = human_money_core::models::voucher::Voucher::try_from(voucher)?;
    let impact = service
        .evaluate_signature_suitability(&core_voucher, &role, &standard_toml_content)
        .map_err(FrontendError::from)?;
    Ok(impact.into())
}

pub(crate) fn export_voucher_standard_impl(
    standard_id: &str,
    config: ContainerConfig,
    wallet_password: Option<&str>,
    standards_dir: &Path,
    state: &AppState,
) -> Result<Vec<u8>, FrontendError> {
    let (service, resolved_pw) =
        state.authorize_and_lock_secret(RequiredAccess::DailyPay, wallet_password)?;
    if !is_safe_filename_component(standard_id) {
        return Err(FrontendError::from(format!(
            "Invalid standard ID '{}'",
            standard_id
        )));
    }
    let toml_path = standards_dir.join(standard_id).join("standard.toml");
    if !toml_path.exists() {
        return Err(FrontendError::from(format!(
            "Standard file standard.toml not found for ID '{}'",
            standard_id
        )));
    }
    let toml_bytes = read_regular_file_bounded(&toml_path, MAX_STANDARD_TOML_BYTES)?;
    let toml_content = String::from_utf8(toml_bytes)
        .map_err(|e| FrontendError::from(format!("Standard file is not valid UTF-8: {}", e)))?;

    service
        .export_voucher_standard(
            &toml_content,
            config,
            resolved_pw.as_deref().map(String::as_str),
        )
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn export_voucher_standard(
    standard_id: String,
    config: ContainerConfig,
    wallet_password: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    info!("Exporting voucher standard '{}'...", standard_id);
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let standards_dir = app_data_dir.join("voucher_standards");
    export_voucher_standard_impl(
        &standard_id,
        config,
        wallet_password.as_deref(),
        &standards_dir,
        &state,
    )
}

#[tauri::command]
pub fn inspect_voucher_standard_file(
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<VoucherStandardDefinitionDto, FrontendError> {
    info!("Inspecting voucher standard file container...");
    let service = state.lock_service()?;
    let core_def = service
        .inspect_voucher_standard_container(&container_bytes, password.as_deref())
        .map_err(FrontendError::from)?;
    Ok(core_def.into())
}

pub(crate) fn import_voucher_standard_impl(
    container_bytes: &[u8],
    password: Option<&str>,
    wallet_password: Option<&str>,
    standards_dir: &Path,
    state: &AppState,
) -> Result<String, FrontendError> {
    let (service, resolved_pw) =
        state.authorize_and_lock_secret(RequiredAccess::Full, wallet_password)?;
    if !standards_dir.exists() {
        std::fs::create_dir_all(standards_dir).map_err(|e| {
            FrontendError::from(format!("Failed to create standards directory: {}", e))
        })?;
    }
    service
        .import_voucher_standard(
            container_bytes,
            password,
            resolved_pw.as_deref().map(String::as_str),
            standards_dir,
        )
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn import_voucher_standard(
    container_bytes: Vec<u8>,
    password: Option<String>,
    wallet_password: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    info!("Importing voucher standard from container file...");
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let standards_dir = app_data_dir.join("voucher_standards");
    import_voucher_standard_impl(
        &container_bytes,
        password.as_deref(),
        wallet_password.as_deref(),
        &standards_dir,
        &state,
    )
}

pub(crate) fn delete_voucher_standard_impl(
    standard_id: &str,
    wallet_password: Option<&str>,
    standards_dir: &Path,
    state: &AppState,
) -> Result<(), FrontendError> {
    let (service, resolved_pw) =
        state.authorize_and_lock_secret(RequiredAccess::Full, wallet_password)?;
    if !is_safe_filename_component(standard_id) {
        return Err(FrontendError::from(format!(
            "Invalid standard ID '{}'",
            standard_id
        )));
    }
    service
        .delete_voucher_standard(
            standard_id,
            resolved_pw.as_deref().map(String::as_str),
            standards_dir,
        )
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn delete_voucher_standard(
    standard_id: String,
    wallet_password: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    info!("Attempting to delete voucher standard '{}'...", standard_id);
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let standards_dir = app_data_dir.join("voucher_standards");
    delete_voucher_standard_impl(
        &standard_id,
        wallet_password.as_deref(),
        &standards_dir,
        &state,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use human_money_core::test_utils::{self, ACTORS};

    #[test]
    fn s06_f2_f17_standards_authorize_before_fs_and_reject_unsafe_ids() {
        let dir = crate::test_support::temp_test_dir("s06_f2_f17_standards");
        let pw = "pw-s06-std";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.logout();
        let state = AppState::new(svc);

        // Locked wallet MUST fail authorization BEFORE creating standards_dir or probing files
        let standards_dir = dir.join("uncreated_standards_dir");
        let import_err =
            import_voucher_standard_impl(b"invalid", None, None, &standards_dir, &state)
                .unwrap_err();
        assert!(
            !standards_dir.exists(),
            "S06-F2: import_voucher_standard must NOT create directory before authorization (err={:?})",
            import_err
        );

        let export_locked = export_voucher_standard_impl(
            "../secret",
            ContainerConfig::Cleartext,
            None,
            &standards_dir,
            &state,
        )
        .unwrap_err();
        assert_ne!(
            export_locked.message, "Invalid standard ID '../secret'",
            "S06-F2: export_voucher_standard must reject locked wallet BEFORE touching filesystem/ID validation"
        );

        // Unlock wallet and verify unsafe standard IDs are rejected by BOTH export and delete (S06-F17)
        {
            let mut s = state.lock_service().unwrap();
            let folder = s.list_profiles().unwrap()[0].folder_name.clone();
            s.login(&folder, pw, false, "test-id".into()).unwrap();
            s.unlock_session(pw, 600).unwrap();
        }
        for bad_id in ["../evil", "a/b", "a\\b", "a:b", "", ".", "..", " leading"] {
            let exp_err = export_voucher_standard_impl(
                bad_id,
                ContainerConfig::Cleartext,
                Some(pw),
                &standards_dir,
                &state,
            )
            .unwrap_err();
            assert!(
                exp_err.message.contains("Invalid standard ID"),
                "S06-F17: export must reject unsafe ID {:?}, got {:?}",
                bad_id,
                exp_err
            );
            let del_err =
                delete_voucher_standard_impl(bad_id, Some(pw), &standards_dir, &state).unwrap_err();
            assert!(
                del_err.message.contains("Invalid standard ID"),
                "S06-F17: delete must reject unsafe ID {:?}, got {:?}",
                bad_id,
                del_err
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
