use crate::{AppState, models::{FrontendUserProfile, FrontendAddressData}};
use human_money_core::{
    wallet::{AggregatedBalance, VoucherSummary, VoucherDetails, ProofOfDoubleSpendSummary},
};

#[tauri::command]
pub fn get_user_profile(state: tauri::State<AppState>) -> Result<FrontendUserProfile, String> {
    let service = state.service.lock().unwrap();
    let profile = service.get_public_profile()?;
    
    Ok(FrontendUserProfile {
        protocol_version: profile.protocol_version,
        id: profile.id,
        first_name: profile.first_name,
        last_name: profile.last_name,
        organization: profile.organization,
        community: profile.community,
        address: profile.address.map(|a| FrontendAddressData {
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
pub fn get_total_balance_by_currency(state: tauri::State<AppState>) -> Result<Vec<AggregatedBalance>, String> {
    let service = state.service.lock().unwrap();
    service.get_total_balance_by_currency()
}

#[tauri::command]
pub fn get_voucher_summaries(state: tauri::State<AppState>) -> Result<Vec<VoucherSummary>, String> {
    let service = state.service.lock().unwrap();
    // The robust filtering is handled in the frontend; the backend's job is to fetch all summaries.
    service.get_voucher_summaries(None, None)
}

#[tauri::command]
pub fn get_voucher_details(local_id: String, state: tauri::State<AppState>) -> Result<VoucherDetails, String> {
    let service = state.service.lock().unwrap();
    service.get_voucher_details(&local_id)
}

#[tauri::command]
pub fn get_voucher_source_sender(local_id: String, state: tauri::State<AppState>) -> Result<Option<String>, String> {
    let service = state.service.lock().unwrap();
    service.get_voucher_source_sender(&local_id)
}

#[tauri::command]
pub fn get_double_spend_conflicts(state: tauri::State<AppState>) -> Result<Vec<ProofOfDoubleSpendSummary>, String> {
    let service = state.service.lock().unwrap();
    service.list_conflicts()
}

#[tauri::command]
pub fn get_proof_of_double_spend(proof_id: String, state: tauri::State<AppState>) -> Result<crate::models::FullProofDetails, String> {
    let service = state.service.lock().unwrap();
    let summaries = service.list_conflicts()?;
    let summary = summaries.into_iter().find(|s| s.proof_id == proof_id)
        .ok_or_else(|| format!("Conflict proof {} not found", proof_id))?;
    
    let proof = service.get_proof_of_double_spend(&proof_id)?;
    
    Ok(crate::models::FullProofDetails {
        proof,
        local_override: summary.local_override,
        local_note: summary.local_note,
        conflict_role: summary.conflict_role,
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
) -> Result<human_money_core::models::conflict::TrustStatus, String> {
    log::info!("Checking reputation for offender: {}", offender_id);
    let service = state.service.lock().unwrap();
    service.check_reputation(&offender_id)
}
