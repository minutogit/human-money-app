use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendWalletEvent {
    pub event_id: String,
    pub local_instance_id: String,
    pub voucher_id: String,
    pub timestamp: String,
    pub event_type: FrontendWalletEventType,
    pub bff_data: FrontendEventBffData,
}

impl From<human_money_core::models::wallet_event::WalletEvent> for FrontendWalletEvent {
    fn from(e: human_money_core::models::wallet_event::WalletEvent) -> Self {
        Self {
            event_id: e.event_id,
            local_instance_id: e.local_instance_id,
            voucher_id: e.voucher_id,
            timestamp: e.timestamp.to_rfc3339(),
            event_type: e.event_type.into(),
            bff_data: e.bff_data.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum FrontendWalletEventType {
    VoucherCreated,
    TransferSent,
    TransferReceived,
    VoucherQuarantined,
    VoucherActivated,
    VoucherVoided,
    VoucherExpired,
    Unknown(String),
}

impl From<human_money_core::models::wallet_event::WalletEventType> for FrontendWalletEventType {
    fn from(t: human_money_core::models::wallet_event::WalletEventType) -> Self {
        match t {
            human_money_core::models::wallet_event::WalletEventType::VoucherCreated => Self::VoucherCreated,
            human_money_core::models::wallet_event::WalletEventType::TransferSent => Self::TransferSent,
            human_money_core::models::wallet_event::WalletEventType::TransferReceived => Self::TransferReceived,
            human_money_core::models::wallet_event::WalletEventType::VoucherQuarantined => Self::VoucherQuarantined,
            human_money_core::models::wallet_event::WalletEventType::VoucherActivated => Self::VoucherActivated,
            human_money_core::models::wallet_event::WalletEventType::VoucherVoided => Self::VoucherVoided,
            human_money_core::models::wallet_event::WalletEventType::VoucherExpired => Self::VoucherExpired,
            human_money_core::models::wallet_event::WalletEventType::Unknown(s) => Self::Unknown(s),
            _ => Self::Unknown("NewEnumValue".to_string()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendEventBffData {
    pub display_currency: String,
    pub amount: String,
    pub is_test_voucher: bool,
    pub counterparty_id: Option<String>,
    pub counterparty_name: Option<String>,
}

impl From<human_money_core::models::wallet_event::EventBffData> for FrontendEventBffData {
    fn from(d: human_money_core::models::wallet_event::EventBffData) -> Self {
        Self {
            display_currency: d.display_currency,
            amount: d.amount,
            is_test_voucher: d.is_test_voucher,
            counterparty_id: d.counterparty_id,
            counterparty_name: d.counterparty_name,
        }
    }
}
