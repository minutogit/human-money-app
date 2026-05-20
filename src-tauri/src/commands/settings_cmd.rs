use crate::{models::{FrontendUserProfile, SealUploadData}, AppState, settings::AppSettings};
use log::{info, error};

#[tauri::command]
pub fn get_app_settings(state: tauri::State<AppState>) -> Result<AppSettings, String> {
    info!("Getting app settings from cache...");
    state.get_cached_settings()
}

#[tauri::command]
pub fn save_app_settings(
    settings: AppSettings,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Saving app settings to disk...");
    let mut service = state.service.lock().unwrap();
    state.save_settings(&mut service, settings, password.as_deref())
}

#[tauri::command]
pub fn update_user_profile(
    profile: FrontendUserProfile,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Updating user profile...");
    let mut service = state.service.lock().unwrap();
    
    let core_profile = human_money_core::models::profile::PublicProfile {
        protocol_version: profile.protocol_version,
        id: profile.id,
        first_name: profile.first_name,
        last_name: profile.last_name,
        organization: profile.organization,
        community: profile.community,
        address: profile.address.map(|a| human_money_core::models::voucher::Address {
            street: a.street,
            house_number: a.house_number,
            zip_code: a.zip_code,
            city: a.city,
            country: a.country,
            full_address: a.full_address,
        }),
        gender: profile.gender,
        email: profile.email,
        phone: profile.phone,
        coordinates: profile.coordinates,
        url: profile.url,
        service_offer: profile.service_offer,
        needs: profile.needs,
        picture_url: profile.picture_url,
    };
    
    service.update_public_profile(core_profile, password.as_deref())
}

#[tauri::command]
pub fn set_conflict_local_override(
    proof_id: String,
    value: bool,
    note: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Setting local override for conflict {} to {} with note: {:?}", proof_id, value, note);
    let mut service = state.service.lock().unwrap();
    
    if let Some(ref pwd) = password {
        if let Err(e) = service.unlock_session(pwd, 300) {
            error!("Failed to unlock session for local override: {}", e);
        }
    }
    
    service.set_conflict_local_override(&proof_id, value, note, password.as_deref())
}

// --- WalletSeal / Sync Commands ---

#[tauri::command]
pub fn get_seal_sync_status(state: tauri::State<AppState>) -> Result<String, String> {
    let service = state.service.lock().unwrap();
    let status = service.get_seal_sync_status()?;
    let status_str = match status {
        human_money_core::models::seal::SyncStatus::PendingUpload => "PendingUpload",
        human_money_core::models::seal::SyncStatus::Synced => "Synced",
    };
    Ok(status_str.to_string())
}

#[tauri::command]
pub fn get_seal_for_upload(state: tauri::State<AppState>) -> Result<Option<SealUploadData>, String> {
    let service = state.service.lock().unwrap();
    
    let data = service.get_seal_for_upload()?;
    if let Some(bytes) = data {
        let seal: human_money_core::models::seal::WalletSeal = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Failed to parse seal for hashing: {}", e))?;
        
        let hash = human_money_core::services::seal_manager::SealManager::compute_seal_hash(&seal)
            .map_err(|e| e.to_string())?;
            
        Ok(Some(SealUploadData {
            seal_bytes: bytes,
            seal_hash: hash,
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn acknowledge_seal_sync(
    uploaded_seal_hash: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let mut service = state.service.lock().unwrap();
    service.acknowledge_seal_sync(&uploaded_seal_hash, password.as_deref()).map_err(|e| e.to_string())
}
