use crate::{models::FrontendNewVoucherData, AppState};
use log::info;
use voucher_lib::{
    models::voucher::Voucher, services::voucher_manager::NewVoucherData,
};

#[tauri::command]
pub fn create_new_voucher(
    standard_toml_content: String,
    data: FrontendNewVoucherData,
    password: String,
    state: tauri::State<AppState>,
) -> Result<Voucher, String> {
    info!("Attempting to create a new voucher...");
    let mut service = state.0.lock().unwrap();
    // For the prototype, language preference is hardcoded.
    let lang_preference = "en-US";

    // Get the current user's ID to use as the creator ID. This fixes the "Invalid creator ID" error.
    let user_id = service.get_user_id()?;

    // Convert the frontend data struct to the library's data struct
    let voucher_data = NewVoucherData {
        nominal_value: voucher_lib::models::voucher::NominalValue {
            amount: data.nominal_value.amount,
            unit: data.nominal_value.unit,
            ..Default::default()
        },
        collateral: voucher_lib::models::voucher::Collateral {
            amount: data.collateral.amount,
            unit: data.collateral.unit,
            abbreviation: data.collateral.abbreviation,
            ..Default::default()
        },
        creator: voucher_lib::models::voucher::Creator {
            id: user_id,
            first_name: data.creator.first_name,
            last_name: data.creator.last_name,
            address: voucher_lib::models::voucher::Address {
                street: data.creator.address.street,
                house_number: data.creator.address.house_number,
                zip_code: data.creator.address.zip_code,
                city: data.creator.address.city,
                country: data.creator.address.country,
                full_address: data.creator.address.full_address,
            },
            organization: data.creator.organization,
            community: data.creator.community,
            phone: data.creator.phone,
            email: data.creator.email,
            url: data.creator.url,
            gender: data.creator.gender,
            service_offer: data.creator.service_offer,
            needs: data.creator.needs,
            coordinates: data.creator.coordinates,
            ..Default::default()
        },
        validity_duration: data.validity_duration,
        non_redeemable_test_voucher: data.non_redeemable_test_voucher,
    };

    service.create_new_voucher(&standard_toml_content, lang_preference, voucher_data, &password)
}
