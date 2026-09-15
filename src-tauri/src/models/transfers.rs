// src-tauri/src/models/transfers.rs
use serde::{Deserialize, Serialize};
use human_money_core::wallet::coin_selection::{RankedVoucher, ScoreBreakdown, SelectionReason, TransferPlan};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendScoreBreakdown {
    pub s_boomerang: i64,
    pub s_eigen: i64,
    pub s_corridor: i64,
    pub s_age: i64,
    pub s_chain: i64,
    pub s_fit: i64,
}

impl From<ScoreBreakdown> for FrontendScoreBreakdown {
    fn from(b: ScoreBreakdown) -> Self {
        Self {
            s_boomerang: b.s_boomerang,
            s_eigen: b.s_eigen,
            s_corridor: b.s_corridor,
            s_age: b.s_age,
            s_chain: b.s_chain,
            s_fit: b.s_fit,
        }
    }
}

impl From<FrontendScoreBreakdown> for ScoreBreakdown {
    fn from(b: FrontendScoreBreakdown) -> Self {
        Self {
            s_boomerang: b.s_boomerang,
            s_eigen: b.s_eigen,
            s_corridor: b.s_corridor,
            s_age: b.s_age,
            s_chain: b.s_chain,
            s_fit: b.s_fit,
        }
    }
}

fn reason_to_string(r: &SelectionReason) -> String {
    match r {
        SelectionReason::ExactMatch => "exactMatch".to_string(),
        SelectionReason::SubsetExact => "subsetExact".to_string(),
        SelectionReason::Greedy => "greedy".to_string(),
        SelectionReason::NotSelected => "notSelected".to_string(),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendRankedVoucher {
    pub local_instance_id: String,
    pub score: i64,
    pub breakdown: FrontendScoreBreakdown,
    pub reason: String,
    pub amount: String,
    pub standard_uuid: String,
}

impl From<RankedVoucher> for FrontendRankedVoucher {
    fn from(r: RankedVoucher) -> Self {
        Self {
            local_instance_id: r.local_instance_id,
            score: r.score,
            breakdown: r.breakdown.into(),
            reason: reason_to_string(&r.reason),
            amount: r.amount.to_string(),
            standard_uuid: r.standard_uuid,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTransferPlan {
    pub selected: Vec<FrontendRankedVoucher>,
    pub total_selected: String,
    pub target: String,
    pub needs_split: bool,
    pub plan_reason: String,
}

impl From<TransferPlan> for FrontendTransferPlan {
    fn from(p: TransferPlan) -> Self {
        Self {
            selected: p.selected.into_iter().map(|r| r.into()).collect(),
            total_selected: p.total_selected.to_string(),
            target: p.target.to_string(),
            needs_split: p.needs_split,
            plan_reason: reason_to_string(&p.plan_reason),
        }
    }
}
