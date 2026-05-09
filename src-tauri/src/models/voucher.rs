use serde::{Deserialize, Serialize};
use super::profile::FrontendAddressData;
use human_money_core::wallet::instance::VoucherStatus;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NominalValueData {
    pub amount: String,
    pub unit: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendCollateralData {
    pub amount: String,
    pub unit: String,
    pub abbreviation: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
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

impl From<human_money_core::models::profile::PublicProfile> for FrontendCreatorData {
    fn from(p: human_money_core::models::profile::PublicProfile) -> Self {
        Self {
            protocol_version: p.protocol_version,
            first_name: p.first_name.unwrap_or_default(),
            last_name: p.last_name.unwrap_or_default(),
            address: p.address.map(|a| a.into()).unwrap_or_default(),
            organization: p.organization,
            community: p.community,
            phone: p.phone,
            email: p.email,
            url: p.url,
            gender: p.gender.unwrap_or_default(),
            service_offer: p.service_offer,
            needs: p.needs,
            coordinates: p.coordinates.unwrap_or_default(),
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FrontendNewVoucherData {
    pub nominal_value: NominalValueData,
    pub creator: FrontendCreatorData,
    pub validity_duration: Option<String>,
    pub non_redeemable_test_voucher: bool,
    pub collateral: FrontendCollateralData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAssetClassSummary {
    pub standard_uuid: String,
    pub is_test_voucher: bool,
    pub display_standard_name: String,
    pub display_currency: String,
}

impl From<human_money_core::wallet::types::AssetClassSummary> for FrontendAssetClassSummary {
    fn from(s: human_money_core::wallet::types::AssetClassSummary) -> Self {
        Self {
            standard_uuid: s.standard_uuid,
            is_test_voucher: s.is_test_voucher,
            display_standard_name: s.display_standard_name,
            display_currency: s.display_currency,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoucherSummary {
    pub local_instance_id: String,
    pub status: String,
    pub creator_id: String,
    pub valid_until: String,
    pub description: String,
    pub current_amount: String,
    pub unit: String,
    pub raw_standard_name: String,
    pub voucher_standard_uuid: String,
    pub transaction_count: u32,
    pub signatures_count: u32,
    pub has_collateral: bool,
    pub creator_first_name: String,
    pub creator_last_name: String,
    pub creator_coordinates: String,
    pub is_test_voucher: bool,
    pub display_currency: String,
    pub display_standard_name: String,
}

impl From<human_money_core::wallet::types::VoucherSummary> for FrontendVoucherSummary {
    fn from(s: human_money_core::wallet::types::VoucherSummary) -> Self {
        let status_str = match s.status {
            VoucherStatus::Incomplete { .. } => "incomplete",
            VoucherStatus::Active => "active",
            VoucherStatus::Archived => "archived",
            VoucherStatus::Quarantined { .. } => "quarantined",
            VoucherStatus::Endorsed { .. } => "endorsed",
            VoucherStatus::Expired => "expired",
        };

        Self {
            local_instance_id: s.local_instance_id,
            status: status_str.to_string(),
            creator_id: s.creator_id,
            valid_until: s.valid_until,
            description: s.description,
            current_amount: s.current_amount,
            unit: s.unit,
            raw_standard_name: s.raw_standard_name,
            voucher_standard_uuid: s.voucher_standard_uuid,
            transaction_count: s.transaction_count,
            signatures_count: s.signatures_count,
            has_collateral: s.has_collateral,
            creator_first_name: s.creator_first_name,
            creator_last_name: s.creator_last_name,
            creator_coordinates: s.creator_coordinates,
            is_test_voucher: s.is_test_voucher,
            display_currency: s.display_currency,
            display_standard_name: s.display_standard_name,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoucherDetails {
    pub local_instance_id: String,
    pub status: String,
    pub voucher: FrontendVoucher,
    pub display_currency: String,
    pub display_standard_name: String,
    pub is_test_voucher: bool,
}

impl From<human_money_core::wallet::types::VoucherDetails> for FrontendVoucherDetails {
    fn from(d: human_money_core::wallet::types::VoucherDetails) -> Self {
        let status_str = match d.status {
            VoucherStatus::Incomplete { .. } => "incomplete",
            VoucherStatus::Active => "active",
            VoucherStatus::Archived => "archived",
            VoucherStatus::Quarantined { .. } => "quarantined",
            VoucherStatus::Endorsed { .. } => "endorsed",
            VoucherStatus::Expired => "expired",
        };

        Self {
            local_instance_id: d.local_instance_id,
            status: status_str.to_string(),
            voucher: d.voucher.into(),
            display_currency: d.display_currency,
            display_standard_name: d.display_standard_name,
            is_test_voucher: d.is_test_voucher,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoucher {
    pub voucher_standard: FrontendVoucherStandard,
    pub voucher_id: String,
    pub voucher_nonce: String,
    pub creation_date: String,
    pub valid_until: String,
    pub non_redeemable_test_voucher: bool,
    pub nominal_value: FrontendValueDefinition,
    pub collateral: Option<FrontendCollateral>,
    pub creator: FrontendCreatorData,
    pub transactions: Vec<FrontendTransaction>,
    pub signatures: Vec<FrontendVoucherSignature>,
}

impl From<human_money_core::models::voucher::Voucher> for FrontendVoucher {
    fn from(v: human_money_core::models::voucher::Voucher) -> Self {
        Self {
            voucher_standard: v.voucher_standard.into(),
            voucher_id: v.voucher_id,
            voucher_nonce: v.voucher_nonce,
            creation_date: v.creation_date,
            valid_until: v.valid_until,
            non_redeemable_test_voucher: v.non_redeemable_test_voucher,
            nominal_value: v.nominal_value.into(),
            collateral: v.collateral.map(|c| c.into()),
            creator: v.creator_profile.into(),
            transactions: v.transactions.into_iter().map(|t| t.into()).collect(),
            signatures: v.signatures.into_iter().map(|s| s.into()).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoucherStandard {
    pub name: String,
    pub uuid: String,
    pub standard_definition_hash: String,
    pub template: FrontendVoucherTemplateData,
}

impl From<human_money_core::models::voucher::VoucherStandard> for FrontendVoucherStandard {
    fn from(s: human_money_core::models::voucher::VoucherStandard) -> Self {
        Self {
            name: s.name,
            uuid: s.uuid,
            standard_definition_hash: s.standard_definition_hash,
            template: s.template.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoucherTemplateData {
    pub description: String,
    pub primary_redemption_type: String,
    pub allow_partial_transfers: bool,
    pub issuance_minimum_validity_duration: String,
    pub footnote: String,
}

impl From<human_money_core::models::voucher::VoucherTemplateData> for FrontendVoucherTemplateData {
    fn from(t: human_money_core::models::voucher::VoucherTemplateData) -> Self {
        Self {
            description: t.description,
            primary_redemption_type: t.primary_redemption_type,
            allow_partial_transfers: t.allow_partial_transfers,
            issuance_minimum_validity_duration: t.issuance_minimum_validity_duration,
            footnote: t.footnote,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendValueDefinition {
    pub unit: String,
    pub amount: String,
    pub abbreviation: Option<String>,
    pub description: Option<String>,
}

impl From<human_money_core::models::voucher::ValueDefinition> for FrontendValueDefinition {
    fn from(v: human_money_core::models::voucher::ValueDefinition) -> Self {
        Self {
            unit: v.unit,
            amount: v.amount,
            abbreviation: v.abbreviation,
            description: v.description,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendCollateral {
    pub unit: String,
    pub amount: String,
    pub abbreviation: Option<String>,
    pub description: Option<String>,
    pub collateral_type: Option<String>,
    pub redeem_condition: Option<String>,
}

impl From<human_money_core::models::voucher::Collateral> for FrontendCollateral {
    fn from(c: human_money_core::models::voucher::Collateral) -> Self {
        Self {
            unit: c.value.unit,
            amount: c.value.amount,
            abbreviation: c.value.abbreviation,
            description: c.value.description,
            collateral_type: c.collateral_type,
            redeem_condition: c.redeem_condition,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTransaction {
    pub t_id: String,
    pub t_type: String,
    pub t_time: String,
    pub prev_hash: String,
    pub receiver_ephemeral_pub_hash: Option<String>,
    pub sender_id: Option<String>,
    pub sender_identity_signature: Option<String>,
    pub recipient_id: String,
    pub amount: String,
    pub sender_remaining_amount: Option<String>,
    pub sender_ephemeral_pub: Option<String>,
    pub change_ephemeral_pub_hash: Option<String>,
    pub privacy_guard: Option<String>,
    pub trap_data: Option<FrontendTrapData>,
    pub layer2_signature: Option<String>,
    pub deletable_at: Option<String>,
}

impl From<human_money_core::models::voucher::Transaction> for FrontendTransaction {
    fn from(t: human_money_core::models::voucher::Transaction) -> Self {
        Self {
            t_id: t.t_id,
            t_type: t.t_type,
            t_time: t.t_time,
            prev_hash: t.prev_hash,
            receiver_ephemeral_pub_hash: t.receiver_ephemeral_pub_hash,
            sender_id: t.sender_id,
            sender_identity_signature: t.sender_identity_signature,
            recipient_id: t.recipient_id,
            amount: t.amount,
            sender_remaining_amount: t.sender_remaining_amount,
            sender_ephemeral_pub: t.sender_ephemeral_pub,
            change_ephemeral_pub_hash: t.change_ephemeral_pub_hash,
            privacy_guard: t.privacy_guard,
            trap_data: t.trap_data.map(|td| td.into()),
            layer2_signature: t.layer2_signature,
            deletable_at: t.deletable_at,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTrapData {
    pub ds_tag: String,
    pub u: String,
    pub blinded_id: String,
    pub proof: String,
}

impl From<human_money_core::models::voucher::TrapData> for FrontendTrapData {
    fn from(td: human_money_core::models::voucher::TrapData) -> Self {
        Self {
            ds_tag: td.ds_tag,
            u: td.u,
            blinded_id: td.blinded_id,
            proof: td.proof,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoucherSignature {
    pub voucher_id: String,
    pub signature_id: String,
    pub signer_id: String,
    pub signature: String,
    pub signature_time: String,
    pub role: String,
    pub details: Option<FrontendCreatorData>,
}

impl From<human_money_core::models::voucher::VoucherSignature> for FrontendVoucherSignature {
    fn from(s: human_money_core::models::voucher::VoucherSignature) -> Self {
        Self {
            voucher_id: s.voucher_id,
            signature_id: s.signature_id,
            signer_id: s.signer_id,
            signature: s.signature,
            signature_time: s.signature_time,
            role: s.role,
            details: s.details.map(|p| p.into()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSignatureImpact {
    pub is_allowed_role: bool,
    pub fatal_conflicts: Vec<String>,
    pub resolved_rules: Vec<String>,
    pub gentle_hints: Vec<String>,
}

impl From<human_money_core::services::signature_manager::SignatureImpact> for FrontendSignatureImpact {
    fn from(i: human_money_core::services::signature_manager::SignatureImpact) -> Self {
        Self {
            is_allowed_role: i.is_allowed_role,
            fatal_conflicts: i.fatal_conflicts,
            resolved_rules: i.resolved_rules,
            gentle_hints: i.gentle_hints,
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VoucherStandardInfo {
    pub id: String,
    pub display_name: String,
    pub content: String,
}
