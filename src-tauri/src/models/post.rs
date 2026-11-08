use serde::{Deserialize, Serialize};

use crate::models::FrontendDropDescriptor;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPostDepositResult {
    pub deposited_to: usize,
    pub msg_id: String,
    pub errors: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPendingSignRequest {
    pub msg_id: String,
    pub sender_did: String,
    pub sender_name: Option<String>,
    pub created_at: u64,
    pub reply_drops: Vec<FrontendDropDescriptor>,
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
pub struct FrontendMarketInquiryReplyDto {
    pub inquiry_id: String,
    pub listing_id: String,
    pub decision: String, // "confirm" | "decline"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_uuid: Option<String>,
    pub replied_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMarketInquiryItem {
    pub inquiry_id: String,
    pub listing_id: String,
    pub listing_title: String,
    pub counterparty_did: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty_name: Option<String>,
    pub message: String,
    pub created_at: u64,
    pub reply_drops: Vec<FrontendDropDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<FrontendMarketInquiryReplyDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPostInboxSummary {
    pub pending_sign_requests: Vec<FrontendPendingSignRequest>,
    pub pending_wot_handshakes: Vec<FrontendPendingWotHandshake>,
    pub seller_inquiries: Vec<FrontendMarketInquiryItem>,
    pub buyer_inquiries: Vec<FrontendMarketInquiryItem>,
    pub unacknowledged_payments: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendPostSyncResult {
    pub uploaded_to: usize,
    pub polled_drops: usize,
    pub imported_payments: usize,
    pub imported_receipts: usize,
    pub imported_signatures: usize,
    pub new_sign_requests: usize,
    pub new_wot_handshakes: usize,
    pub new_inquiries: usize,
    pub new_inquiry_replies: usize,
    pub pruned_boxes: usize,
    pub errors: Vec<String>,
}
