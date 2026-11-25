use serde::{Deserialize, Serialize};

use crate::models::FrontendSyncDescriptor;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPostDepositResult {
    pub deposited_to: usize,
    #[serde(default)]
    pub recipient_deposited_to: usize,
    pub msg_id: String,
    #[serde(default)]
    pub deferred_count: usize,
    #[serde(default)]
    pub queued_for_next_flush: bool,
    pub errors: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPendingSignRequest {
    pub msg_id: String,
    pub sender_did: String,
    pub sender_name: Option<String>,
    pub created_at: u64,
    pub reply_syncs: Vec<FrontendSyncDescriptor>,
    pub container_bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPendingWotHandshake {
    pub msg_id: String,
    pub sender_did: String,
    pub sender_name: Option<String>,
    pub proposed_level: u8,
    pub reply_requested: bool,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendInquiryChatMessage {
    pub msg_id: String,
    pub sender: String, // "me" | "them"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>, // "confirm" | "decline"
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum FrontendInquiryTarget {
    Ad {
        #[serde(rename = "adId")]
        ad_id: String,
        #[serde(rename = "adTitle")]
        ad_title: String,
    },
    Person {
        #[serde(
            rename = "skillTitle",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        skill_title: Option<String>,
    },
}

impl From<FrontendInquiryTarget> for humoco_market::InquiryTarget {
    fn from(target: FrontendInquiryTarget) -> Self {
        match target {
            FrontendInquiryTarget::Ad { ad_id, ad_title } => Self::Ad { ad_id, ad_title },
            FrontendInquiryTarget::Person { skill_title } => Self::Person { skill_title },
        }
    }
}

impl From<humoco_market::InquiryTarget> for FrontendInquiryTarget {
    fn from(target: humoco_market::InquiryTarget) -> Self {
        match target {
            humoco_market::InquiryTarget::Ad { ad_id, ad_title } => Self::Ad { ad_id, ad_title },
            humoco_market::InquiryTarget::Person { skill_title } => Self::Person { skill_title },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMarketInquiryItem {
    pub inquiry_id: String,
    pub role: String, // "seller" | "buyer"
    pub target: FrontendInquiryTarget,
    pub counterparty_did: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty_picture_url: Option<String>,
    pub is_contact: bool,
    pub status: String, // "open" | "confirmed" | "declined"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_uuid: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub unread: bool,
    pub reply_syncs: Vec<FrontendSyncDescriptor>,
    pub messages: Vec<FrontendInquiryChatMessage>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendUnsupportedPostItem {
    pub msg_id: String,
    pub sender_did: String,
    pub sender_name: Option<String>,
    pub payload_type: String,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPendingTolerancePayment {
    pub msg_id: String,
    pub sender_did: String,
    pub sender_name: Option<String>,
    pub created_at: u64,
    pub zone: String, // "soft" | "critical"
    pub error_message: String,
    pub has_conflict: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPostInboxSummary {
    pub pending_sign_requests: Vec<FrontendPendingSignRequest>,
    pub pending_wot_handshakes: Vec<FrontendPendingWotHandshake>,
    pub inquiries: Vec<FrontendMarketInquiryItem>,
    pub unacknowledged_payments: Vec<String>,
    #[serde(default)]
    pub unsupported_messages: Vec<FrontendUnsupportedPostItem>,
    #[serde(default)]
    pub pending_tolerance_payments: Vec<FrontendPendingTolerancePayment>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPostSyncResult {
    pub uploaded_to: usize,
    pub polled_syncs: usize,
    pub imported_payments: usize,
    pub imported_receipts: usize,
    pub imported_signatures: usize,
    pub new_sign_requests: usize,
    pub new_wot_handshakes: usize,
    #[serde(default)]
    pub imported_wot_lists: usize,
    pub new_inquiries: usize,
    pub new_inquiry_messages: usize,
    #[serde(default)]
    pub new_tolerance_payments: usize,
    pub pruned_boxes: usize,
    #[serde(default)]
    pub deferred_outbox_boxes: usize,
    #[serde(default)]
    pub skipped_wifi_only: bool,
    pub errors: Vec<String>,
    pub retry_after_secs: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::FrontendInquiryTarget;

    #[test]
    fn s10_x1_frontend_inquiry_target_camel_case_wire_format_and_deny_unknown_fields() {
        let ad = FrontendInquiryTarget::Ad {
            ad_id: "0123456789abcdef".to_string(),
            ad_title: "Forest Honey".to_string(),
        };
        let ad_json = serde_json::to_string(&ad).unwrap();
        assert_eq!(
            ad_json,
            r#"{"kind":"ad","adId":"0123456789abcdef","adTitle":"Forest Honey"}"#
        );
        assert_eq!(
            serde_json::from_str::<FrontendInquiryTarget>(&ad_json).unwrap(),
            ad
        );

        let person_skill = FrontendInquiryTarget::Person {
            skill_title: Some("Carpentry".to_string()),
        };
        let person_skill_json = serde_json::to_string(&person_skill).unwrap();
        assert_eq!(
            person_skill_json,
            r#"{"kind":"person","skillTitle":"Carpentry"}"#
        );
        assert_eq!(
            serde_json::from_str::<FrontendInquiryTarget>(&person_skill_json).unwrap(),
            person_skill
        );

        let person_general = FrontendInquiryTarget::Person { skill_title: None };
        let person_general_json = serde_json::to_string(&person_general).unwrap();
        assert_eq!(person_general_json, r#"{"kind":"person"}"#);
        assert_eq!(
            serde_json::from_str::<FrontendInquiryTarget>(&person_general_json).unwrap(),
            person_general
        );

        // Zero-Compat: snake_case fields and unknown fields must be rejected.
        let snake_ad_err = serde_json::from_str::<FrontendInquiryTarget>(
            r#"{"kind":"ad","ad_id":"0123456789abcdef","adTitle":"Forest Honey"}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(
            snake_ad_err.contains("unknown field") && snake_ad_err.contains("ad_id"),
            "unexpected snake_case ad error: {snake_ad_err}"
        );

        let snake_person_err = serde_json::from_str::<FrontendInquiryTarget>(
            r#"{"kind":"person","skill_title":"Carpentry"}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(
            snake_person_err.contains("unknown field") && snake_person_err.contains("skill_title"),
            "unexpected snake_case person error: {snake_person_err}"
        );

        let extra_err = serde_json::from_str::<FrontendInquiryTarget>(
            r#"{"kind":"ad","adId":"0123456789abcdef","adTitle":"Forest Honey","extra":1}"#,
        )
        .unwrap_err()
        .to_string();
        assert!(
            extra_err.contains("unknown field") && extra_err.contains("extra"),
            "unexpected extra field error: {extra_err}"
        );
    }

    #[test]
    fn test_frontend_inquiry_target_market_roundtrip() {
        let ad = FrontendInquiryTarget::Ad {
            ad_id: "ad-42".into(),
            ad_title: "Organic Apples".into(),
        };
        let market_ad = humoco_market::InquiryTarget::from(ad.clone());
        assert_eq!(
            market_ad,
            humoco_market::InquiryTarget::Ad {
                ad_id: "ad-42".into(),
                ad_title: "Organic Apples".into(),
            }
        );
        assert_eq!(FrontendInquiryTarget::from(market_ad), ad);

        for person in [
            FrontendInquiryTarget::Person {
                skill_title: Some("Plumbing".into()),
            },
            FrontendInquiryTarget::Person { skill_title: None },
        ] {
            let market_person = humoco_market::InquiryTarget::from(person.clone());
            assert_eq!(FrontendInquiryTarget::from(market_person), person);
        }
    }
}
