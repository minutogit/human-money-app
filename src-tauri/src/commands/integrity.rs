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
