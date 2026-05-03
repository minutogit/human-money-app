use crate::AppState;
use crate::models::{VoucherStandardDefinitionDto, FrontendSignatureImpact};
use human_money_core::models::voucher::Voucher;
use log::info;

#[tauri::command]
pub fn parse_standard_toml(
    toml_content: String,
    state: tauri::State<AppState>,
) -> Result<VoucherStandardDefinitionDto, String> {
    let service = state.service.lock().unwrap();
    let core_standard = service.parse_voucher_standard(&toml_content)?;
    Ok(core_standard.into())
}

#[tauri::command]
pub fn get_allowed_signature_roles_from_standard(
    toml_content: String,
    state: tauri::State<AppState>,
) -> Result<Vec<String>, String> {
    info!("Getting allowed signature roles from standard...");
    let service = state.service.lock().unwrap();
    service.get_allowed_signature_roles_from_standard(&toml_content)
}

#[tauri::command]
pub fn evaluate_signature_suitability(
    voucher: Voucher,
    role: String,
    standard_toml_content: String,
    state: tauri::State<AppState>,
) -> Result<FrontendSignatureImpact, String> {
    info!("Evaluating signature suitability for role: {}", role);
    let service = state.service.lock().unwrap();
    let impact = service.evaluate_signature_suitability(&voucher, &role, &standard_toml_content)?;
    Ok(impact.into())
}
