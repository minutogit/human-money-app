use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ReceiveCommitMode {
    #[default]
    OnlineVerify,
    ForceTrustOffline,
}

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
            MnemonicLanguage::ChineseSimplified => {
                human_money_core::MnemonicLanguage::ChineseSimplified
            }
            MnemonicLanguage::ChineseTraditional => {
                human_money_core::MnemonicLanguage::ChineseTraditional
            }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
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
    #[serde(default)]
    pub received_amount: String,
    #[serde(default)]
    pub own_available_amount: String,
    #[serde(default)]
    pub own_circulating_amount: String,
    #[serde(default)]
    pub net_balance: String,
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
    pub counterparty_id: Option<String>,
    pub counterparty_name: Option<String>,
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
            counterparty_id: i.counterparty_id,
            counterparty_name: i.counterparty_name,
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
            counterparty_id: i.counterparty_id,
            counterparty_name: i.counterparty_name,
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
pub struct TransactionRecord {
    pub id: String,
    pub direction: String, // "sent" or "received"
    pub recipient_id: String,
    pub sender_id: String,
    pub timestamp: String, // ISO 8601

    pub summable_amounts: std::collections::HashMap<String, String>,
    pub countable_items: std::collections::HashMap<String, u32>,

    pub involved_vouchers: Vec<String>, // local_instance_ids
    pub involved_sources_details: Option<Vec<FrontendInvolvedVoucherInfo>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bundle_data: Vec<u8>,
    pub bundle_id: String,
    pub notes: Option<String>,
    pub sender_profile_name: Option<String>,
}

/// Canonical persisted outgoing transfer bundle bytes keyed by `bundle_id`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredOutgoingBundle {
    pub bundle_id: String,
    pub recipient_id: String,
    pub created_at: String,
    pub bundle_bytes: Vec<u8>,
}

/// Encrypted store holding canonical outgoing transfer bundles for retry/download (`OUTGOING_BUNDLES_KEY`).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredOutgoingBundles {
    #[serde(default)]
    pub bundles: std::collections::BTreeMap<String, StoredOutgoingBundle>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSourceTransfer {
    pub local_instance_id: String,
    pub amount_to_send: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FrontendL2QuorumDto {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_nodes_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTransactionFingerprint {
    pub ds_tag: String,
    pub t_id: String,
    pub encrypted_timestamp: String,
    pub layer2_signature: String,
    pub sender_ephemeral_pub: String,
    pub deletable_at: String,
    pub trap_r: String,
    pub trap_s: String,
    pub layer2_voucher_id: String,
    pub privacy_guard_hash: String,
    pub created_at_ms: u64,
}

impl From<human_money_core::models::conflict::TransactionFingerprint>
    for FrontendTransactionFingerprint
{
    fn from(fp: human_money_core::models::conflict::TransactionFingerprint) -> Self {
        Self {
            ds_tag: fp.ds_tag,
            t_id: fp.t_id,
            encrypted_timestamp: fp.encrypted_timestamp.to_string(),
            layer2_signature: fp.layer2_signature,
            sender_ephemeral_pub: fp.sender_ephemeral_pub,
            deletable_at: fp.deletable_at,
            trap_r: fp.trap_r,
            trap_s: fp.trap_s,
            layer2_voucher_id: fp.layer2_voucher_id,
            privacy_guard_hash: fp.privacy_guard_hash,
            created_at_ms: fp.created_at_ms,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveSuccessPayload {
    pub sender_id: String,
    pub sender_profile_name: Option<String>,
    pub notes: Option<String>,
    pub transfer_summary: crate::models::FrontendTransferSummary,
    pub involved_vouchers: Vec<String>,
    pub involved_vouchers_details: Vec<crate::models::FrontendInvolvedVoucherInfo>,
    pub verifiable_conflicts:
        std::collections::HashMap<String, Vec<FrontendTransactionFingerprint>>,
    pub conflict_summaries: Vec<crate::models::FrontendProofOfDoubleSpendSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l2_quorum: Option<FrontendL2QuorumDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receive_kind: Option<String>,
    pub checkout_verdict: Option<crate::models::FrontendCheckoutReleaseVerdict>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purchase_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newly_committed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_replay: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wot_imported_lists: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unrelated_open_conflicts_count: Option<usize>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CreateBundleResult {
    pub bundle_data: Vec<u8>,
    pub bundle_id: String,
    pub involved_sources_details: Vec<crate::models::FrontendInvolvedVoucherInfo>,
    pub l2_dispatch: Vec<crate::gateway_resolver::IntentDispatchObservation>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoucherL2StatusDto {
    pub status: String,
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successor_t_id: Option<String>,
    pub reduced_guarantor_protection: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_revision: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::FrontendTransactionFingerprint;
    use human_money_core::models::conflict::TransactionFingerprint;

    #[test]
    fn frontend_transaction_fingerprint_serializes_v4_camel_case_without_legacy_fields() {
        let core_fp = TransactionFingerprint {
            ds_tag: "ds-1".to_string(),
            t_id: "tx-1".to_string(),
            encrypted_timestamp: 340282366920938463463374607431768211455,
            layer2_signature: "sig-1".to_string(),
            sender_ephemeral_pub: "eph-1".to_string(),
            deletable_at: "2028-01-01T00:00:00Z".to_string(),
            trap_r: "trap-r-1".to_string(),
            trap_s: "trap-s-1".to_string(),
            layer2_voucher_id: "l2-vid-1".to_string(),
            privacy_guard_hash: "pg-hash-1".to_string(),
            created_at_ms: 1_760_000_000_123,
        };

        let dto = FrontendTransactionFingerprint::from(core_fp);
        let value = serde_json::to_value(&dto).unwrap();
        let obj = value.as_object().expect("must serialize as JSON object");

        assert_eq!(obj.get("dsTag").and_then(|v| v.as_str()), Some("ds-1"));
        assert_eq!(obj.get("tId").and_then(|v| v.as_str()), Some("tx-1"));
        assert_eq!(
            obj.get("encryptedTimestamp").and_then(|v| v.as_str()),
            Some("340282366920938463463374607431768211455")
        );
        assert_eq!(
            obj.get("layer2Signature").and_then(|v| v.as_str()),
            Some("sig-1")
        );
        assert_eq!(
            obj.get("senderEphemeralPub").and_then(|v| v.as_str()),
            Some("eph-1")
        );
        assert_eq!(
            obj.get("deletableAt").and_then(|v| v.as_str()),
            Some("2028-01-01T00:00:00Z")
        );
        assert_eq!(obj.get("trapR").and_then(|v| v.as_str()), Some("trap-r-1"));
        assert_eq!(obj.get("trapS").and_then(|v| v.as_str()), Some("trap-s-1"));
        assert_eq!(
            obj.get("layer2VoucherId").and_then(|v| v.as_str()),
            Some("l2-vid-1")
        );
        assert_eq!(
            obj.get("privacyGuardHash").and_then(|v| v.as_str()),
            Some("pg-hash-1")
        );
        assert_eq!(
            obj.get("createdAtMs").and_then(|v| v.as_u64()),
            Some(1_760_000_000_123)
        );
        assert!(!obj.contains_key("u"));
        assert!(!obj.contains_key("blindedId"));
        assert!(!obj.contains_key("ds_tag"));
    }
}
