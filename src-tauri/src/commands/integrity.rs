use crate::models::{IntegrityReport, FrontendError};
use crate::AppState;
use tauri::State;
use log::{info, error};

#[tauri::command]
pub async fn check_wallet_integrity(
    state: State<'_, AppState>,
) -> Result<IntegrityReport, FrontendError> {
    info!("Checking wallet integrity...");
    let mut service = state.service.lock().unwrap();
    let report = service.check_integrity(None)
        .map_err(|e| {
            error!("Integrity check failed: {}", e);
            e
        }).map_err(FrontendError::from)?;
    
    info!("Integrity report: {:?}", report);
    Ok(report.into())
}

#[tauri::command]
pub async fn repair_wallet_integrity(
    state: State<'_, AppState>,
    password: Option<String>,
) -> Result<(), FrontendError> {
    info!("Repairing wallet integrity...");
    let mut service = state.service.lock().unwrap();
    service.repair_integrity(password.as_deref())
        .map_err(|e| {
            error!("Integrity repair failed: {}", e);
            e
        }).map_err(FrontendError::from)?;
    
    info!("Integrity repair successful.");
    Ok(())
}

#[tauri::command]
pub fn export_double_spend_proof(
    proof_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, FrontendError> {
    info!("Exporting double spend proof: {}", proof_id);
    let service = state.service.lock().unwrap();
    let proof = service.get_proof_of_double_spend(&proof_id)
        .map_err(FrontendError::from)?;
    let bytes = serde_json::to_vec_pretty(&proof)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    Ok(bytes)
}

#[tauri::command]
pub fn import_double_spend_proof(
    proof_bytes: Vec<u8>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, FrontendError> {
    info!("Importing double spend proof ({} bytes)...", proof_bytes.len());
    let proof: human_money_core::models::conflict::ProofOfDoubleSpend =
        serde_json::from_slice(&proof_bytes).map_err(|e| FrontendError::from(e.to_string()))?;
    let proof_id = proof.proof_id.clone();
    let mut service = state.service.lock().unwrap();
    service.import_proof(proof, password.as_deref())
        .map_err(FrontendError::from)?;
    info!("Double spend proof imported successfully: {}", proof_id);
    Ok(proof_id)
}

