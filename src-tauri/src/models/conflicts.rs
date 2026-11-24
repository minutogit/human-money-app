use super::voucher::FrontendTransaction;
use super::wot::FrontendVoucherTrustDto;
use human_money_core::models::conflict::ConflictRole;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendProofOfDoubleSpendSummary {
    pub proof_id: String,
    pub offender_id: String,
    pub fork_point_prev_hash: String,
    pub report_timestamp: String,
    pub local_override: bool,
    pub endorsement_count: usize,
    pub local_note: Option<String>,
    pub conflict_role: String,
    pub affected_voucher_name: Option<String>,
    pub display_affected_voucher_name: Option<String>,
    pub voucher_standard_uuid: Option<String>,
    pub is_test_voucher: bool,
    pub is_cryptographically_attributed: bool,
}

impl From<human_money_core::wallet::types::ProofOfDoubleSpendSummary>
    for FrontendProofOfDoubleSpendSummary
{
    fn from(s: human_money_core::wallet::types::ProofOfDoubleSpendSummary) -> Self {
        let role_str = match s.conflict_role {
            ConflictRole::Victim => "victim",
            ConflictRole::Witness => "witness",
        };

        Self {
            proof_id: s.proof_id,
            is_cryptographically_attributed: human_money_core::ValidatedUserId::parse(
                &s.offender_id,
            )
            .is_ok(),
            offender_id: s.offender_id,
            fork_point_prev_hash: s.fork_point_prev_hash,
            report_timestamp: s.report_timestamp,
            local_override: s.local_override,
            endorsement_count: s.endorsement_count,
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
    pub affected_voucher_name: Option<String>,
    pub voucher_standard_uuid: Option<String>,
    pub non_redeemable_test_voucher: bool,
    pub is_cryptographically_attributed: bool,
    pub has_victim_witness: bool,
    pub victim_witness_count: usize,
    pub suspected_identity: Option<String>,
}

impl From<human_money_core::models::conflict::ProofOfDoubleSpend> for FrontendProofOfDoubleSpend {
    fn from(p: human_money_core::models::conflict::ProofOfDoubleSpend) -> Self {
        let mut conflicting_transactions: Vec<FrontendTransaction> = p
            .conflicting_transactions
            .into_iter()
            .map(|t| t.into())
            .collect();

        // Sort conflicting transactions by timestamp (earliest first) so the winner is index 0
        conflicting_transactions.sort_by(|a, b| {
            let ta = chrono::DateTime::parse_from_rfc3339(&a.t_time);
            let tb = chrono::DateTime::parse_from_rfc3339(&b.t_time);
            match (ta, tb) {
                (Ok(da), Ok(db)) => da.cmp(&db),
                _ => a.t_time.cmp(&b.t_time),
            }
        });

        let is_cryptographically_attributed =
            human_money_core::ValidatedUserId::parse(&p.offender_id).is_ok();
        let has_victim_witness = !p.victim_witnesses.is_empty();
        let victim_witness_count = p.victim_witnesses.len();
        let suspected_identity = p.suspected_identity.clone();

        Self {
            proof_id: p.proof_id,
            offender_id: p.offender_id,
            fork_point_prev_hash: p.fork_point_prev_hash,
            conflicting_transactions,
            deletable_at: p.deletable_at,
            reporter_id: p.reporter_id,
            report_timestamp: p.report_timestamp,
            reporter_signature: p.reporter_signature,
            resolutions: p
                .resolutions
                .map(|res| res.into_iter().map(|r| r.into()).collect()),
            affected_voucher_name: None,
            voucher_standard_uuid: None,
            non_redeemable_test_voucher: false,
            is_cryptographically_attributed,
            has_victim_witness,
            victim_witness_count,
            suspected_identity,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendResolutionEndorsement {
    pub endorsement_id: String,
    pub proof_id: String,
    pub endorser_id: String,
    pub resolution_timestamp: String,
    pub notes: Option<String>,
    pub endorser_signature: String,
    pub endorser_name: Option<String>,
    pub is_own_identity: bool,
    pub wot_trust: FrontendVoucherTrustDto,
}

impl From<human_money_core::models::conflict::ResolutionEndorsement>
    for FrontendResolutionEndorsement
{
    fn from(r: human_money_core::models::conflict::ResolutionEndorsement) -> Self {
        Self {
            endorsement_id: r.endorsement_id,
            proof_id: r.proof_id,
            endorser_id: r.endorser_id,
            resolution_timestamp: r.resolution_timestamp,
            notes: r.notes,
            endorser_signature: r.endorser_signature,
            endorser_name: None,
            is_own_identity: false,
            wot_trust: FrontendVoucherTrustDto::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum FrontendTrustStatus {
    Clean,
    KnownOffender(String),
    Resolved(String),
}

impl From<human_money_core::models::conflict::TrustStatus> for FrontendTrustStatus {
    fn from(s: human_money_core::models::conflict::TrustStatus) -> Self {
        match s {
            human_money_core::models::conflict::TrustStatus::Clean => Self::Clean,
            human_money_core::models::conflict::TrustStatus::KnownOffender(id) => {
                Self::KnownOffender(id)
            }
            human_money_core::models::conflict::TrustStatus::Resolved { proof_id, .. } => {
                Self::Resolved(proof_id)
            }
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FullProofDetails {
    pub proof: FrontendProofOfDoubleSpend,
    pub local_override: bool,
    pub local_note: Option<String>,
    pub conflict_role: String,
    pub can_endorse: bool,
    pub has_own_endorsement: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendProofImportResult {
    pub proof_id: String,
    pub already_known: bool,
    pub added_endorsements: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use human_money_core::models::conflict::{
        ProofOfDoubleSpend, ResolutionEndorsement, TrustStatus, VictimWitnessDisclosure,
    };
    use human_money_core::models::voucher::{Transaction, TrapWitness};
    use human_money_core::services::crypto::{create_user_id, generate_ed25519_keypair_for_tests};
    use human_money_core::wallet::types::ProofOfDoubleSpendSummary;

    fn make_dummy_summary(offender_id: &str, role: ConflictRole) -> ProofOfDoubleSpendSummary {
        ProofOfDoubleSpendSummary {
            proof_id: "p1".into(),
            offender_id: offender_id.into(),
            fork_point_prev_hash: "h".into(),
            report_timestamp: "2026-01-01T00:00:00Z".into(),
            local_override: true,
            endorsement_count: 2,
            local_note: Some("note".into()),
            conflict_role: role,
            affected_voucher_name: Some("Minuto".into()),
            display_affected_voucher_name: Some("Minuto".into()),
            voucher_standard_uuid: Some("uuid-1".into()),
            is_test_voucher: false,
        }
    }

    fn make_dummy_proof(offender_id: &str) -> ProofOfDoubleSpend {
        ProofOfDoubleSpend {
            proof_id: "p1".into(),
            offender_id: offender_id.into(),
            fork_point_prev_hash: "h".into(),
            conflicting_transactions: vec![],
            deletable_at: "2030-01-01T00:00:00Z".into(),
            reporter_id: "r".into(),
            report_timestamp: "2026-01-01T00:00:00Z".into(),
            reporter_signature: "sig".into(),
            resolutions: None,
            fingerprints: vec![],
            victim_witnesses: vec![],
            suspected_identity: None,
            affected_voucher_name: None,
            voucher_standard_uuid: None,
            non_redeemable_test_voucher: false,
        }
    }

    #[test]
    fn s06_f15_conflict_attribution_requires_valid_user_id() {
        let (pk, _) = generate_ed25519_keypair_for_tests(Some("s06-f15-valid"));
        let canonical_did = create_user_id(&pk, None).unwrap();
        let prefixed_did = create_user_id(&pk, Some("shop")).unwrap();

        // Positive baselines: canonical and prefixed did:key user IDs are cryptographically attributed
        for valid_id in [&canonical_did, &prefixed_did] {
            let fe_summary = FrontendProofOfDoubleSpendSummary::from(make_dummy_summary(
                valid_id,
                ConflictRole::Victim,
            ));
            assert!(
                fe_summary.is_cryptographically_attributed,
                "Valid user ID '{}' must be marked attributed on summary",
                valid_id
            );

            let fe_proof = FrontendProofOfDoubleSpend::from(make_dummy_proof(valid_id));
            assert!(
                fe_proof.is_cryptographically_attributed,
                "Valid user ID '{}' must be marked attributed on full proof",
                valid_id
            );
        }

        // Negative cases: malformed or substring-spoofed IDs must NOT be marked attributed
        for spoofed in ["not-a-did:key:fake", "evil:did:key:notvalid", "", "Unknown"] {
            let fe_summary = FrontendProofOfDoubleSpendSummary::from(make_dummy_summary(
                spoofed,
                ConflictRole::Witness,
            ));
            assert!(
                !fe_summary.is_cryptographically_attributed,
                "Spoofed offender_id '{}' must NOT be marked attributed",
                spoofed
            );

            let fe_proof = FrontendProofOfDoubleSpend::from(make_dummy_proof(spoofed));
            assert!(
                !fe_proof.is_cryptographically_attributed,
                "ProofOfDoubleSpend with spoofed offender_id '{}' must NOT be marked attributed",
                spoofed
            );
        }
    }

    #[test]
    fn test_conflict_dto_conversions_sort_rfc3339_map_roles_witnesses_and_trust() {
        let (pk, _) = generate_ed25519_keypair_for_tests(Some("dto-conv"));
        let valid_did = create_user_id(&pk, None).unwrap();

        let victim_summary =
            FrontendProofOfDoubleSpendSummary::from(make_dummy_summary(&valid_did, ConflictRole::Victim));
        assert_eq!(victim_summary.proof_id, "p1");
        assert_eq!(victim_summary.conflict_role, "victim");
        assert!(victim_summary.local_override);
        assert_eq!(victim_summary.endorsement_count, 2);
        assert_eq!(victim_summary.local_note.as_deref(), Some("note"));

        let witness_summary = FrontendProofOfDoubleSpendSummary::from(make_dummy_summary(
            &valid_did,
            ConflictRole::Witness,
        ));
        assert_eq!(witness_summary.conflict_role, "witness");

        // Empty victim_witnesses -> false / 0
        let empty_witness_proof = FrontendProofOfDoubleSpend::from(make_dummy_proof(&valid_did));
        assert!(!empty_witness_proof.has_victim_witness);
        assert_eq!(empty_witness_proof.victim_witness_count, 0);

        // RFC3339 sort: "2026-06-01T12:00:00+02:00" == 10:00:00Z (earlier) despite lexical > "11:00:00Z"
        let mut rich_proof = make_dummy_proof(&valid_did);
        rich_proof.conflicting_transactions = vec![
            Transaction {
                t_id: "later".into(),
                t_time: "2026-06-01T11:00:00Z".into(),
                ..Default::default()
            },
            Transaction {
                t_id: "earlier".into(),
                t_time: "2026-06-01T12:00:00+02:00".into(),
                ..Default::default()
            },
        ];
        rich_proof.victim_witnesses = vec![VictimWitnessDisclosure {
            fingerprint_index: 0,
            sender_permanent_did: valid_did.clone(),
            witness: TrapWitness::default(),
        }];
        rich_proof.suspected_identity = Some(valid_did.clone());
        rich_proof.resolutions = Some(vec![ResolutionEndorsement {
            endorsement_id: "e1".into(),
            proof_id: "p1".into(),
            endorser_id: valid_did.clone(),
            resolution_timestamp: "2026-06-02T00:00:00Z".into(),
            notes: Some("reimbursed".into()),
            endorser_signature: "esig".into(),
        }]);

        let fe_rich = FrontendProofOfDoubleSpend::from(rich_proof);
        assert_eq!(fe_rich.conflicting_transactions.len(), 2);
        assert_eq!(fe_rich.conflicting_transactions[0].t_id, "earlier");
        assert_eq!(fe_rich.conflicting_transactions[1].t_id, "later");
        assert!(fe_rich.has_victim_witness);
        assert_eq!(fe_rich.victim_witness_count, 1);
        assert_eq!(fe_rich.suspected_identity.as_deref(), Some(valid_did.as_str()));

        let res = fe_rich.resolutions.unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].endorsement_id, "e1");
        assert_eq!(res[0].notes.as_deref(), Some("reimbursed"));
        assert!(!res[0].is_own_identity);

        assert!(matches!(
            FrontendTrustStatus::from(TrustStatus::Clean),
            FrontendTrustStatus::Clean
        ));
        assert!(matches!(
            FrontendTrustStatus::from(TrustStatus::KnownOffender("p1".into())),
            FrontendTrustStatus::KnownOffender(ref id) if id == "p1"
        ));
        assert!(matches!(
            FrontendTrustStatus::from(TrustStatus::Resolved {
                proof_id: "p2".into(),
                note: Some("ok".into()),
            }),
            FrontendTrustStatus::Resolved(ref id) if id == "p2"
        ));
    }
}
