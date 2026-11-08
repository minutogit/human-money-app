use serde::Serialize;
use std::collections::HashMap;

/// Structured error DTO for the React frontend.
/// Enables i18n localization via `react-i18next` interpolation.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendError {
    /// i18n translation key, e.g. "error.wallet.locked"
    pub code: String,
    /// English fallback text (original error message)
    pub message: String,
    /// Interpolation variables for i18next, e.g. {"expected": "100", "found": "50"}
    pub details: HashMap<String, String>,
}

impl From<String> for FrontendError {
    fn from(error_string: String) -> Self {
        let (code, details) = classify_error(&error_string);
        FrontendError {
            code,
            message: error_string,
            details,
        }
    }
}

impl From<humoco_market::MarketError> for FrontendError {
    fn from(error: humoco_market::MarketError) -> Self {
        FrontendError::from(error.to_string())
    }
}

impl From<human_money_core::crypto::GetPubkeyError> for FrontendError {
    fn from(error: human_money_core::crypto::GetPubkeyError) -> Self {
        FrontendError::from(error.to_string())
    }
}

impl From<humoco_wot::WotError> for FrontendError {
    fn from(error: humoco_wot::WotError) -> Self {
        FrontendError::from(error.to_string())
    }
}

impl From<human_money_core::app_service::AppFacadeError> for FrontendError {
    fn from(error: human_money_core::app_service::AppFacadeError) -> Self {
        let message = error.to_string();
        let mut details = HashMap::new();
        
        let code = match &error {
            human_money_core::app_service::AppFacadeError::WalletLocked => "error.wallet.locked".to_string(),
            human_money_core::app_service::AppFacadeError::SessionExpired(_) => "error.auth.sessionTimeout".to_string(),
            human_money_core::app_service::AppFacadeError::SessionNotActive(_) => "error.auth.sessionNotActive".to_string(),
            human_money_core::app_service::AppFacadeError::DoubleSpendAttemptBlocked { local_instance_id } => {
                details.insert("localInstanceId".to_string(), local_instance_id.clone());
                "error.conflict.doubleSpendBlocked".to_string()
            }
            human_money_core::app_service::AppFacadeError::RequiresSealRecovery => "error.security.sealRecoveryRequired".to_string(),
            human_money_core::app_service::AppFacadeError::StateRollbackDetected => "error.security.rollbackDetected".to_string(),
            human_money_core::app_service::AppFacadeError::WalletLockedDueToFork => "error.security.walletLockedDueToFork".to_string(),
            human_money_core::app_service::AppFacadeError::WalletSupersededByHandover { target_epoch, is_seed_recovery } => {
                details.insert("targetEpoch".to_string(), target_epoch.to_string());
                details.insert("isSeedRecovery".to_string(), is_seed_recovery.to_string());
                "error.security.supersededByHandover".to_string()
            }
            human_money_core::app_service::AppFacadeError::WalletConcurrentCloneDetected => "error.security.concurrentCloneDetected".to_string(),
            human_money_core::app_service::AppFacadeError::VoucherNotActive(status) => {
                if matches!(status, human_money_core::wallet::instance::VoucherStatus::Quarantined { .. }) {
                    "error.conflict.voucherQuarantined".to_string()
                } else {
                    "error.validation.voucherNotActive".to_string()
                }
            }
            human_money_core::app_service::AppFacadeError::Validation(val_err) => {
                let msg = val_err.to_string();
                if msg.contains("still in use") || msg.contains("in use by") {
                    "standards.inUseError".to_string()
                } else if msg.contains("Initial transaction amount must match") {
                    if let Some((expected, found)) = extract_expected_found(&msg) {
                        details.insert("expected".into(), expected);
                        details.insert("found".into(), found);
                    }
                    "error.validation.initAmountMismatch".to_string()
                } else if msg.contains("Insufficient funds") {
                    if let Some((needed, available)) = extract_two_labeled_values(&msg, "Needed:", "Available:") {
                        details.insert("needed".into(), needed);
                        details.insert("available".into(), available);
                    }
                    "error.validation.insufficientFunds".to_string()
                } else {
                    "error.validation.generic".to_string()
                }
            }
            _ => {
                let (c, d) = classify_error(&message);
                details = d;
                c
            }
        };

        FrontendError {
            code,
            message,
            details,
        }
    }
}

/// Maps known error message prefixes to i18n codes and extracts interpolation variables.
///
/// Uses stable prefixes from `thiserror` `#[error("...")]` macros in `human_money_core::error`.
/// Performance note: This runs only on error paths, never in hot loops — HashMap allocation
/// and String cloning are perfectly acceptable here for code clarity.
fn classify_error(msg: &str) -> (String, HashMap<String, String>) {
    let mut details = HashMap::new();

    // --- Security Errors (must precede generic wallet lock substring checks) ---
    if msg.starts_with("Security Notice: This wallet profile was transferred to another device") {
        return ("error.security.supersededByHandover".into(), details);
    }
    if msg.starts_with("Security Lockdown: Concurrent clone detected!") {
        return ("error.security.concurrentCloneDetected".into(), details);
    }
    if msg.starts_with("Security Lockdown:") {
        return ("error.security.walletLockedDueToFork".into(), details);
    }
    if msg.starts_with("Security Alert:") {
        return ("error.security.sealRecoveryRequired".into(), details);
    }
    if msg.starts_with("Critical Error: Wallet state manipulation") {
        return ("error.security.rollbackDetected".into(), details);
    }

    // --- Wallet State Errors ---
    if msg.contains("Wallet is locked") || msg.contains("AppService is locked") {
        return ("error.wallet.locked".into(), details);
    }
    if msg.starts_with("Session timed out") {
        return ("error.auth.sessionTimeout".into(), details);
    }
    if msg.starts_with("Password required") {
        return ("error.auth.passwordRequired".into(), details);
    }
    if msg.contains("device mismatch") || msg.contains("belongs to a different device") || msg.contains("Device Mismatch") {
        return ("error.auth.deviceMismatch".into(), details);
    }

    // --- Quarantine / Double Spend ---
    if msg.contains("quarantined due to a detected double-spend") {
        return ("error.conflict.voucherQuarantined".into(), details);
    }
    if msg.contains("Double spend attempt blocked") {
        return ("error.conflict.doubleSpendBlocked".into(), details);
    }

    // --- Validation Errors (with structured details extraction) ---
    if msg.contains("still in use") || msg.contains("in use by") {
        return ("standards.inUseError".into(), details);
    }

    if msg.contains("Initial transaction amount must match") {
        if let Some((expected, found)) = extract_expected_found(msg) {
            details.insert("expected".into(), expected);
            details.insert("found".into(), found);
        }
        return ("error.validation.initAmountMismatch".into(), details);
    }

    if msg.contains("Insufficient funds") {
        if let Some((needed, available)) = extract_two_labeled_values(msg, "Needed:", "Available:") {
            details.insert("needed".into(), needed);
            details.insert("available".into(), available);
        }
        return ("error.validation.insufficientFunds".into(), details);
    }

    // --- Bundle Processing ---
    if msg.starts_with("Transaction Rejected:") {
        return ("error.transfer.bundleTooOld".into(), details);
    }
    if msg.starts_with("CRITICAL WARNING:") {
        return ("error.transfer.bundleRecoveryZone".into(), details);
    }
    if msg.starts_with("Warning: This transaction occurred shortly") {
        return ("error.transfer.bundleToleranceZone".into(), details);
    }

    // --- Fallback: Unknown error ---
    ("error.internal.unknown".into(), details)
}

/// Lightweight parser using `split_once()` — no regex crate needed.
/// Extracts values from patterns like "... Expected: 100, Found: 50"
fn extract_expected_found(msg: &str) -> Option<(String, String)> {
    let expected_part = msg.split_once("Expected: ")?.1;
    let expected = expected_part.split(',').next()?.trim().to_string();

    let found_part = msg.split_once("Found: ")?.1;
    let found = found_part.trim_matches(|c: char| c == '.' || c == ' ').to_string();

    Some((expected, found))
}

/// Generic parser for two labeled values: "... Label1: X ... Label2: Y"
fn extract_two_labeled_values(msg: &str, label1: &str, label2: &str) -> Option<(String, String)> {
    let val1_part = msg.split_once(label1)?.1;
    let val1 = val1_part.split(',').next()?.trim().to_string();

    let val2_part = msg.split_once(label2)?.1;
    let val2 = val2_part.trim_matches(|c: char| c == '.' || c == ' ').to_string();

    Some((val1, val2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use human_money_core::app_service::AppFacadeError;

    #[test]
    fn test_error_mapping_superseded_by_handover() {
        let err = AppFacadeError::WalletSupersededByHandover {
            target_epoch: 4,
            is_seed_recovery: true,
        };
        let frontend_err = FrontendError::from(err);
        assert_eq!(frontend_err.code, "error.security.supersededByHandover");
        assert_eq!(frontend_err.details.get("targetEpoch"), Some(&"4".to_string()));
        assert_eq!(frontend_err.details.get("isSeedRecovery"), Some(&"true".to_string()));
    }

    #[test]
    fn test_error_mapping_concurrent_clone_detected() {
        let err = AppFacadeError::WalletConcurrentCloneDetected;
        let frontend_err = FrontendError::from(err);
        assert_eq!(frontend_err.code, "error.security.concurrentCloneDetected");
    }

    #[test]
    fn test_classify_error_fallback() {
        let (code1, _) = classify_error("Security Notice: This wallet profile was transferred to another device (target epoch #2, is_seed_recovery: false). Deactivated on this device.");
        assert_eq!(code1, "error.security.supersededByHandover");

        let (code2, _) = classify_error("Security Lockdown: Concurrent clone detected! Another device has performed transactions with the exact same prefix without a handover.");
        assert_eq!(code2, "error.security.concurrentCloneDetected");

        let (code3, _) = classify_error("Security Lockdown: Wallet is locked due to a detected fork in the transaction history. Recovery required.");
        assert_eq!(code3, "error.security.walletLockedDueToFork");
    }
}
