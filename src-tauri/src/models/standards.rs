use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use human_money_core::models::voucher_standard_definition as core;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DynamicRuleDto {
    pub expression: String,
    pub message: String,
}

impl From<core::DynamicRule> for DynamicRuleDto {
    fn from(rule: core::DynamicRule) -> Self {
        Self {
            expression: rule.expression,
            message: rule.message,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImmutableIdentityDto {
    pub uuid: String,
    pub name: String,
    pub abbreviation: String,
}

impl From<core::ImmutableIdentity> for ImmutableIdentityDto {
    fn from(identity: core::ImmutableIdentity) -> Self {
        Self {
            uuid: identity.uuid,
            name: identity.name,
            abbreviation: identity.abbreviation,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PrimaryRedemptionTypeDto {
    GoodsOrServices,
    Time,
    PhysicalAsset,
}

impl From<core::PrimaryRedemptionType> for PrimaryRedemptionTypeDto {
    fn from(t: core::PrimaryRedemptionType) -> Self {
        match t {
            core::PrimaryRedemptionType::GoodsOrServices => Self::GoodsOrServices,
            core::PrimaryRedemptionType::Time => Self::Time,
            core::PrimaryRedemptionType::PhysicalAsset => Self::PhysicalAsset,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CollateralTypeDto {
    PersonalGuarantee,
    FiatBacked,
    CryptoBacked,
    PhysicalAsset,
}

impl From<core::CollateralType> for CollateralTypeDto {
    fn from(t: core::CollateralType) -> Self {
        match t {
            core::CollateralType::PersonalGuarantee => Self::PersonalGuarantee,
            core::CollateralType::FiatBacked => Self::FiatBacked,
            core::CollateralType::CryptoBacked => Self::CryptoBacked,
            core::CollateralType::PhysicalAsset => Self::PhysicalAsset,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyModeDto {
    Public,
    Stealth,
    Flexible,
}

impl From<core::PrivacyMode> for PrivacyModeDto {
    fn from(m: core::PrivacyMode) -> Self {
        match m {
            core::PrivacyMode::Public => Self::Public,
            core::PrivacyMode::Stealth => Self::Stealth,
            core::PrivacyMode::Flexible => Self::Flexible,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImmutableBlueprintDto {
    pub unit: String,
    pub primary_redemption_type: PrimaryRedemptionTypeDto,
    pub collateral_type: CollateralTypeDto,
}

impl From<core::ImmutableBlueprint> for ImmutableBlueprintDto {
    fn from(b: core::ImmutableBlueprint) -> Self {
        Self {
            unit: b.unit,
            primary_redemption_type: b.primary_redemption_type.into(),
            collateral_type: b.collateral_type.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImmutableFeaturesDto {
    pub allow_partial_transfers: bool,
    pub balances_are_summable: bool,
    pub amount_decimal_places: u8,
    pub privacy_mode: PrivacyModeDto,
    pub allowed_t_types: Vec<String>,
}

impl From<core::ImmutableFeatures> for ImmutableFeaturesDto {
    fn from(f: core::ImmutableFeatures) -> Self {
        Self {
            allow_partial_transfers: f.allow_partial_transfers,
            balances_are_summable: f.balances_are_summable,
            amount_decimal_places: f.amount_decimal_places,
            privacy_mode: f.privacy_mode.into(),
            allowed_t_types: f.allowed_t_types,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImmutableIssuanceDto {
    pub validity_duration_range: Vec<String>,
    pub issuance_minimum_validity_duration: String,
    pub additional_signatures_range: Vec<u32>,
    pub allowed_signature_roles: Vec<String>,
}

impl From<core::ImmutableIssuance> for ImmutableIssuanceDto {
    fn from(i: core::ImmutableIssuance) -> Self {
        Self {
            validity_duration_range: i.validity_duration_range,
            issuance_minimum_validity_duration: i.issuance_minimum_validity_duration,
            additional_signatures_range: i.additional_signatures_range,
            allowed_signature_roles: i.allowed_signature_roles,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImmutableZoneDto {
    pub identity: ImmutableIdentityDto,
    pub blueprint: ImmutableBlueprintDto,
    pub features: ImmutableFeaturesDto,
    pub issuance: ImmutableIssuanceDto,
    pub custom_rules: HashMap<String, DynamicRuleDto>,
}

impl From<core::ImmutableZone> for ImmutableZoneDto {
    fn from(z: core::ImmutableZone) -> Self {
        Self {
            identity: z.identity.into(),
            blueprint: z.blueprint.into(),
            features: z.features.into(),
            issuance: z.issuance.into(),
            custom_rules: z.custom_rules.into_iter().map(|(k, v)| (k, v.into())).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MutableMetadataDto {
    pub issuer_name: String,
    pub homepage_url: Option<String>,
    pub documentation_url: Option<String>,
    pub keywords: Vec<String>,
}

impl From<core::MutableMetadata> for MutableMetadataDto {
    fn from(m: core::MutableMetadata) -> Self {
        Self {
            issuer_name: m.issuer_name,
            homepage_url: m.homepage_url,
            documentation_url: m.documentation_url,
            keywords: m.keywords,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MutableAppConfigDto {
    pub default_validity_duration: Option<String>,
    pub round_up_validity_to: Option<String>,
    pub server_history_retention: Option<String>,
}

impl From<core::MutableAppConfig> for MutableAppConfigDto {
    fn from(c: core::MutableAppConfig) -> Self {
        Self {
            default_validity_duration: c.default_validity_duration,
            round_up_validity_to: c.round_up_validity_to,
            server_history_retention: c.server_history_retention,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MutableI18nDto {
    pub descriptions: HashMap<String, String>,
    pub footnotes: HashMap<String, String>,
    pub collateral_descriptions: HashMap<String, String>,
}

impl From<core::MutableI18n> for MutableI18nDto {
    fn from(i: core::MutableI18n) -> Self {
        Self {
            descriptions: i.descriptions,
            footnotes: i.footnotes,
            collateral_descriptions: i.collateral_descriptions,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MutableZoneDto {
    pub metadata: MutableMetadataDto,
    pub app_config: MutableAppConfigDto,
    pub i18n: MutableI18nDto,
}

impl From<core::MutableZone> for MutableZoneDto {
    fn from(z: core::MutableZone) -> Self {
        Self {
            metadata: z.metadata.into(),
            app_config: z.app_config.into(),
            i18n: z.i18n.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignatureBlockDto {
    pub issuer_id: String,
    pub signature: String,
}

impl From<core::SignatureBlock> for SignatureBlockDto {
    fn from(s: core::SignatureBlock) -> Self {
        Self {
            issuer_id: s.issuer_id,
            signature: s.signature,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VoucherStandardDefinitionDto {
    pub immutable: ImmutableZoneDto,
    pub mutable: MutableZoneDto,
    pub signature: Option<SignatureBlockDto>,
}

impl From<core::VoucherStandardDefinition> for VoucherStandardDefinitionDto {
    fn from(d: core::VoucherStandardDefinition) -> Self {
        Self {
            immutable: d.immutable.into(),
            mutable: d.mutable.into(),
            signature: d.signature.map(Into::into),
        }
    }
}
