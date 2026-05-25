use crate::AppState;
use crate::models::{VoucherStandardDefinitionDto, FrontendSignatureImpact, FrontendError};
use log::info;

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
