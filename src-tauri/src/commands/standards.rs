use crate::AppState;
use crate::models::{VoucherStandardDefinitionDto, FrontendSignatureImpact, FrontendError};
use human_money_core::models::secure_container::ContainerConfig;
use log::info;
use tauri::Manager;

#[tauri::command]
pub fn parse_standard_toml(
    toml_content: String,
    state: tauri::State<AppState>,
) -> Result<VoucherStandardDefinitionDto, FrontendError> {
    let service = state.service.lock().unwrap();
    let core_standard = service.parse_voucher_standard(&toml_content).map_err(FrontendError::from)?;
    Ok(core_standard.into())
}

#[tauri::command]
pub fn get_allowed_signature_roles_from_standard(
    toml_content: String,
    state: tauri::State<AppState>,
) -> Result<Vec<String>, FrontendError> {
    info!("Getting allowed signature roles from standard...");
    let service = state.service.lock().unwrap();
    service.get_allowed_signature_roles_from_standard(&toml_content).map_err(FrontendError::from)
}

#[tauri::command]
pub fn evaluate_signature_suitability(
    voucher: crate::models::FrontendVoucher,
    role: String,
    standard_toml_content: String,
    state: tauri::State<AppState>,
) -> Result<FrontendSignatureImpact, FrontendError> {
    info!("Evaluating signature suitability for role: {}", role);
    let service = state.service.lock().unwrap();
    let core_voucher = human_money_core::models::voucher::Voucher::from(voucher);
    let impact = service.evaluate_signature_suitability(&core_voucher, &role, &standard_toml_content).map_err(FrontendError::from)?;
    Ok(impact.into())
}

#[tauri::command]
pub fn export_voucher_standard(
    standard_id: String,
    config: ContainerConfig,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    info!("Exporting voucher standard '{}'...", standard_id);
    let app_data_dir = app.path().app_data_dir().map_err(|e| FrontendError::from(e.to_string()))?;
    let toml_path = app_data_dir.join("voucher_standards").join(&standard_id).join("standard.toml");
    if !toml_path.exists() {
        return Err(FrontendError::from(format!("Standard file standard.toml not found for ID '{}'", standard_id)));
    }
    let toml_content = std::fs::read_to_string(&toml_path)
        .map_err(|e| FrontendError::from(format!("Failed to read standard file: {}", e)))?;

    let service = state.service.lock().unwrap();
    service.export_voucher_standard(&toml_content, config).map_err(FrontendError::from)
}

#[tauri::command]
pub fn inspect_voucher_standard_file(
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<VoucherStandardDefinitionDto, FrontendError> {
    info!("Inspecting voucher standard file container...");
    let service = state.service.lock().unwrap();
    let core_def = service
        .inspect_voucher_standard_container(&container_bytes, password.as_deref())
        .map_err(FrontendError::from)?;
    Ok(core_def.into())
}

#[tauri::command]
pub fn import_voucher_standard(
    container_bytes: Vec<u8>,
    password: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    info!("Importing voucher standard from container file...");
    let app_data_dir = app.path().app_data_dir().map_err(|e| FrontendError::from(e.to_string()))?;
    let standards_dir = app_data_dir.join("voucher_standards");
    if !standards_dir.exists() {
        std::fs::create_dir_all(&standards_dir)
            .map_err(|e| FrontendError::from(format!("Failed to create standards directory: {}", e)))?;
    }

    let service = state.service.lock().unwrap();
    service
        .import_voucher_standard(&container_bytes, password.as_deref(), &standards_dir)
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn delete_voucher_standard(
    standard_id: String,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    info!("Attempting to delete voucher standard '{}'...", standard_id);
    let app_data_dir = app.path().app_data_dir().map_err(|e| FrontendError::from(e.to_string()))?;
    let standards_dir = app_data_dir.join("voucher_standards");

    let service = state.service.lock().unwrap();
    service
        .delete_voucher_standard(&standard_id, &standards_dir)
        .map_err(FrontendError::from)
}

