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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub profile_name: String,
    pub folder_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAggregatedBalance {
    pub standard_name: String,
    pub standard_uuid: String,
    pub unit: String,
    pub total_amount: String,
    pub display_currency: String,
    pub display_standard_name: String,
    pub is_test_voucher: bool,
}

impl From<human_money_core::wallet::types::AggregatedBalance> for FrontendAggregatedBalance {
    fn from(b: human_money_core::wallet::types::AggregatedBalance) -> Self {
        Self {
            standard_name: b.standard_name,
            standard_uuid: b.standard_uuid,
            unit: b.unit,
            total_amount: b.total_amount,
            display_currency: b.display_currency,
            display_standard_name: b.display_standard_name,
            is_test_voucher: b.is_test_voucher,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTransferSummary {
    pub summable_amounts: std::collections::HashMap<String, String>,
    pub countable_items: std::collections::HashMap<String, u32>,
}

impl From<human_money_core::wallet::types::TransferSummary> for FrontendTransferSummary {
    fn from(s: human_money_core::wallet::types::TransferSummary) -> Self {
        Self {
            summable_amounts: s.summable_amounts,
            countable_items: s.countable_items,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendInvolvedVoucherInfo {
    pub local_instance_id: String,
    pub voucher_id: String,
    pub standard_name: String,
    pub unit: String,
    pub amount: String,
    pub allow_partial_transfers: bool,
    pub is_test_voucher: bool,
    pub display_currency: String,
    pub display_standard_name: String,
}

impl From<human_money_core::wallet::types::InvolvedVoucherInfo> for FrontendInvolvedVoucherInfo {
    fn from(i: human_money_core::wallet::types::InvolvedVoucherInfo) -> Self {
        Self {
            local_instance_id: i.local_instance_id,
            voucher_id: i.voucher_id,
            standard_name: i.standard_name,
            unit: i.unit,
            amount: i.amount,
            allow_partial_transfers: i.allow_partial_transfers,
            is_test_voucher: i.is_test_voucher,
            display_currency: i.display_currency,
            display_standard_name: i.display_standard_name,
        }
    }
}

impl From<FrontendInvolvedVoucherInfo> for human_money_core::wallet::types::InvolvedVoucherInfo {
    fn from(i: FrontendInvolvedVoucherInfo) -> Self {
        Self {
            local_instance_id: i.local_instance_id,
            voucher_id: i.voucher_id,
            standard_name: i.standard_name,
            unit: i.unit,
            amount: i.amount,
            allow_partial_transfers: i.allow_partial_transfers,
            is_test_voucher: i.is_test_voucher,
            display_currency: i.display_currency,
            display_standard_name: i.display_standard_name,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "type", content = "items", rename_all = "camelCase")]
pub enum IntegrityReport {
    Valid,
    MissingItems(Vec<String>),
    ManipulatedItems(Vec<String>),
    UnknownItems(Vec<String>),
    IntegrityOutdated,
    InvalidSignature,
    MissingIntegrityRecord,
}

impl From<human_money_core::models::storage_integrity::IntegrityReport> for IntegrityReport {
    fn from(report: human_money_core::models::storage_integrity::IntegrityReport) -> Self {
        match report {
            human_money_core::models::storage_integrity::IntegrityReport::Valid => IntegrityReport::Valid,
            human_money_core::models::storage_integrity::IntegrityReport::MissingItems(items) => IntegrityReport::MissingItems(items),
            human_money_core::models::storage_integrity::IntegrityReport::ManipulatedItems(items) => IntegrityReport::ManipulatedItems(items),
            human_money_core::models::storage_integrity::IntegrityReport::UnknownItems(items) => IntegrityReport::UnknownItems(items),
            human_money_core::models::storage_integrity::IntegrityReport::IntegrityOutdated => IntegrityReport::IntegrityOutdated,
            human_money_core::models::storage_integrity::IntegrityReport::InvalidSignature => IntegrityReport::InvalidSignature,
            human_money_core::models::storage_integrity::IntegrityReport::MissingIntegrityRecord => IntegrityReport::MissingIntegrityRecord,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTransactionRecord {
    pub id: String,
    pub direction: String, // "sent" or "received"
    pub recipient_id: String,
    pub sender_id: String,
    pub timestamp: String, // ISO 8601
    pub summable_amounts: std::collections::HashMap<String, String>,
    pub countable_items: std::collections::HashMap<String, u32>,
    pub involved_vouchers: Vec<String>, // local_instance_ids
    pub involved_sources_details: Option<Vec<FrontendInvolvedVoucherInfo>>,
    pub bundle_data: Vec<u8>,
    pub bundle_id: String,
    pub notes: Option<String>,
    pub sender_profile_name: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransactionRecord {
    pub id: String,
    pub direction: String, // "sent" or "received"
    pub recipient_id: String,
    pub sender_id: String,
    pub timestamp: String, // ISO 8601

    #[serde(alias = "total_amount_by_unit", alias = "summable_amounts", alias = "summableAmounts", default)]
    pub summable_amounts: std::collections::HashMap<String, String>,

    #[serde(alias = "countable_items", alias = "countableItems", default)]
    pub countable_items: std::collections::HashMap<String, u32>,
    
    pub involved_vouchers: Vec<String>, // local_instance_ids
    pub involved_sources_details: Option<Vec<human_money_core::wallet::types::InvolvedVoucherInfo>>,

    pub bundle_data: Vec<u8>,
    pub bundle_id: String,
    pub notes: Option<String>,
    pub sender_profile_name: Option<String>,
}

impl From<TransactionRecord> for FrontendTransactionRecord {
    fn from(r: TransactionRecord) -> Self {
        (&r).into()
    }
}

impl From<&TransactionRecord> for FrontendTransactionRecord {
    fn from(r: &TransactionRecord) -> Self {
        Self {
            id: r.id.clone(),
            direction: r.direction.clone(),
            recipient_id: r.recipient_id.clone(),
            sender_id: r.sender_id.clone(),
            timestamp: r.timestamp.clone(),
            summable_amounts: r.summable_amounts.clone(),
            countable_items: r.countable_items.clone(),
            involved_vouchers: r.involved_vouchers.clone(),
            involved_sources_details: r.involved_sources_details.as_ref().map(|v| v.iter().map(|i| i.clone().into()).collect()),
            bundle_data: r.bundle_data.clone(),
            bundle_id: r.bundle_id.clone(),
            notes: r.notes.clone(),
            sender_profile_name: r.sender_profile_name.clone(),
        }
    }
}

impl From<FrontendTransactionRecord> for TransactionRecord {
    fn from(r: FrontendTransactionRecord) -> Self {
        Self {
            id: r.id,
            direction: r.direction,
            recipient_id: r.recipient_id,
            sender_id: r.sender_id,
            timestamp: r.timestamp,
            summable_amounts: r.summable_amounts,
            countable_items: r.countable_items,
            involved_vouchers: r.involved_vouchers,
            involved_sources_details: r.involved_sources_details.map(|v| v.into_iter().map(|i| i.into()).collect()),
            bundle_data: r.bundle_data,
            bundle_id: r.bundle_id,
            notes: r.notes,
            sender_profile_name: r.sender_profile_name,
        }
    }
}
