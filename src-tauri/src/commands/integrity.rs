use crate::models::IntegrityReport;
use crate::AppState;
use tauri::State;
use log::{info, error};

#[tauri::command]
pub async fn check_wallet_integrity(
    state: State<'_, AppState>,
) -> Result<IntegrityReport, String> {
    info!("Checking wallet integrity...");
    let mut service = state.service.lock().unwrap();
    let report = service.check_integrity(None)
        .map_err(|e| {
            error!("Integrity check failed: {}", e);
            e
        })?;
    
    info!("Integrity report: {:?}", report);
    Ok(report.into())
}

#[tauri::command]
pub async fn repair_wallet_integrity(
    state: State<'_, AppState>,
    password: Option<String>,
) -> Result<(), String> {
    info!("Repairing wallet integrity...");
    let mut service = state.service.lock().unwrap();
    service.repair_integrity(password.as_deref())
        .map_err(|e| {
            error!("Integrity repair failed: {}", e);
            e
        })?;
    
    info!("Integrity repair successful.");
    Ok(())
}
