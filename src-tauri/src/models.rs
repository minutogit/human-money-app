use serde::{Deserialize, Serialize};

// Enum for mnemonic language selection (BIP-39)
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MnemonicLanguage {
    English,
    German,
    Spanish,
    French,
    Italian,
    Japanese,
    Korean,
    Portuguese,
    Czech,
    ChineseSimplified,
    ChineseTraditional,
}

impl From<MnemonicLanguage> for human_money_core::MnemonicLanguage {
    fn from(lang: MnemonicLanguage) -> Self {
        match lang {
            MnemonicLanguage::English => human_money_core::MnemonicLanguage::English,
            MnemonicLanguage::German => human_money_core::MnemonicLanguage::German,
            MnemonicLanguage::Spanish => human_money_core::MnemonicLanguage::Spanish,
            MnemonicLanguage::French => human_money_core::MnemonicLanguage::French,
            MnemonicLanguage::Italian => human_money_core::MnemonicLanguage::Italian,
            MnemonicLanguage::Japanese => human_money_core::MnemonicLanguage::Japanese,
            MnemonicLanguage::Korean => human_money_core::MnemonicLanguage::Korean,
            MnemonicLanguage::Portuguese => human_money_core::MnemonicLanguage::Portuguese,
            MnemonicLanguage::Czech => human_money_core::MnemonicLanguage::Czech,
            MnemonicLanguage::ChineseSimplified => human_money_core::MnemonicLanguage::ChineseSimplified,
            MnemonicLanguage::ChineseTraditional => human_money_core::MnemonicLanguage::ChineseTraditional,
        }
    }
}

// A local struct that mirrors `human_money_core::...::NewVoucherData` but derives `Deserialize`.
// This is necessary because Tauri needs to deserialize the JSON payload from the frontend,
// and we cannot add `#[derive(Deserialize)]` to the original struct in the upstream library.
#[derive(Deserialize, Debug)]
pub struct NominalValueData {
    pub amount: String,
    pub unit: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrontendAddressData {
    pub street: String,
    pub house_number: String,
    pub zip_code: String,
    pub city: String,
    pub country: String,
    pub full_address: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrontendCollateralData {
    pub amount: String,
    pub unit: String,
    pub abbreviation: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrontendCreatorData {
    pub protocol_version: Option<String>,
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

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
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
pub struct FrontendContact {
    pub did: String,
    pub profile: FrontendUserProfile,
    pub tags: Vec<String>,
    pub added_at: String,
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct FrontendAddressBook {
    pub contacts: std::collections::HashMap<String, FrontendContact>,
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
pub struct ProfileInfo {
    pub profile_name: String,
    pub folder_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct VoucherStandardInfo {
    pub id: String,
    pub content: String,
}
#[derive(Serialize, Clone)]
pub struct FullProofDetails {
    pub proof: human_money_core::models::conflict::ProofOfDoubleSpend,
    pub local_override: bool,
    pub local_note: Option<String>,
    pub conflict_role: human_money_core::models::conflict::ConflictRole,
}
