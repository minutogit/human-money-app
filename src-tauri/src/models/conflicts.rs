use serde::{Deserialize, Serialize};
use super::voucher::FrontendTransaction;
use human_money_core::models::conflict::ConflictRole;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendProofOfDoubleSpendSummary {
    pub proof_id: String,
    pub offender_id: String,
    pub fork_point_prev_hash: String,
    pub report_timestamp: String,
    pub is_resolved: bool,
    pub has_l2_verdict: bool,
    pub local_override: bool,
    pub local_note: Option<String>,
    pub conflict_role: String,
    pub affected_voucher_name: Option<String>,
    pub display_affected_voucher_name: Option<String>,
    pub voucher_standard_uuid: Option<String>,
    pub is_test_voucher: bool,
}

impl From<human_money_core::wallet::types::ProofOfDoubleSpendSummary> for FrontendProofOfDoubleSpendSummary {
    fn from(s: human_money_core::wallet::types::ProofOfDoubleSpendSummary) -> Self {
        let role_str = match s.conflict_role {
            ConflictRole::Victim => "victim",
            ConflictRole::Witness => "witness",
        };

        Self {
            proof_id: s.proof_id,
            offender_id: s.offender_id,
            fork_point_prev_hash: s.fork_point_prev_hash,
            report_timestamp: s.report_timestamp,
            is_resolved: s.is_resolved,
            has_l2_verdict: s.has_l2_verdict,
            local_override: s.local_override,
            local_note: s.local_note,
            conflict_role: role_str.to_string(),
            affected_voucher_name: s.affected_voucher_name,
            display_affected_voucher_name: s.display_affected_voucher_name,
            voucher_standard_uuid: s.voucher_standard_uuid,
            is_test_voucher: s.is_test_voucher,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendProofOfDoubleSpend {
    pub proof_id: String,
    pub offender_id: String,
    pub fork_point_prev_hash: String,
    pub conflicting_transactions: Vec<FrontendTransaction>,
    pub deletable_at: String,
    pub reporter_id: String,
    pub report_timestamp: String,
    pub reporter_signature: String,
    pub resolutions: Option<Vec<FrontendResolutionEndorsement>>,
    pub layer2_verdict: Option<serde_json::Value>,
    pub affected_voucher_name: Option<String>,
    pub voucher_standard_uuid: Option<String>,
    pub is_resolved: Option<bool>,
    pub non_redeemable_test_voucher: bool,
}

impl From<human_money_core::models::conflict::ProofOfDoubleSpend> for FrontendProofOfDoubleSpend {
    fn from(p: human_money_core::models::conflict::ProofOfDoubleSpend) -> Self {
        Self {
            proof_id: p.proof_id,
            offender_id: p.offender_id,
            fork_point_prev_hash: p.fork_point_prev_hash,
            conflicting_transactions: p.conflicting_transactions.into_iter().map(|t| t.into()).collect(),
            deletable_at: p.deletable_at,
            reporter_id: p.reporter_id,
            report_timestamp: p.report_timestamp,
            reporter_signature: p.reporter_signature,
            resolutions: p.resolutions.map(|res| res.into_iter().map(|r| r.into()).collect()),
            layer2_verdict: p.layer2_verdict.map(|v| serde_json::to_value(v).unwrap()),
            affected_voucher_name: None,
            voucher_standard_uuid: None,
            is_resolved: None,
            non_redeemable_test_voucher: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendResolutionEndorsement {
    pub endorsement_id: String,
    pub proof_id: String,
    pub victim_id: String,
    pub resolution_timestamp: String,
    pub notes: Option<String>,
    pub victim_signature: String,
}

impl From<human_money_core::models::conflict::ResolutionEndorsement> for FrontendResolutionEndorsement {
    fn from(r: human_money_core::models::conflict::ResolutionEndorsement) -> Self {
        Self {
            endorsement_id: r.endorsement_id,
            proof_id: r.proof_id,
            victim_id: r.victim_id,
            resolution_timestamp: r.resolution_timestamp,
            notes: r.notes,
            victim_signature: r.victim_signature,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum FrontendTrustStatus {
    Clean,
    KnownOffender(String),
    Resolved(String, bool),
}

impl From<human_money_core::models::conflict::TrustStatus> for FrontendTrustStatus {
    fn from(s: human_money_core::models::conflict::TrustStatus) -> Self {
        match s {
            human_money_core::models::conflict::TrustStatus::Clean => Self::Clean,
            human_money_core::models::conflict::TrustStatus::KnownOffender(id) => Self::KnownOffender(id),
            human_money_core::models::conflict::TrustStatus::Resolved { proof_id, is_local, .. } => Self::Resolved(proof_id, is_local),
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FullProofDetails {
    pub proof: FrontendProofOfDoubleSpend,
    pub local_override: bool,
    pub local_note: Option<String>,
    pub conflict_role: String,
}
