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

impl FrontendError {
    pub fn with_code(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: HashMap::new(),
        }
    }
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
        let message = error.to_string();
        let code = match &error {
            humoco_market::MarketError::Validation(_) => "error.market.validation",
            humoco_market::MarketError::Identity(_) => "error.market.identity",
            humoco_market::MarketError::Serialization(_) => "error.market.serialization",
            humoco_market::MarketError::ParseDate(_) => "error.market.parseDate",
            humoco_market::MarketError::Crypto(_) => "error.market.crypto",
            humoco_market::MarketError::InvalidJws(_) => "error.market.invalidCard",
            humoco_market::MarketError::Slot(_) => "error.market.slot",
            humoco_market::MarketError::PayloadTooLarge(_) => "error.market.payloadTooLarge",
        };
        FrontendError::with_code(code, message)
    }
}

impl From<human_money_core::crypto::GetPubkeyError> for FrontendError {
    fn from(error: human_money_core::crypto::GetPubkeyError) -> Self {
        FrontendError::from(error.to_string())
    }
}

impl From<humoco_wot::WotError> for FrontendError {
    fn from(error: humoco_wot::WotError) -> Self {
        let message = error.to_string();
        let code = match &error {
            humoco_wot::WotError::InvalidDid(_) => "error.wot.invalidDid",
            humoco_wot::WotError::WeakPublicKey
            | humoco_wot::WotError::InvalidSignature
            | humoco_wot::WotError::SignerMismatch { .. } => "error.wot.invalidSignature",
            humoco_wot::WotError::MalformedSdJwt(_) => "error.wot.malformedPackage",
            humoco_wot::WotError::InvalidTrustLevel(_) => "error.wot.invalidTrustLevel",
            humoco_wot::WotError::InvalidHopLimit(_) => "error.wot.invalidHopLimit",
            humoco_wot::WotError::SelfTrustForbidden => "error.wot.selfTrustForbidden",
            humoco_wot::WotError::FutureTimestamp { .. } => "error.wot.futureTimestamp",
            humoco_wot::WotError::EquivocationDetected { .. } => "error.wot.equivocation",
            humoco_wot::WotError::LimitExceeded(_) => "error.wot.limitExceeded",
            humoco_wot::WotError::Serialization(_) => "error.wot.serialization",
        };
        FrontendError::with_code(code, message)
    }
}

impl From<crate::sync_client::SyncClientError> for FrontendError {
    fn from(error: crate::sync_client::SyncClientError) -> Self {
        let message = error.to_string();
        let mut details = HashMap::new();
        let code = match &error {
            crate::sync_client::SyncClientError::LocalValidation(_) => "error.sync.localValidation",
            crate::sync_client::SyncClientError::Transport(_) => "error.sync.transport",
            crate::sync_client::SyncClientError::RateLimited {
                status,
                retry_after_secs,
                ..
            } => {
                details.insert("status".to_string(), status.to_string());
                details.insert("retryAfterSecs".to_string(), retry_after_secs.to_string());
                "error.sync.rateLimited"
            }
            crate::sync_client::SyncClientError::HttpStatus {
                status,
                retry_after_secs,
                ..
            } => {
                details.insert("status".to_string(), status.to_string());
                if let Some(secs) = retry_after_secs {
                    details.insert("retryAfterSecs".to_string(), secs.to_string());
                }
                "error.sync.httpStatus"
            }
            crate::sync_client::SyncClientError::ResponseTooLarge { max_bytes, .. } => {
                details.insert("maxBytes".to_string(), max_bytes.to_string());
                "error.sync.responseTooLarge"
            }
            crate::sync_client::SyncClientError::InvalidPayload(_) => "error.sync.invalidPayload",
        };
        FrontendError {
            code: code.to_string(),
            message,
            details,
        }
    }
}

impl From<human_money_core::app_service::AppFacadeError> for FrontendError {
    fn from(error: human_money_core::app_service::AppFacadeError) -> Self {
        let message = error.to_string();
        let mut details = HashMap::new();

        let code = match &error {
            human_money_core::app_service::AppFacadeError::WalletLocked => {
                "error.wallet.locked".to_string()
            }
            human_money_core::app_service::AppFacadeError::SessionExpired(_)
            | human_money_core::app_service::AppFacadeError::App(
                human_money_core::error::AppError::SessionExpired { .. },
            ) => "error.auth.sessionTimeout".to_string(),
            human_money_core::app_service::AppFacadeError::SessionNotActive(_)
            | human_money_core::app_service::AppFacadeError::App(
                human_money_core::error::AppError::SessionNotActive { .. },
            ) => "error.auth.sessionNotActive".to_string(),
            human_money_core::app_service::AppFacadeError::DoubleSpendAttemptBlocked {
                local_instance_id,
            } => {
                details.insert("localInstanceId".to_string(), local_instance_id.clone());
                "error.conflict.doubleSpendBlocked".to_string()
            }
            human_money_core::app_service::AppFacadeError::RequiresSealRecovery => {
                "error.security.sealRecoveryRequired".to_string()
            }
            human_money_core::app_service::AppFacadeError::StateRollbackDetected => {
                "error.security.rollbackDetected".to_string()
            }
            human_money_core::app_service::AppFacadeError::WalletLockedDueToFork => {
                "error.security.walletLockedDueToFork".to_string()
            }
            human_money_core::app_service::AppFacadeError::WalletSupersededByHandover {
                target_epoch,
                is_seed_recovery,
            } => {
                details.insert("targetEpoch".to_string(), target_epoch.to_string());
                details.insert("isSeedRecovery".to_string(), is_seed_recovery.to_string());
                "error.security.supersededByHandover".to_string()
            }
            human_money_core::app_service::AppFacadeError::WalletConcurrentCloneDetected => {
                "error.security.concurrentCloneDetected".to_string()
            }
            human_money_core::app_service::AppFacadeError::App(
                human_money_core::error::AppError::InsufficientAccessMode { required, current },
            ) => {
                details.insert("required".to_string(), required.clone());
                details.insert("current".to_string(), current.clone());
                "error.auth.insufficientAccessMode".to_string()
            }
            human_money_core::app_service::AppFacadeError::VoucherNotActive(status) => {
                if matches!(
                    status,
                    human_money_core::wallet::instance::VoucherStatus::Quarantined { .. }
                ) {
                    "error.conflict.voucherQuarantined".to_string()
                } else {
                    "error.validation.voucherNotActive".to_string()
                }
            }
            human_money_core::app_service::AppFacadeError::InvalidPayloadType => {
                "error.validation.unsupportedPayloadType".to_string()
            }
            human_money_core::app_service::AppFacadeError::Validation(val_err) => {
                let msg = val_err.to_string();
                if let Some((c, d)) = classify_validation_message(&msg) {
                    details = d;
                    c
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

fn classify_validation_message(msg: &str) -> Option<(String, HashMap<String, String>)> {
    let mut details = HashMap::new();
    if msg.contains("still in use") || msg.contains("in use by") {
        return Some(("standards.inUseError".into(), details));
    }
    if msg.contains("Initial transaction amount must match") {
        if let Some((expected, found)) = extract_expected_found(msg) {
            details.insert("expected".into(), expected);
            details.insert("found".into(), found);
        }
        return Some(("error.validation.initAmountMismatch".into(), details));
    }
    if msg.contains("Insufficient funds") {
        if let Some((needed, available)) = extract_two_labeled_values(msg, "Needed:", "Available:")
        {
            details.insert("needed".into(), needed);
            details.insert("available".into(), available);
        }
        return Some(("error.validation.insufficientFunds".into(), details));
    }
    if msg.contains("Split change voucher is blocked by unresolved or conflicted parent intent") {
        return Some(("error.validation.splitChangePendingL2".into(), details));
    }
    if msg.contains("Output has an unresolved durable L2 send intent") {
        return Some(("error.validation.intentPendingL2".into(), details));
    }
    if msg.contains("bundle missing AnchorManifest sidecar") {
        return Some(("error.validation.anchorManifestMissing".into(), details));
    }
    None
}

/// Maps known error message prefixes to i18n codes and extracts interpolation variables.
///
/// Uses stable prefixes from `thiserror` `#[error("...")]` macros in `human_money_core::error`.
/// Performance note: This runs only on error paths, never in hot loops — HashMap allocation
/// and String cloning are perfectly acceptable here for code clarity.
fn classify_error(msg: &str) -> (String, HashMap<String, String>) {
    let details = HashMap::new();

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
    if msg.contains("device mismatch")
        || msg.contains("belongs to a different device")
        || msg.contains("Device Mismatch")
    {
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
    if let Some(hit) = classify_validation_message(msg) {
        return hit;
    }

    // --- Bundle Processing ---
    if msg.contains("Invalid payload type in secure container") {
        return ("error.validation.unsupportedPayloadType".into(), details);
    }
    if msg.starts_with("Transaction Rejected:") {
        return ("error.transfer.bundleTooOld".into(), details);
    }
    if msg.starts_with("CRITICAL WARNING:") {
        return ("error.transfer.bundleRecoveryZone".into(), details);
    }
    if msg.starts_with("Warning: This transaction occurred shortly") {
        return ("error.transfer.bundleToleranceZone".into(), details);
    }

    // --- Post & Social Recovery Errors ---
    if msg.contains("Recipient sync inbox unreachable") {
        return ("error.post.recipientUnreachable".into(), details);
    }
    if msg.contains("answers to the security questions") {
        return ("error.recovery.wrongAnswers".into(), details);
    }
    if msg.contains("BIP39_PASSPHRASE_REQUIRED") {
        return ("error.recovery.bip39PassphraseRequired".into(), details);
    }
    if msg.contains("SYNC_UNREACHABLE") {
        return ("error.recovery.syncUnreachable".into(), details);
    }
    if msg.contains("Guarantor anchor verification failed") {
        return ("error.voucher.guarantorAnchorCheckFailed".into(), details);
    }

    // --- Fallback: Unknown error ---
    ("error.internal.unknown".into(), details)
}

/// Lightweight parser using `split_once()` — no regex crate needed.
/// Extracts values from patterns like "... Expected: 100, Found: 50"
fn extract_expected_found(msg: &str) -> Option<(String, String)> {
    extract_two_labeled_values(msg, "Expected:", "Found:")
}

/// Generic parser for two labeled values: "... Label1: X ... Label2: Y" regardless of order.
fn extract_two_labeled_values(msg: &str, label1: &str, label2: &str) -> Option<(String, String)> {
    let extract = |label: &str| -> Option<String> {
        let part = msg.split_once(label)?.1;
        Some(
            part.split(',')
                .next()?
                .trim_matches(|c: char| c == '.' || c == ' ')
                .to_string(),
        )
    };

    Some((extract(label1)?, extract(label2)?))
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
        assert_eq!(
            frontend_err.details.get("targetEpoch"),
            Some(&"4".to_string())
        );
        assert_eq!(
            frontend_err.details.get("isSeedRecovery"),
            Some(&"true".to_string())
        );
    }

    #[test]
    fn test_error_mapping_concurrent_clone_detected() {
        let err = AppFacadeError::WalletConcurrentCloneDetected;
        let frontend_err = FrontendError::from(err);
        assert_eq!(frontend_err.code, "error.security.concurrentCloneDetected");
    }

    #[test]
    fn test_classify_error_fallback() {
        let (code1, _) = classify_error(
            "Security Notice: This wallet profile was transferred to another device (target epoch #2, is_seed_recovery: false). Deactivated on this device.",
        );
        assert_eq!(code1, "error.security.supersededByHandover");

        let (code2, _) = classify_error(
            "Security Lockdown: Concurrent clone detected! Another device has performed transactions with the exact same prefix without a handover.",
        );
        assert_eq!(code2, "error.security.concurrentCloneDetected");

        let (code3, _) = classify_error(
            "Security Lockdown: Wallet is locked due to a detected fork in the transaction history. Recovery required.",
        );
        assert_eq!(code3, "error.security.walletLockedDueToFork");
    }

    #[test]
    fn test_error_mapping_pending_l2_intents() {
        let split_err = AppFacadeError::Validation(
            human_money_core::error::ValidationError::SecurityViolation(
                "Split change voucher is blocked by unresolved or conflicted parent intent".into(),
            ),
        );
        let mapped_split = FrontendError::from(split_err);
        assert_eq!(mapped_split.code, "error.validation.splitChangePendingL2");

        let intent_err = AppFacadeError::Validation(
            human_money_core::error::ValidationError::SecurityViolation(
                "Output has an unresolved durable L2 send intent".into(),
            ),
        );
        let mapped_intent = FrontendError::from(intent_err);
        assert_eq!(mapped_intent.code, "error.validation.intentPendingL2");

        let (classified_split, _) = classify_error(
            "Validation Error: Security violation: Split change voucher is blocked by unresolved or conflicted parent intent",
        );
        assert_eq!(classified_split, "error.validation.splitChangePendingL2");

        let (classified_intent, _) = classify_error(
            "Validation Error: Security violation: Output has an unresolved durable L2 send intent",
        );
        assert_eq!(classified_intent, "error.validation.intentPendingL2");
    }

    #[test]
    fn test_error_mapping_forward_compat_anchor_and_payload_type() {
        let missing_anchor_err = AppFacadeError::Validation(
            human_money_core::error::ValidationError::SecurityViolation(
                "bundle missing AnchorManifest sidecar".into(),
            ),
        );
        let mapped_anchor = FrontendError::from(missing_anchor_err);
        assert_eq!(mapped_anchor.code, "error.validation.anchorManifestMissing");

        let (classified_anchor, _) = classify_error(
            "Validation Error: Security violation: bundle missing AnchorManifest sidecar",
        );
        assert_eq!(classified_anchor, "error.validation.anchorManifestMissing");

        let payload_err = AppFacadeError::InvalidPayloadType;
        let mapped_payload = FrontendError::from(payload_err);
        assert_eq!(
            mapped_payload.code,
            "error.validation.unsupportedPayloadType"
        );

        let (classified_payload, _) = classify_error("Invalid payload type in secure container.");
        assert_eq!(
            classified_payload,
            "error.validation.unsupportedPayloadType"
        );
    }

    #[test]
    fn test_from_market_error_maps_all_variants() {
        use humoco_market::MarketError;
        let cases = [
            (MarketError::Validation("bad".into()), "error.market.validation"),
            (MarketError::Identity("id".into()), "error.market.identity"),
            (
                MarketError::Serialization("json".into()),
                "error.market.serialization",
            ),
            (MarketError::ParseDate("date".into()), "error.market.parseDate"),
            (MarketError::Crypto("enc".into()), "error.market.crypto"),
            (MarketError::InvalidJws("jws".into()), "error.market.invalidCard"),
            (MarketError::Slot("slot".into()), "error.market.slot"),
            (
                MarketError::PayloadTooLarge("big".into()),
                "error.market.payloadTooLarge",
            ),
        ];
        for (err, expected_code) in cases {
            let expected_msg = err.to_string();
            let fe = FrontendError::from(err);
            assert_eq!(fe.code, expected_code);
            assert_eq!(fe.message, expected_msg);
            assert!(fe.details.is_empty());
        }
    }

    #[test]
    fn test_from_wot_error_maps_all_variants() {
        use humoco_wot::WotError;
        let cases = [
            (WotError::InvalidDid("x".into()), "error.wot.invalidDid"),
            (WotError::WeakPublicKey, "error.wot.invalidSignature"),
            (WotError::InvalidSignature, "error.wot.invalidSignature"),
            (
                WotError::SignerMismatch {
                    expected: "a".into(),
                    actual: "b".into(),
                },
                "error.wot.invalidSignature",
            ),
            (
                WotError::MalformedSdJwt("sd".into()),
                "error.wot.malformedPackage",
            ),
            (
                WotError::InvalidTrustLevel(9),
                "error.wot.invalidTrustLevel",
            ),
            (WotError::InvalidHopLimit(0), "error.wot.invalidHopLimit"),
            (
                WotError::SelfTrustForbidden,
                "error.wot.selfTrustForbidden",
            ),
            (
                WotError::FutureTimestamp {
                    claim_secs: 20,
                    now_secs: 10,
                },
                "error.wot.futureTimestamp",
            ),
            (
                WotError::EquivocationDetected {
                    issuer: "did:key:z1".into(),
                    version: 3,
                },
                "error.wot.equivocation",
            ),
            (
                WotError::LimitExceeded("max".into()),
                "error.wot.limitExceeded",
            ),
            (
                WotError::Serialization("ser".into()),
                "error.wot.serialization",
            ),
        ];
        for (err, expected_code) in cases {
            let expected_msg = err.to_string();
            let fe = FrontendError::from(err);
            assert_eq!(fe.code, expected_code);
            assert_eq!(fe.message, expected_msg);
        }
    }

    #[test]
    fn test_from_sync_client_error_maps_all_variants_and_details() {
        use crate::sync_client::SyncClientError;

        let local = FrontendError::from(SyncClientError::LocalValidation("bad url".into()));
        assert_eq!(local.code, "error.sync.localValidation");
        assert_eq!(local.message, "bad url");
        assert!(local.details.is_empty());

        let transport = FrontendError::from(SyncClientError::Transport("timeout".into()));
        assert_eq!(transport.code, "error.sync.transport");
        assert_eq!(transport.message, "timeout");

        let rate = FrontendError::from(SyncClientError::RateLimited {
            status: 429,
            retry_after_secs: 30,
            context: "upload".into(),
            snippet: "slow down".into(),
        });
        assert_eq!(rate.code, "error.sync.rateLimited");
        assert_eq!(rate.details.get("status").map(String::as_str), Some("429"));
        assert_eq!(
            rate.details.get("retryAfterSecs").map(String::as_str),
            Some("30")
        );

        let http_with_retry = FrontendError::from(SyncClientError::HttpStatus {
            status: 503,
            retry_after_secs: Some(12),
            context: "poll".into(),
            snippet: "busy".into(),
        });
        assert_eq!(http_with_retry.code, "error.sync.httpStatus");
        assert_eq!(
            http_with_retry.details.get("status").map(String::as_str),
            Some("503")
        );
        assert_eq!(
            http_with_retry
                .details
                .get("retryAfterSecs")
                .map(String::as_str),
            Some("12")
        );

        let http_no_retry = FrontendError::from(SyncClientError::HttpStatus {
            status: 404,
            retry_after_secs: None,
            context: "get".into(),
            snippet: "missing".into(),
        });
        assert_eq!(http_no_retry.code, "error.sync.httpStatus");
        assert_eq!(
            http_no_retry.details.get("status").map(String::as_str),
            Some("404")
        );
        assert!(!http_no_retry.details.contains_key("retryAfterSecs"));

        let too_large = FrontendError::from(SyncClientError::ResponseTooLarge {
            max_bytes: 4096,
            context: "slot".into(),
        });
        assert_eq!(too_large.code, "error.sync.responseTooLarge");
        assert_eq!(
            too_large.details.get("maxBytes").map(String::as_str),
            Some("4096")
        );

        let invalid = FrontendError::from(SyncClientError::InvalidPayload("corrupt".into()));
        assert_eq!(invalid.code, "error.sync.invalidPayload");
        assert_eq!(invalid.message, "corrupt");
    }

    #[test]
    fn test_from_app_facade_error_maps_all_variants() {
        use human_money_core::error::AppError;
        use human_money_core::wallet::instance::VoucherStatus;

        assert_eq!(
            FrontendError::from(AppFacadeError::WalletLocked).code,
            "error.wallet.locked"
        );
        assert_eq!(
            FrontendError::from(AppFacadeError::SessionExpired("exp".into())).code,
            "error.auth.sessionTimeout"
        );
        assert_eq!(
            FrontendError::from(AppFacadeError::App(AppError::SessionExpired {
                reason: "ttl".into(),
            }))
            .code,
            "error.auth.sessionTimeout"
        );
        assert_eq!(
            FrontendError::from(AppFacadeError::SessionNotActive("none".into())).code,
            "error.auth.sessionNotActive"
        );
        assert_eq!(
            FrontendError::from(AppFacadeError::App(AppError::SessionNotActive {
                reason: "locked".into(),
            }))
            .code,
            "error.auth.sessionNotActive"
        );

        let ds = FrontendError::from(AppFacadeError::DoubleSpendAttemptBlocked {
            local_instance_id: "inst-42".into(),
        });
        assert_eq!(ds.code, "error.conflict.doubleSpendBlocked");
        assert_eq!(
            ds.details.get("localInstanceId").map(String::as_str),
            Some("inst-42")
        );

        assert_eq!(
            FrontendError::from(AppFacadeError::RequiresSealRecovery).code,
            "error.security.sealRecoveryRequired"
        );
        assert_eq!(
            FrontendError::from(AppFacadeError::StateRollbackDetected).code,
            "error.security.rollbackDetected"
        );
        assert_eq!(
            FrontendError::from(AppFacadeError::WalletLockedDueToFork).code,
            "error.security.walletLockedDueToFork"
        );

        let mode = FrontendError::from(AppFacadeError::App(AppError::InsufficientAccessMode {
            required: "Full".into(),
            current: "ReadOnly".into(),
        }));
        assert_eq!(mode.code, "error.auth.insufficientAccessMode");
        assert_eq!(
            mode.details.get("required").map(String::as_str),
            Some("Full")
        );
        assert_eq!(
            mode.details.get("current").map(String::as_str),
            Some("ReadOnly")
        );

        assert_eq!(
            FrontendError::from(AppFacadeError::VoucherNotActive(
                VoucherStatus::Quarantined {
                    reason: "fork".into(),
                }
            ))
            .code,
            "error.conflict.voucherQuarantined"
        );
        assert_eq!(
            FrontendError::from(AppFacadeError::VoucherNotActive(VoucherStatus::Active)).code,
            "error.validation.voucherNotActive"
        );

        let generic_val = FrontendError::from(AppFacadeError::Validation(
            human_money_core::error::ValidationError::InvalidBundleSignature,
        ));
        assert_eq!(generic_val.code, "error.validation.generic");

        let fallback_quarantine = FrontendError::from(AppFacadeError::VoucherInQuarantine);
        assert_eq!(fallback_quarantine.code, "error.conflict.voucherQuarantined");

        let insufficient_facade = FrontendError::from(AppFacadeError::InsufficientFunds {
            available: "20".parse().unwrap(),
            needed: "50".parse().unwrap(),
        });
        assert_eq!(
            insufficient_facade.code,
            "error.validation.insufficientFunds"
        );
        assert_eq!(
            insufficient_facade.details.get("needed").map(String::as_str),
            Some("50")
        );
        assert_eq!(
            insufficient_facade
                .details
                .get("available")
                .map(String::as_str),
            Some("20")
        );
    }

    #[test]
    fn test_classify_validation_message_and_value_extractors() {
        assert_eq!(
            classify_validation_message("Standard is still in use")
                .unwrap()
                .0,
            "standards.inUseError"
        );
        assert_eq!(
            classify_validation_message("Template in use by 3 vouchers")
                .unwrap()
                .0,
            "standards.inUseError"
        );

        let (code_init, details_init) = classify_validation_message(
            "Initial transaction amount must match nominal value. Expected: 100, Found: 50.",
        )
        .unwrap();
        assert_eq!(code_init, "error.validation.initAmountMismatch");
        assert_eq!(
            details_init.get("expected").map(String::as_str),
            Some("100")
        );
        assert_eq!(details_init.get("found").map(String::as_str), Some("50"));

        let (code_init_no_labels, details_init_no_labels) =
            classify_validation_message("Initial transaction amount must match").unwrap();
        assert_eq!(code_init_no_labels, "error.validation.initAmountMismatch");
        assert!(details_init_no_labels.is_empty());

        let (code_funds, details_funds) =
            classify_validation_message("Insufficient funds. Needed: 50, Available: 20. ").unwrap();
        assert_eq!(code_funds, "error.validation.insufficientFunds");
        assert_eq!(details_funds.get("needed").map(String::as_str), Some("50"));
        assert_eq!(
            details_funds.get("available").map(String::as_str),
            Some("20")
        );

        let (code_funds_no_labels, details_funds_no_labels) =
            classify_validation_message("Insufficient funds").unwrap();
        assert_eq!(code_funds_no_labels, "error.validation.insufficientFunds");
        assert!(details_funds_no_labels.is_empty());

        assert_eq!(
            extract_expected_found("Expected: 100, Found: 50. "),
            Some(("100".to_string(), "50".to_string()))
        );
        assert_eq!(extract_expected_found("Expected: 100 without found"), None);
        assert_eq!(extract_expected_found("Found: 50 without expected"), None);

        assert_eq!(
            extract_two_labeled_values("Needed: 75, Available: 12.50.", "Needed:", "Available:"),
            Some(("75".to_string(), "12.50".to_string()))
        );
        assert_eq!(
            extract_two_labeled_values("Available: 12.50, Needed: 75.", "Needed:", "Available:"),
            Some(("75".to_string(), "12.50".to_string()))
        );
        assert_eq!(
            extract_two_labeled_values("Needed: 75", "Needed:", "Available:"),
            None
        );
        assert!(classify_validation_message("unrelated error").is_none());
    }

    #[test]
    fn test_classify_error_all_string_branches_and_from_string() {
        let cases = [
            ("Security Alert: missing seal", "error.security.sealRecoveryRequired"),
            (
                "Critical Error: Wallet state manipulation detected",
                "error.security.rollbackDetected",
            ),
            ("Wallet is locked.", "error.wallet.locked"),
            ("AppService is locked", "error.wallet.locked"),
            ("Session timed out after idle", "error.auth.sessionTimeout"),
            ("Password required to unlock", "error.auth.passwordRequired"),
            ("Seal device mismatch occurred", "error.auth.deviceMismatch"),
            ("Wallet belongs to a different device", "error.auth.deviceMismatch"),
            ("Device Mismatch: wrong host", "error.auth.deviceMismatch"),
            (
                "Action aborted: quarantined due to a detected double-spend",
                "error.conflict.voucherQuarantined",
            ),
            (
                "Double spend attempt blocked for v1",
                "error.conflict.doubleSpendBlocked",
            ),
            (
                "Initial transaction amount must match nominal value. Expected: 10, Found: 5.",
                "error.validation.initAmountMismatch",
            ),
            ("Transaction Rejected: too old", "error.transfer.bundleTooOld"),
            (
                "CRITICAL WARNING: recovery zone",
                "error.transfer.bundleRecoveryZone",
            ),
            (
                "Warning: This transaction occurred shortly before recovery",
                "error.transfer.bundleToleranceZone",
            ),
            (
                "Recipient sync inbox unreachable at host",
                "error.post.recipientUnreachable",
            ),
            (
                "Incorrect answers to the security questions",
                "error.recovery.wrongAnswers",
            ),
            (
                "BIP39_PASSPHRASE_REQUIRED to decrypt",
                "error.recovery.bip39PassphraseRequired",
            ),
            (
                "SYNC_UNREACHABLE while polling",
                "error.recovery.syncUnreachable",
            ),
            (
                "Guarantor anchor verification failed for v1",
                "error.voucher.guarantorAnchorCheckFailed",
            ),
            ("Something completely unexpected", "error.internal.unknown"),
        ];
        for (input, expected_code) in cases {
            let (code, _) = classify_error(input);
            assert_eq!(code, expected_code, "input was: {input}");
        }

        let from_str = FrontendError::from("Wallet is locked.".to_string());
        assert_eq!(from_str.code, "error.wallet.locked");
        assert_eq!(from_str.message, "Wallet is locked.");

        let pubkey_err = FrontendError::from(
            human_money_core::crypto::GetPubkeyError::InvalidPrefix,
        );
        assert_eq!(pubkey_err.code, "error.internal.unknown");
        assert!(pubkey_err.message.contains("Invalid prefix"));
    }
}
