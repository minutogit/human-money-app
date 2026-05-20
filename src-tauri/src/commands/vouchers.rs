// src-tauri/src/commands/vouchers.rs
use crate::{models::{FrontendNewVoucherData}, AppState};
use log::{info, error};
use human_money_core::{
    models::voucher::Voucher,
    models::secure_container::ContainerConfig,
    services::voucher_manager::NewVoucherData,
};

#[tauri::command]
pub fn create_new_voucher(
    standard_toml_content: String,
    data: FrontendNewVoucherData,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendVoucher, String> {
    info!("Attempting to create a new voucher...");
    let mut service = state.service.lock().unwrap();
    let lang_preference = "en-US";
    let user_id = service.get_user_id()?;

    let voucher_data = NewVoucherData {
        nominal_value: human_money_core::models::voucher::ValueDefinition {
            amount: data.nominal_value.amount,
            unit: data.nominal_value.unit,
            ..Default::default()
        },
        collateral: Some(human_money_core::models::voucher::Collateral {
            value: human_money_core::models::voucher::ValueDefinition {
                amount: data.collateral.amount,
                unit: data.collateral.unit,
                ..Default::default()
            },
            ..Default::default()
        }),
        creator_profile: human_money_core::models::profile::PublicProfile {
            protocol_version: data.creator.protocol_version,
            id: Some(user_id),
            first_name: Some(data.creator.first_name),
            last_name: Some(data.creator.last_name),
            address: Some(human_money_core::models::voucher::Address {
                street: data.creator.address.street,
                house_number: data.creator.address.house_number,
                zip_code: data.creator.address.zip_code,
                city: data.creator.address.city,
                country: data.creator.address.country,
                full_address: data.creator.address.full_address,
            }),
            organization: data.creator.organization,
            community: data.creator.community,
            phone: data.creator.phone,
            email: data.creator.email,
            url: data.creator.url,
            gender: Some(data.creator.gender),
            coordinates: Some(data.creator.coordinates),
            service_offer: data.creator.service_offer,
            needs: data.creator.needs,
            ..Default::default()
        },
        validity_duration: data.validity_duration,
        non_redeemable_test_voucher: data.non_redeemable_test_voucher,
    };

    let voucher = service.create_new_voucher(&standard_toml_content, lang_preference, voucher_data, password.as_deref())?;
    
    if let Err(e) = state.refresh_events_cache(&mut service, password.as_deref()) {
        error!("Failed to refresh events cache after create_new_voucher: {}", e);
    }

    Ok(voucher.into())
}

#[tauri::command]
pub fn create_signing_request_bundle(
    local_instance_id: String,
    config: ContainerConfig,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, String> {
    info!(
        "Creating signing request bundle for voucher {} with config {:?}",
        local_instance_id, config
    );
    let service = state.service.lock().unwrap();
    service.create_signing_request_bundle(&local_instance_id, config)
}

#[tauri::command]
pub fn open_voucher_signing_request(
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendVoucherDetails, String> {
    info!("Opening voucher signing request bundle...");
    let service = state.service.lock().unwrap();
    match service.open_voucher_signing_request(&container_bytes, password.as_deref()) {
        Ok(v) => {
            info!("Signature request opened successfully. Voucher ID: {}, Creator: {}", v.voucher_id, v.creator_profile.id.as_deref().unwrap_or("N/A"));
            
            let display_currency = v.nominal_value.unit.clone();
            let display_standard_name = v.voucher_standard.name.clone();
            let is_test_voucher = v.non_redeemable_test_voucher;
            
            Ok(crate::models::FrontendVoucherDetails {
                local_instance_id: format!("external-{}", v.voucher_id.chars().take(8).collect::<String>()),
                status: "Active".to_string(),
                voucher: v.into(),
                display_currency,
                display_standard_name,
                is_test_voucher,
            })
        }
        Err(e) => Err(e)
    }
}

#[tauri::command]
pub fn create_detached_signature_response_bundle(
    voucher: Voucher,
    role: String,
    include_details: bool,
    config: ContainerConfig,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, String> {
    info!(
        "Creating detached signature response for role {} with config {:?}",
        role, config
    );
    let mut service = state.service.lock().unwrap();
    
    if !service.is_wallet_unlocked() {
        error!("SIGNING ERROR: AppService is in LOCKED state. No profile is currently loaded in the backend.");
        return Err("Wallet is completely locked. Please log out and log in again to refresh your session.".to_string());
    }

    if let Some(ref pwd) = password {
        if let Err(e) = service.unlock_session(pwd, 300) {
            error!("Failed to unlock session during signature creation: {}", e);
        }
    }

    service.create_detached_signature_response_bundle(&voucher, &role, include_details, config, password.as_deref())
}

#[tauri::command]
pub fn process_and_attach_signature(
    container_bytes: Vec<u8>,
    standard_toml_content: String,
    container_password: Option<String>,
    wallet_password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    info!("Processing and attaching signature from bundle...");
    let mut service = state.service.lock().unwrap();
    service.process_and_attach_signature(
        &container_bytes,
        &standard_toml_content,
        container_password.as_deref(),
        wallet_password.as_deref(),
    )
}

#[tauri::command]
pub fn remove_voucher_signature(
    local_instance_id: String,
    signature_id: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!(
        "Removing signature {} from voucher {}...",
        signature_id, local_instance_id
    );
    let mut service = state.service.lock().unwrap();
    service.remove_voucher_signature(
        &local_instance_id,
        &signature_id,
        password.as_deref(),
    )
}
