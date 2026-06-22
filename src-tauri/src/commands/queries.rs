use crate::AppState;
use crate::models::FrontendError;

#[tauri::command]
pub fn get_active_asset_classes(state: tauri::State<AppState>) -> Result<Vec<crate::models::FrontendAssetClassSummary>, FrontendError> {
    let service = state.service.lock().unwrap();
    let asset_classes = service.get_active_asset_classes().map_err(FrontendError::from)?;
    Ok(asset_classes.into_iter().map(|s| s.into()).collect())
}

#[tauri::command]
pub fn get_user_profile(state: tauri::State<AppState>) -> Result<crate::models::FrontendUserProfile, FrontendError> {
    let service = state.service.lock().unwrap();
    let profile = service.get_public_profile().map_err(FrontendError::from)?;
    
    Ok(crate::models::FrontendUserProfile {
        protocol_version: profile.protocol_version,
        id: profile.id,
        first_name: profile.first_name,
        last_name: profile.last_name,
        organization: profile.organization,
        community: profile.community,
        address: profile.address.map(|a| crate::models::FrontendAddressData {
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
    })
}

#[tauri::command]
pub fn get_user_id(state: tauri::State<AppState>) -> Result<String, FrontendError> {
    let service = state.service.lock().unwrap();
    service.get_user_id().map_err(FrontendError::from)
}

#[tauri::command]
pub fn get_total_balance_by_currency(state: tauri::State<AppState>) -> Result<Vec<crate::models::FrontendAggregatedBalance>, FrontendError> {
    let service = state.service.lock().unwrap();
    let balances = service.get_total_balance_by_currency().map_err(FrontendError::from)?;
    Ok(balances.into_iter().map(|b| b.into()).collect())
}

#[tauri::command]
pub fn get_voucher_summaries(test_filter: Option<bool>, state: tauri::State<AppState>) -> Result<Vec<crate::models::FrontendVoucherSummary>, FrontendError> {
    let service = state.service.lock().unwrap();
    // Use the optional test_filter provided by the frontend.
    let summaries = service.get_voucher_summaries(None, None, test_filter).map_err(FrontendError::from)?;
    Ok(summaries.into_iter().map(|s| s.into()).collect())
}

#[tauri::command]
pub fn get_voucher_details(local_id: String, state: tauri::State<AppState>) -> Result<crate::models::FrontendVoucherDetails, FrontendError> {
    let service = state.service.lock().unwrap();
    let details = service.get_voucher_details(&local_id).map_err(FrontendError::from)?;
    Ok(details.into())
}

#[tauri::command]
pub fn get_voucher_source_sender(local_id: String, state: tauri::State<AppState>) -> Result<Option<String>, FrontendError> {
    let service = state.service.lock().unwrap();
    service.get_voucher_source_sender(&local_id).map_err(FrontendError::from)
}

#[tauri::command]
pub fn get_double_spend_conflicts(state: tauri::State<AppState>) -> Result<Vec<crate::models::FrontendProofOfDoubleSpendSummary>, FrontendError> {
    let service = state.service.lock().unwrap();
    let conflicts = service.list_conflicts().map_err(FrontendError::from)?;
    Ok(conflicts.into_iter().map(|s| s.into()).collect())
}

#[tauri::command]
pub fn get_proof_of_double_spend(proof_id: String, state: tauri::State<AppState>) -> Result<crate::models::FullProofDetails, FrontendError> {
    let service = state.service.lock().unwrap();
    let summaries = service.list_conflicts().map_err(FrontendError::from)?;
    let summary = summaries.into_iter().find(|s| s.proof_id == proof_id)
        .ok_or_else(|| FrontendError::from(format!("Conflict proof {} not found", proof_id)))?;
    
    let proof = service.get_proof_of_double_spend(&proof_id).map_err(FrontendError::from)?;
    
    let conflict_role = match summary.conflict_role {
        human_money_core::models::conflict::ConflictRole::Victim => "Victim",
        human_money_core::models::conflict::ConflictRole::Witness => "Witness",
    };
    
    Ok(crate::models::FullProofDetails {
        proof: proof.into(),
        local_override: summary.local_override,
        local_note: summary.local_note,
        conflict_role: conflict_role.to_string(),
    })
}

#[tauri::command]
pub fn get_proof_id_for_voucher(local_id: String, state: tauri::State<AppState>) -> Result<Option<String>, FrontendError> {
    let service = state.service.lock().unwrap();
    service.get_proof_id_for_voucher(&local_id).map_err(FrontendError::from)
}

#[tauri::command]
pub fn check_reputation(
    offender_id: String,
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendTrustStatus, FrontendError> {
    log::info!("Checking reputation for offender: {}", offender_id);
    let service = state.service.lock().unwrap();
    let status = service.check_reputation(&offender_id).map_err(FrontendError::from)?;
    Ok(status.into())
}
#[tauri::command]
pub fn get_event_history(
    offset: usize,
    limit: usize,
    state: tauri::State<AppState>,
) -> Result<Vec<crate::models::FrontendWalletEvent>, FrontendError> {
    let events_cache = state.events.lock().unwrap();
    if let Some(events) = events_cache.as_ref() {
        let start = offset.min(events.len());
        let end = (offset + limit).min(events.len());
        Ok(events[start..end].iter().map(|e| e.clone().into()).collect())
    } else {
        Err(FrontendError::from("Events cache not initialized".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use human_money_core::app_service::AppService;
    use human_money_core::models::wallet_event::{WalletEvent, WalletEventType};
    use chrono::Utc;

    #[test]
    fn test_get_event_history_uses_cache() {
        use std::path::PathBuf;
        // Mock AppState
        let service = AppService::new(&PathBuf::from("/tmp")).unwrap();
        let state_raw = AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(Some(vec![
                WalletEvent {
                    event_id: "test_event_1".to_string(),
                    event_type: WalletEventType::VoucherActivated,
                    timestamp: Utc::now(),
                    local_instance_id: "voucher_1".to_string(),
                    voucher_id: "vid_1".to_string(),
                    bff_data: Default::default(),
                }
            ])),
            contacts: Mutex::new(None),
            settings: Mutex::new(None),
        };
        
        let events = state_raw.events.lock().unwrap();
        assert!(events.is_some());
        assert_eq!(events.as_ref().unwrap().len(), 1);
        assert_eq!(events.as_ref().unwrap()[0].event_id, "test_event_1");
    }
}
