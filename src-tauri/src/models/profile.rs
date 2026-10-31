use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
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

impl From<human_money_core::models::profile::PublicProfile> for FrontendUserProfile {
    fn from(p: human_money_core::models::profile::PublicProfile) -> Self {
        Self {
            protocol_version: p.protocol_version,
            id: p.id,
            first_name: p.first_name,
            last_name: p.last_name,
            organization: p.organization,
            community: p.community,
            address: p.address.map(|a| a.into()),
            gender: p.gender,
            email: p.email,
            phone: p.phone,
            coordinates: p.coordinates,
            url: p.url,
            service_offer: p.service_offer,
            needs: p.needs,
            picture_url: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendContact {
    pub did: String,
    pub profile: FrontendUserProfile,
    pub tags: Vec<String>,
    pub added_at: String,
    pub notes: Option<String>,
    #[serde(default)]
    pub trust_level: Option<u8>,
    #[serde(default)]
    pub desired_hops: Option<u8>,
    #[serde(default)]
    pub has_peer_list: bool,
    #[serde(default)]
    pub peer_list_updated_at: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAddressBook {
    pub contacts: std::collections::HashMap<String, FrontendContact>,
}
