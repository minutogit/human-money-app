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

impl From<human_money_core::app_service::AppFacadeError> for FrontendError {
    fn from(error: human_money_core::app_service::AppFacadeError) -> Self {
        let message = error.to_string();
        let mut details = HashMap::new();
        
        let code = match &error {
            human_money_core::app_service::AppFacadeError::WalletLocked(_) => "error.wallet.locked".to_string(),
            human_money_core::app_service::AppFacadeError::SessionExpired(_) => "error.auth.sessionTimeout".to_string(),
            human_money_core::app_service::AppFacadeError::SessionNotActive(_) => "error.auth.sessionNotActive".to_string(),
            human_money_core::app_service::AppFacadeError::DoubleSpendAttemptBlocked(local_id) => {
                details.insert("localInstanceId".to_string(), local_id.clone());
                "error.conflict.doubleSpendBlocked".to_string()
            }
            human_money_core::app_service::AppFacadeError::RequiresSealRecovery(_) => "error.security.sealRecoveryRequired".to_string(),
            human_money_core::app_service::AppFacadeError::StateRollbackDetected(_) => "error.security.rollbackDetected".to_string(),
            human_money_core::app_service::AppFacadeError::WalletLockedDueToFork(_) => "error.security.walletLockedDueToFork".to_string(),
            human_money_core::app_service::AppFacadeError::VoucherNotActive(status_msg) => {
                if status_msg.contains("Quarantined") {
                    "error.conflict.voucherQuarantined".to_string()
                } else {
                    "error.validation.voucherNotActive".to_string()
                }
            }
            human_money_core::app_service::AppFacadeError::ValidationError(msg) => {
                if msg.contains("Initial transaction amount must match") {
                    if let Some((expected, found)) = extract_expected_found(msg) {
                        details.insert("expected".into(), expected);
                        details.insert("found".into(), found);
                    }
                    "error.validation.initAmountMismatch".to_string()
                } else if msg.contains("Insufficient funds") {
                    if let Some((needed, available)) = extract_two_labeled_values(msg, "Needed:", "Available:") {
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

    // --- Security Errors ---
    if msg.starts_with("Security Alert:") {
        return ("error.security.sealRecoveryRequired".into(), details);
    }
    if msg.starts_with("Critical Error: Wallet state manipulation") {
        return ("error.security.rollbackDetected".into(), details);
    }
    if msg.starts_with("Security Lockdown:") {
        return ("error.security.walletLockedDueToFork".into(), details);
    }

    // --- Quarantine / Double Spend ---
    if msg.contains("quarantined due to a detected double-spend") {
        return ("error.conflict.voucherQuarantined".into(), details);
    }
    if msg.contains("Double spend attempt blocked") {
        return ("error.conflict.doubleSpendBlocked".into(), details);
    }

    // --- Validation Errors (with structured details extraction) ---
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
