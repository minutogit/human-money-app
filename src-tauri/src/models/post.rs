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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_manifest_json: Option<String>,
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
#[serde(rename_all = "camelCase")]
pub struct FrontendMarketInquiryItem {
    pub inquiry_id: String,
    pub role: String, // "seller" | "buyer"
    pub target: humoco_market::InquiryTarget,
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
