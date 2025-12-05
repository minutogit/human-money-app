use serde::{Deserialize, Serialize};

// A local struct that mirrors `human_money_core::...::NewVoucherData` but derives `Deserialize`.
// This is necessary because Tauri needs to deserialize the JSON payload from the frontend,
// and we cannot add `#[derive(Deserialize)]` to the original struct in the upstream library.
#[derive(Deserialize, Debug)]
pub struct NominalValueData {
    pub amount: String,
    pub unit: String,
}

#[derive(Deserialize, Debug)]
pub struct FrontendAddressData {
    pub street: String,
    pub house_number: String,
    pub zip_code: String,
    pub city: String,
    pub country: String,
    pub full_address: String,
}

#[derive(Deserialize, Debug)]
pub struct FrontendCollateralData {
    pub amount: String,
    pub unit: String,
    pub abbreviation: String,
}

#[derive(Deserialize, Debug)]
pub struct FrontendCreatorData {
    pub first_name: String,
    pub last_name: String,
    pub address: FrontendAddressData,
    pub organization: Option<String>,
    pub community: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub url: Option<String>,
    pub gender: String,
    pub service_offer: Option<String>,
    pub needs: Option<String>,
    pub coordinates: String,
}

#[derive(Deserialize, Debug)]
pub struct FrontendNewVoucherData {
    pub nominal_value: NominalValueData,
    pub creator: FrontendCreatorData,
    pub validity_duration: Option<String>,
    pub non_redeemable_test_voucher: bool,
    pub collateral: FrontendCollateralData,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub profile_name: String,
    pub folder_name: String,
}

#[derive(Serialize, Clone)]
pub struct VoucherStandardInfo {
    pub id: String,
    pub content: String,
}
