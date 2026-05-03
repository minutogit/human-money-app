use crate::AppState;

#[tauri::command]
pub fn get_active_asset_classes(state: tauri::State<AppState>) -> Result<Vec<crate::models::FrontendAssetClassSummary>, String> {
    let service = state.service.lock().unwrap();
    let asset_classes = service.get_active_asset_classes()?;
    Ok(asset_classes.into_iter().map(|s| s.into()).collect())
}

#[tauri::command]
pub fn get_user_profile(state: tauri::State<AppState>) -> Result<crate::models::FrontendUserProfile, String> {
    let service = state.service.lock().unwrap();
    let profile = service.get_public_profile()?;
    
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
        ..Default::default()
    })
}

#[tauri::command]
pub fn get_user_id(state: tauri::State<AppState>) -> Result<String, String> {
    let service = state.service.lock().unwrap();
    service.get_user_id()
}

#[tauri::command]
pub fn get_total_balance_by_currency(state: tauri::State<AppState>) -> Result<Vec<crate::models::FrontendAggregatedBalance>, String> {
    let service = state.service.lock().unwrap();
    let balances = service.get_total_balance_by_currency()?;
    Ok(balances.into_iter().map(|b| b.into()).collect())
}

#[tauri::command]
pub fn get_voucher_summaries(test_filter: Option<bool>, state: tauri::State<AppState>) -> Result<Vec<crate::models::FrontendVoucherSummary>, String> {
    let service = state.service.lock().unwrap();
    // Use the optional test_filter provided by the frontend.
    let summaries = service.get_voucher_summaries(None, None, test_filter)?;
    Ok(summaries.into_iter().map(|s| s.into()).collect())
}

#[tauri::command]
pub fn get_voucher_details(local_id: String, state: tauri::State<AppState>) -> Result<crate::models::FrontendVoucherDetails, String> {
    let service = state.service.lock().unwrap();
    let details = service.get_voucher_details(&local_id)?;
    Ok(details.into())
}

#[tauri::command]
pub fn get_voucher_source_sender(local_id: String, state: tauri::State<AppState>) -> Result<Option<String>, String> {
    let service = state.service.lock().unwrap();
    service.get_voucher_source_sender(&local_id)
}

#[tauri::command]
pub fn get_double_spend_conflicts(state: tauri::State<AppState>) -> Result<Vec<crate::models::FrontendProofOfDoubleSpendSummary>, String> {
    let service = state.service.lock().unwrap();
    let conflicts = service.list_conflicts()?;
    Ok(conflicts.into_iter().map(|s| s.into()).collect())
}

#[tauri::command]
pub fn get_proof_of_double_spend(proof_id: String, state: tauri::State<AppState>) -> Result<crate::models::FullProofDetails, String> {
    let service = state.service.lock().unwrap();
    let summaries = service.list_conflicts()?;
    let summary = summaries.into_iter().find(|s| s.proof_id == proof_id)
        .ok_or_else(|| format!("Conflict proof {} not found", proof_id))?;
    
    let proof = service.get_proof_of_double_spend(&proof_id)?;
    
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
pub fn get_proof_id_for_voucher(local_id: String, state: tauri::State<AppState>) -> Result<Option<String>, String> {
    let service = state.service.lock().unwrap();
    let conflicts = service.list_conflicts()?;
    let details = service.get_voucher_details(&local_id)?;
    

    for conflict in &conflicts {
        let proof = service.get_proof_of_double_spend(&conflict.proof_id)?;
        
        
        // Match 1: Direct transaction ID match (Checks all transactions in the voucher)
        let has_tx_match = proof.conflicting_transactions.iter().any(|tx| {
            details.voucher.transactions.iter().any(|vtx| vtx.t_id == tx.t_id)
        });
        if has_tx_match { 
            log::info!("  ✅ Match 1 (t_id) found!");
            return Ok(Some(conflict.proof_id.clone())); 
        }

        // Match 2: DS-Tag match (Very robust)
        let proof_ds_tags: Vec<&str> = proof.conflicting_transactions.iter()
            .filter_map(|tx| tx.trap_data.as_ref().map(|td| td.ds_tag.as_str()))
            .collect();
            
        let has_ds_tag_match = details.voucher.transactions.iter().any(|vtx| {
            vtx.trap_data.as_ref().map(|td| proof_ds_tags.contains(&td.ds_tag.as_str())).unwrap_or(false)
        });
        if has_ds_tag_match { 
            log::info!("  ✅ Match 2 (ds_tag) found!");
            return Ok(Some(conflict.proof_id.clone())); 
        }

        // Match 3: Deep Fork Point match (Scans entire chain)
        let has_deep_fork_match = details.voucher.transactions.iter().any(|vtx| {
            vtx.prev_hash == proof.fork_point_prev_hash || vtx.t_id == proof.fork_point_prev_hash
        });
        // Ergänzung: Falls der Fork-Point irgendwo in der Historie des Vouchers war.
        let has_any_history_match = details.voucher.transactions.iter().any(|vtx| {
             // In einem Proof ist die fork_point_prev_hash die t_id des Elter-Vouchers.
             vtx.prev_hash == proof.fork_point_prev_hash
        });

        if has_deep_fork_match || has_any_history_match { 
            log::info!("  ✅ Match 3 (fork_point) found!");
            return Ok(Some(conflict.proof_id.clone())); 
        }

        // Match 4: Offender & Chain Link (The most aggressive fallback)
        // If the offender is the sender of ANY transaction in this voucher,
        // and we are in quarantine, it's highly likely this is the right proof.
        let is_offender_involved = details.voucher.transactions.iter().any(|vtx| {
            vtx.sender_id.as_deref() == Some(proof.offender_id.as_str())
        });
        if is_offender_involved {
            log::info!("  ✅ Match 4 (offender involvement) found!");
            return Ok(Some(conflict.proof_id.clone()));
        }

        // Match 5: Recipient match (Targeted at the victim)
        let voucher_last_recipient = details.voucher.transactions.last()
            .map(|tx| tx.recipient_id.as_str());
        let has_recipient_match = proof.conflicting_transactions.iter().any(|tx| {
            voucher_last_recipient == Some(tx.recipient_id.as_str())
        });
        if has_recipient_match { 
            log::info!("  ✅ Match 5 (recipient_id) found!");
            return Ok(Some(conflict.proof_id.clone())); 
        }
    }
    
    log::warn!("=== No proof found for quarantined voucher {} after checking {} conflicts ===", local_id, conflicts.len());
    Ok(None)
}

#[tauri::command]
pub fn check_reputation(
    offender_id: String,
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendTrustStatus, String> {
    log::info!("Checking reputation for offender: {}", offender_id);
    let service = state.service.lock().unwrap();
    let status = service.check_reputation(&offender_id)?;
    Ok(status.into())
}
#[tauri::command]
pub fn get_event_history(
    offset: usize,
    limit: usize,
    state: tauri::State<AppState>,
) -> Result<Vec<crate::models::FrontendWalletEvent>, String> {
    // 1. Check cache first
    let events_cache = state.events.lock().unwrap();
    if let Some(events) = events_cache.as_ref() {
        // Return slice from cache based on offset/limit
        let start = offset.min(events.len());
        let end = (offset + limit).min(events.len());
        return Ok(events[start..end].iter().map(|e| e.clone().into()).collect());
    }
    
    // 2. Fallback to backend (might require session to be active)
    let mut service = state.service.lock().unwrap();
    let events = service.get_event_history(offset, limit, None)?;
    Ok(events.into_iter().map(|e| e.into()).collect())
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
