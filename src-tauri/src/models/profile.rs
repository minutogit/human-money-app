use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAddressData {
    #[serde(default)]
    pub street: String,
    #[serde(default)]
    pub house_number: String,
    #[serde(default)]
    pub zip_code: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub full_address: String,
}

impl From<human_money_core::models::voucher::Address> for FrontendAddressData {
    fn from(a: human_money_core::models::voucher::Address) -> Self {
        Self {
            street: a.street,
            house_number: a.house_number,
            zip_code: a.zip_code,
            city: a.city,
            country: a.country,
            full_address: a.full_address,
        }
    }
}

impl From<FrontendAddressData> for human_money_core::models::voucher::Address {
    fn from(a: FrontendAddressData) -> Self {
        Self {
            street: a.street,
            house_number: a.house_number,
            zip_code: a.zip_code,
            city: a.city,
            country: a.country,
            full_address: a.full_address,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrontendUserProfile {
    pub protocol_version: Option<String>,
    pub id: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub organization: Option<String>,
    pub community: Option<String>,
    pub address: Option<FrontendAddressData>,
    pub gender: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub coordinates: Option<String>,
    pub url: Option<String>,
    pub service_offer: Option<String>,
    pub needs: Option<String>,
    pub picture_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendContact {
    pub did: String,
    pub profile: FrontendUserProfile,
    pub tags: Vec<String>,
    pub added_at: String,
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAddressBook {
    pub contacts: std::collections::HashMap<String, FrontendContact>,
}
