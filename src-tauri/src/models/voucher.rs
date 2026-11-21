use super::profile::FrontendAddressData;
use human_money_core::wallet::instance::VoucherStatus;
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub abbreviation: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrontendCreatorData {
    #[serde(default)]
    pub protocol_version: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub address: Option<FrontendAddressData>,
    #[serde(default)]
    pub organization: Option<String>,
    #[serde(default)]
    pub community: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub gender: Option<String>,
    #[serde(default)]
    pub service_offer: Option<String>,
    #[serde(default)]
    pub needs: Option<String>,
    #[serde(default)]
    pub coordinates: Option<String>,
    #[serde(default)]
    pub picture_url: Option<String>,
}

impl From<human_money_core::models::profile::PublicProfile> for FrontendCreatorData {
    fn from(p: human_money_core::models::profile::PublicProfile) -> Self {
        Self {
            protocol_version: p.protocol_version,
            id: p.id,
            first_name: p.first_name,
            last_name: p.last_name,
            address: p.address.map(|a| a.into()),
            organization: p.organization,
            community: p.community,
            phone: p.phone,
            email: p.email,
            url: p.url,
            gender: p.gender,
            service_offer: p.service_offer,
            needs: p.needs,
            coordinates: p.coordinates,
            picture_url: p.picture_url,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendExpectedL2Committee {
    pub lock_pubs: Vec<String>,
    pub threshold: u8,
    pub source: String, // "live" | "cached" | "none"
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FrontendNewVoucherData {
    pub nominal_value: NominalValueData,
    pub creator: FrontendCreatorData,
    pub validity_duration: Option<String>,
    pub non_redeemable_test_voucher: bool,
    pub collateral: FrontendCollateralData,
    pub layer2_anchor: Option<String>,
    #[serde(default)]
    pub allow_offline_only: bool,
    #[serde(default)]
    pub expected_committee: Option<FrontendExpectedL2Committee>,
    #[serde(default)]
    pub force_reprobe: bool,
    #[serde(default)]
    pub explicit_offline_only: bool,
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
    pub allow_partial_transfers: bool,
    #[serde(default)]
    pub has_layer2_anchor: bool,
    #[serde(default)]
    pub trust: super::wot::FrontendVoucherTrustDto,
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
            description: String::new(),
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
            // L1 Core VoucherSummary does not carry standard definition features.
            // Full divisibility is resolved from parsed standards in the frontend.
            allow_partial_transfers: true,
            has_layer2_anchor: false,
            trust: super::wot::FrontendVoucherTrustDto::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoucherDetails {
    pub local_instance_id: String,
    pub status: String,
    pub current_amount: String,
    pub voucher: FrontendVoucher,
    pub display_currency: String,
    pub display_standard_name: String,
    pub is_test_voucher: bool,
    pub trust: super::wot::FrontendVoucherTrustDto,
    pub reduced_guarantor_protection: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_manifest_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_delegations_json: Option<String>,
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
        let current_amount = d.voucher.nominal_value.amount.clone();

        Self {
            local_instance_id: d.local_instance_id,
            status: status_str.to_string(),
            current_amount,
            voucher: d.voucher.into(),
            display_currency: d.display_currency,
            display_standard_name: d.display_standard_name,
            is_test_voucher: d.is_test_voucher,
            trust: super::wot::FrontendVoucherTrustDto::default(),
            reduced_guarantor_protection: false,
            manifest_revision: None,
            anchor_manifest_json: None,
            anchor_delegations_json: None,
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
    pub layer2_anchor: Option<String>,
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
            layer2_anchor: v.layer2_anchor.map(hex::encode),
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
}

impl From<human_money_core::models::voucher::VoucherStandard> for FrontendVoucherStandard {
    fn from(s: human_money_core::models::voucher::VoucherStandard) -> Self {
        Self {
            name: s.name,
            uuid: s.uuid,
            standard_definition_hash: s.standard_definition_hash,
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
    pub trap_r: String,
    pub trap_s: String,
}

impl From<human_money_core::models::voucher::TrapData> for FrontendTrapData {
    fn from(td: human_money_core::models::voucher::TrapData) -> Self {
        Self {
            ds_tag: td.ds_tag,
            trap_r: td.trap_r,
            trap_s: td.trap_s,
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

impl From<human_money_core::services::voucher_validation::SignatureImpact>
    for FrontendSignatureImpact
{
    fn from(i: human_money_core::services::voucher_validation::SignatureImpact) -> Self {
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

impl From<FrontendVoucherStandard> for human_money_core::models::voucher::VoucherStandard {
    fn from(s: FrontendVoucherStandard) -> Self {
        Self {
            name: s.name,
            uuid: s.uuid,
            standard_definition_hash: s.standard_definition_hash,
        }
    }
}

impl From<FrontendValueDefinition> for human_money_core::models::voucher::ValueDefinition {
    fn from(v: FrontendValueDefinition) -> Self {
        Self {
            unit: v.unit,
            amount: v.amount,
            abbreviation: v.abbreviation,
            description: v.description,
        }
    }
}

impl From<FrontendCollateral> for human_money_core::models::voucher::Collateral {
    fn from(c: FrontendCollateral) -> Self {
        Self {
            value: human_money_core::models::voucher::ValueDefinition {
                unit: c.unit,
                amount: c.amount,
                abbreviation: c.abbreviation,
                description: c.description,
            },
            collateral_type: c.collateral_type,
            redeem_condition: c.redeem_condition,
        }
    }
}

impl From<FrontendCreatorData> for human_money_core::models::profile::PublicProfile {
    fn from(c: FrontendCreatorData) -> Self {
        Self {
            protocol_version: c.protocol_version,
            id: c.id,
            first_name: c.first_name,
            last_name: c.last_name,
            address: c.address.map(|a| a.into()),
            organization: c.organization,
            community: c.community,
            phone: c.phone,
            email: c.email,
            url: c.url,
            gender: c.gender,
            coordinates: c.coordinates,
            service_offer: c.service_offer,
            needs: c.needs,
            picture_url: c.picture_url,
        }
    }
}

impl From<FrontendTrapData> for human_money_core::models::voucher::TrapData {
    fn from(td: FrontendTrapData) -> Self {
        Self {
            ds_tag: td.ds_tag,
            trap_r: td.trap_r,
            trap_s: td.trap_s,
        }
    }
}

impl From<FrontendTransaction> for human_money_core::models::voucher::Transaction {
    fn from(t: FrontendTransaction) -> Self {
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

impl From<FrontendVoucherSignature> for human_money_core::models::voucher::VoucherSignature {
    fn from(s: FrontendVoucherSignature) -> Self {
        Self {
            voucher_id: s.voucher_id,
            signature_id: s.signature_id,
            signer_id: s.signer_id,
            signature: s.signature,
            signature_time: s.signature_time,
            role: s.role,
            details: s.details.map(|d| d.into()),
        }
    }
}

impl TryFrom<FrontendVoucher> for human_money_core::models::voucher::Voucher {
    type Error = super::FrontendError;

    fn try_from(v: FrontendVoucher) -> Result<Self, Self::Error> {
        let layer2_anchor = match v
            .layer2_anchor
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(h) => Some(
                human_money_core::humoco_proto::decode_hex_fixed::<32>(h).map_err(|e| {
                    super::FrontendError::from(format!("Invalid layer2_anchor hex: {}", e))
                })?,
            ),
            None => None,
        };
        Ok(Self {
            voucher_standard: v.voucher_standard.into(),
            voucher_id: v.voucher_id,
            voucher_nonce: v.voucher_nonce,
            creation_date: v.creation_date,
            valid_until: v.valid_until,
            non_redeemable_test_voucher: v.non_redeemable_test_voucher,
            nominal_value: v.nominal_value.into(),
            collateral: v.collateral.map(|c| c.into()),
            creator_profile: v.creator.into(),
            transactions: v.transactions.into_iter().map(|t| t.into()).collect(),
            signatures: v.signatures.into_iter().map(|s| s.into()).collect(),
            layer2_anchor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s06_f3_voucher_try_from_rejects_malformed_anchor_and_preserves_creator_fields() {
        let orig_profile = human_money_core::models::profile::PublicProfile {
            id: Some("0:al@did:key:z6Mkexample".into()),
            first_name: Some("Alice".into()),
            last_name: None,
            address: None,
            gender: None,
            coordinates: None,
            picture_url: Some("https://example.org/avatar.png".into()),
            ..Default::default()
        };
        let fe_creator = FrontendCreatorData::from(orig_profile.clone());
        let roundtripped = human_money_core::models::profile::PublicProfile::from(fe_creator);
        assert_eq!(roundtripped.address, None);
        assert_eq!(roundtripped.gender, None);
        assert_eq!(roundtripped.coordinates, None);
        assert_eq!(roundtripped.last_name, None);
        assert_eq!(
            roundtripped.picture_url.as_deref(),
            Some("https://example.org/avatar.png")
        );

        let bad_voucher = FrontendVoucher {
            voucher_standard: FrontendVoucherStandard {
                name: "Test".into(),
                uuid: "uuid".into(),
                standard_definition_hash: "hash".into(),
            },
            voucher_id: "vid".into(),
            voucher_nonce: "nonce".into(),
            creation_date: "2026-01-01T00:00:00Z".into(),
            valid_until: "2027-01-01T00:00:00Z".into(),
            non_redeemable_test_voucher: false,
            nominal_value: FrontendValueDefinition {
                unit: "FT".into(),
                amount: "10".into(),
                abbreviation: None,
                description: None,
            },
            collateral: None,
            creator: FrontendCreatorData::from(orig_profile),
            layer2_anchor: Some("not-valid-hex".into()),
            transactions: vec![],
            signatures: vec![],
        };
        let err = human_money_core::models::voucher::Voucher::try_from(bad_voucher.clone());
        assert!(
            err.is_err(),
            "S06-F3: malformed non-empty layer2_anchor must fail TryFrom instead of becoming None"
        );

        let mut empty_anchor = bad_voucher;
        empty_anchor.layer2_anchor = Some("   ".into());
        let ok = human_money_core::models::voucher::Voucher::try_from(empty_anchor).unwrap();
        assert_eq!(ok.layer2_anchor, None);
    }
}
