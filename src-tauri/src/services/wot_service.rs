use crate::models::{
    FrontendError, FrontendUserProfile, FrontendVoucherTrustDto, FrontendWotImportResultDto,
    FrontendWotProviderDto, FrontendWotSummaryDto,
};
use crate::state::AppState;
use chrono::Utc;
use human_money_core::models::secure_container::{ContainerConfig, PrivacyMode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WotFileEnvelope {
    pub format_version: u8,
    pub sender_did: String,
    pub sender_profile: Option<FrontendUserProfile>,
    pub sync_package: humoco_wot::WotSyncPackage,
}

fn looks_like_provider_filter(bytes: &[u8]) -> Option<&str> {
    if bytes.len() > humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN {
        return None;
    }
    let s = std::str::from_utf8(bytes).ok()?.trim();
    let val: serde_json::Value = serde_json::from_str(s).ok()?;
    if val.get("typ").and_then(|v| v.as_str()) == Some(humoco_wot::PROVIDER_FILTER_TYP) {
        Some(s)
    } else {
        None
    }
}

fn summary_from_register(reg: &humoco_wot::TrustRegister) -> FrontendWotSummaryDto {
    let rated_contacts_count = reg.own_contacts.values().filter(|e| e.level >= 1).count();
    let verified_humans_count = reg.own_contacts.values().filter(|e| e.level == 0).count();
    let close_confidants_count = reg.own_contacts.values().filter(|e| e.level == 3).count();
    let imported_peer_lists_count = reg.raw_list_store.len();
    let own_list_version = reg.own_signed_list.as_ref().map(|l| l.v);
    let own_list_max_hops = reg.policy.own_list_max_hops;
    let provider_filters_count = reg.provider_filters.len();

    FrontendWotSummaryDto {
        rated_contacts_count,
        verified_humans_count,
        close_confidants_count,
        imported_peer_lists_count,
        own_list_version,
        own_list_max_hops,
        provider_filters_count,
    }
}

fn providers_from_register(
    reg: &humoco_wot::TrustRegister,
    now_secs: u64,
) -> Vec<FrontendWotProviderDto> {
    let mut list: Vec<FrontendWotProviderDto> = reg
        .provider_filters
        .values()
        .map(|p| provider_filter_to_dto(p, reg, now_secs, reg.policy.freshness_grace_secs))
        .collect();

    list.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.issuer_did.cmp(&b.issuer_did))
    });
    list
}

fn is_known_contact_for_register(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
    target_raw: &str,
    canonical_target: Option<&str>,
) -> bool {
    let book_opt = state.cache_contacts().or_else(|| {
        crate::state::load_encrypted_json_or_default::<crate::models::FrontendAddressBook>(
            service,
            crate::state::CONTACTS_DATA_NAME,
            password,
        )
        .ok()
    });
    book_opt
        .as_ref()
        .map(|book| {
            book.contacts.keys().any(|k| {
                k == target_raw
                    || (canonical_target.is_some()
                        && crate::state::canonicalize_did(k).ok().as_deref() == canonical_target)
            })
        })
        .unwrap_or(false)
}

fn provider_filter_to_dto(
    prov: &humoco_wot::StoredProviderFilter,
    reg: &humoco_wot::TrustRegister,
    now_secs: u64,
    grace_secs: u64,
) -> FrontendWotProviderDto {
    let is_stale = now_secs.saturating_sub(prov.payload.iat) > grace_secs;
    let direct_trust = reg
        .own_contacts
        .get(&prov.payload.iss)
        .map(|e| e.level)
        .unwrap_or(0);
    let trust_level = prov.local_trust_level.max(direct_trust);

    FrontendWotProviderDto {
        issuer_did: prov.payload.iss.clone(),
        name: prov.payload.name.clone(),
        version: prov.payload.v,
        issued_at: prov.payload.iat,
        source_url: prov.payload.source_url.clone(),
        member_count: prov.payload.member_count,
        trust_level,
        imported_at: prov.imported_at,
        is_stale,
    }
}

fn import_provider_filter_to_dto(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    raw_json: &str,
    password: Option<&str>,
) -> Result<FrontendWotImportResultDto, FrontendError> {
    let now_secs = Utc::now().timestamp().max(0) as u64;
    let mut reg = state
        .get_or_load_wot_register(service, password)
        .map_err(FrontendError::from)?;

    let iss = reg
        .import_provider_filter(raw_json, now_secs)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    state
        .save_wot_register(service, &reg, password)
        .map_err(FrontendError::from)?;

    let stored = reg.provider_filters.get(&iss).ok_or_else(|| {
        FrontendError::from("Imported provider filter not found in register".to_string())
    })?;

    let is_known_contact =
        is_known_contact_for_register(state, service, password, &iss, Some(&iss));

    let existing_trust_level = Some(stored.local_trust_level);

    Ok(FrontendWotImportResultDto {
        sender_did: iss,
        sender_profile: None,
        imported: 1,
        updated: 0,
        merged: 0,
        skipped_unchanged: 0,
        rejected_spam: 0,
        is_known_contact,
        existing_trust_level,
        is_provider_filter: true,
        provider_name: Some(stored.payload.name.clone()),
        provider_member_count: Some(stored.payload.member_count),
    })
}

pub fn get_wot_summary_impl(state: &AppState) -> Result<FrontendWotSummaryDto, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let reg = state
        .get_or_load_wot_register(&mut service, None)
        .map_err(FrontendError::from)?;
    Ok(summary_from_register(&reg))
}

pub(crate) fn apply_desired_hops_to_cached_contacts(
    state: &AppState,
    reg: &humoco_wot::TrustRegister,
) {
    state.with_cached_contacts_mut(|opt| {
        if let Some(book) = opt {
            for c in book.contacts.values_mut() {
                if let Ok(can) = crate::state::canonicalize_did(&c.did) {
                    if let Some(entry) = reg.own_contacts.get(&can) {
                        c.desired_hops = Some(entry.desired_hops);
                    }
                }
            }
        }
    });
}

pub(crate) fn prepare_wot_max_hops(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    max_hops: u8,
    pw: Option<&str>,
) -> Result<humoco_wot::TrustRegister, FrontendError> {
    if !(1..=3).contains(&max_hops) {
        return Err(FrontendError::from(
            "maxHops must be between 1 and 3".to_string(),
        ));
    }

    let mut reg = state
        .get_or_load_wot_register(service, pw)
        .map_err(FrontendError::from)?;

    let now_secs = Utc::now().timestamp().max(0) as u64;
    reg.set_own_list_max_hops(max_hops, now_secs)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    reg.clear_drop_sent_log(None);

    state
        .ensure_wot_list_signed(service, &mut reg)
        .map_err(FrontendError::from)?;
    Ok(reg)
}

pub(crate) fn set_wot_max_hops_with_service(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    max_hops: u8,
    pw: Option<&str>,
) -> Result<humoco_wot::TrustRegister, FrontendError> {
    let reg = prepare_wot_max_hops(state, service, max_hops, pw)?;
    state
        .save_wot_register(service, &reg, pw)
        .map_err(FrontendError::from)?;
    apply_desired_hops_to_cached_contacts(state, &reg);
    Ok(reg)
}

pub fn set_wot_max_hops_impl(
    max_hops: u8,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendWotSummaryDto, FrontendError> {
    if !(1..=3).contains(&max_hops) {
        return Err(FrontendError::from(
            "maxHops must be between 1 and 3".to_string(),
        ));
    }

    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let reg = set_wot_max_hops_with_service(state, &mut service, max_hops, pw)?;
    Ok(summary_from_register(&reg))
}

pub fn export_wot_package_impl(
    recipient_did: Option<String>,
    container_password: Option<String>,
    share_with_all_contacts: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<u8>, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let (sender_did, sender_profile) = service
        .with_wallet(|w| {
            (
                w.get_user_id().to_string(),
                Some(w.profile.to_public_profile().into()),
            )
        })
        .map_err(FrontendError::from)?;

    let mut reg = state
        .get_or_load_wot_register(&mut service, pw)
        .map_err(FrontendError::from)?;

    let signed_changed = state
        .ensure_wot_list_signed(&mut service, &mut reg)
        .map_err(FrontendError::from)?;
    if signed_changed {
        state
            .save_wot_register(&mut service, &reg, pw)
            .map_err(FrontendError::from)?;
    }

    let share_all = share_with_all_contacts.unwrap_or(false);

    let mut has_single_recipient = false;
    let (sync_package, receipt, config) = if share_all {
        let target_dids: Vec<String> = reg
            .own_contacts
            .iter()
            .filter(|(_, entry)| entry.level >= 1)
            .take(human_money_core::models::secure_container::MAX_RECIPIENTS)
            .map(|(did, _)| did.clone())
            .collect();

        if target_dids.is_empty() {
            return Err(FrontendError::from(
                "No rated contacts (level 1-3) available to encrypt for".to_string(),
            ));
        }

        let (pkg, rcpt) = reg
            .prepare_sync_package(None, Some(2_000_000), true)
            .map_err(|e| FrontendError::from(e.to_string()))?;

        (
            pkg,
            rcpt,
            ContainerConfig::TargetDids(target_dids, PrivacyMode::TrialDecryption),
        )
    } else {
        let rcpt_can = recipient_did
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                crate::state::canonicalize_did(s).map_err(|e| FrontendError::from(e.to_string()))
            })
            .transpose()?;
        has_single_recipient = rcpt_can.is_some();

        let (pkg, rcpt) = reg
            .prepare_sync_package(rcpt_can.as_deref(), Some(2_000_000), true)
            .map_err(|e| FrontendError::from(e.to_string()))?;

        let conf = if let Some(rcpt) = rcpt_can.as_deref() {
            ContainerConfig::TargetDid(rcpt.to_string(), PrivacyMode::TrialDecryption)
        } else if let Some(pwd) = container_password.filter(|s| !s.is_empty()) {
            ContainerConfig::Password(pwd)
        } else {
            return Err(FrontendError::from(
                "Either recipientDid or containerPassword is required".to_string(),
            ));
        };

        (pkg, rcpt, conf)
    };

    let envelope = WotFileEnvelope {
        format_version: 1,
        sender_did,
        sender_profile,
        sync_package,
    };

    let envelope_bytes = serde_json::to_vec(&envelope)
        .map_err(|e| FrontendError::from(format!("Failed to serialize envelope: {}", e)))?;

    let container_bytes = service
        .seal_trust_container(&envelope_bytes, config, pw)
        .map_err(FrontendError::from)?;

    if !share_all && has_single_recipient {
        let now_secs = Utc::now().timestamp().max(0) as u64;
        reg.commit_sync_receipt(receipt, now_secs);
        let _ = state.save_wot_register(&mut service, &reg, pw);
    }

    Ok(container_bytes)
}

pub fn import_wot_package_impl(
    container_bytes: Vec<u8>,
    container_password: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendWotImportResultDto, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    // Case 1: Unencrypted public provider filter JSON
    if let Some(raw_json) = looks_like_provider_filter(&container_bytes) {
        return import_provider_filter_to_dto(state, &mut service, raw_json, pw);
    }

    // Case 2: SecureContainer
    let plaintext = service
        .open_trust_container(&container_bytes, container_password.as_deref(), |bytes| {
            if let Some(raw_json) = looks_like_provider_filter(bytes) {
                let now_secs = Utc::now().timestamp().max(0) as u64;
                let parsed = humoco_wot::parse_and_verify_provider_filter(raw_json, now_secs)
                    .map_err(|e| {
                        human_money_core::Error::Validation(
                            human_money_core::error::ValidationError::InvalidTransaction(
                                e.to_string(),
                            ),
                        )
                    })?;
                Ok(parsed.payload.iss)
            } else {
                let env: WotFileEnvelope = serde_json::from_slice(bytes)
                    .map_err(|e| human_money_core::Error::Json(e.to_string()))?;
                if env.format_version != 1 {
                    return Err(human_money_core::Error::Validation(
                        human_money_core::error::ValidationError::InvalidTransaction(format!(
                            "Unsupported WoT file envelope version: {}",
                            env.format_version
                        )),
                    ));
                }
                crate::state::canonicalize_did(&env.sender_did).map_err(|e| {
                    human_money_core::Error::Validation(
                        human_money_core::error::ValidationError::InvalidTransaction(format!(
                            "Invalid senderDid in WoT envelope: {}",
                            e
                        )),
                    )
                })?;
                Ok(env.sender_did)
            }
        })
        .map_err(FrontendError::from)?;

    if let Some(raw_json) = looks_like_provider_filter(&plaintext) {
        return import_provider_filter_to_dto(state, &mut service, raw_json, pw);
    }

    let envelope: WotFileEnvelope = serde_json::from_slice(&plaintext)
        .map_err(|e| FrontendError::from(format!("Failed to parse envelope: {}", e)))?;
    if envelope.format_version != 1 {
        return Err(FrontendError::from(format!(
            "Unsupported WoT file envelope version: {}",
            envelope.format_version
        )));
    }

    let mut reg = state
        .get_or_load_wot_register(&mut service, pw)
        .map_err(FrontendError::from)?;

    let can_sender = crate::state::canonicalize_did(&envelope.sender_did)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let now_secs = Utc::now().timestamp().max(0) as u64;
    let stats = reg
        .import_sync_package(&envelope.sync_package, Some(can_sender.as_str()), now_secs)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    if reg.own_list_dirty {
        state
            .ensure_wot_list_signed(&mut service, &mut reg)
            .map_err(FrontendError::from)?;
    }

    state
        .save_wot_register(&mut service, &reg, pw)
        .map_err(FrontendError::from)?;

    let is_known_contact = is_known_contact_for_register(
        state,
        &mut service,
        pw,
        &envelope.sender_did,
        Some(&can_sender),
    );

    let existing_trust_level = reg.own_contacts.get(&can_sender).map(|e| e.level);

    Ok(FrontendWotImportResultDto {
        sender_did: envelope.sender_did,
        sender_profile: envelope.sender_profile,
        imported: stats.imported,
        updated: stats.updated,
        merged: stats.merged,
        skipped_unchanged: stats.skipped_unchanged,
        rejected_spam: stats.rejected_spam,
        is_known_contact,
        existing_trust_level,
        is_provider_filter: false,
        provider_name: None,
        provider_member_count: None,
    })
}

pub fn get_wot_providers_impl(
    state: &AppState,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let reg = state
        .get_or_load_wot_register(&mut service, None)
        .map_err(FrontendError::from)?;
    let now_secs = Utc::now().timestamp().max(0) as u64;
    Ok(providers_from_register(&reg, now_secs))
}

pub fn set_wot_provider_trust_impl(
    issuer_did: String,
    level: u8,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    let can_issuer = crate::state::canonicalize_did(&issuer_did)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut reg = state
        .get_or_load_wot_register(&mut service, pw)
        .map_err(FrontendError::from)?;

    reg.set_provider_trust(&can_issuer, level)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    state
        .save_wot_register(&mut service, &reg, pw)
        .map_err(FrontendError::from)?;

    let now_secs = Utc::now().timestamp().max(0) as u64;
    Ok(providers_from_register(&reg, now_secs))
}

pub fn remove_wot_provider_impl(
    issuer_did: String,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    let can_issuer = crate::state::canonicalize_did(&issuer_did)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut reg = state
        .get_or_load_wot_register(&mut service, pw)
        .map_err(FrontendError::from)?;

    let removed = reg
        .remove_provider_filter(&can_issuer)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    if !removed {
        return Err(FrontendError::from(format!(
            "Provider filter {} not found",
            issuer_did
        )));
    }

    state
        .save_wot_register(&mut service, &reg, pw)
        .map_err(FrontendError::from)?;

    let now_secs = Utc::now().timestamp().max(0) as u64;
    Ok(providers_from_register(&reg, now_secs))
}

pub async fn refresh_wot_provider_url_impl(
    issuer_did: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendWotProviderDto, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let can_issuer = crate::state::canonicalize_did(&issuer_did)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let source_url = state.with_session_required(
        &session,
        "Wallet session changed before loading WoT provider filter",
        |service| {
            let reg = state
                .get_or_load_wot_register(service, pw)
                .map_err(FrontendError::from)?;
            let prov = reg.provider_filters.get(&can_issuer).ok_or_else(|| {
                FrontendError::from(format!("Provider filter {} not found", can_issuer))
            })?;
            let url = prov.payload.source_url.clone().ok_or_else(|| {
                FrontendError::from("Provider filter has no sourceUrl configured".to_string())
            })?;
            Ok(url)
        },
    )?;

    let client = crate::sync_client::create_sync_http_client();
    let raw_json = crate::sync_client::fetch_text_bounded(
        &client,
        &source_url,
        humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN,
        "provider filter",
    )
    .await
    .map_err(|e| {
        if let crate::sync_client::SyncClientError::LocalValidation(msg) = e {
            FrontendError::from(format!(
                "Security violation: provider sourceUrl failed SSRF/HTTPS validation: {}",
                msg
            ))
        } else {
            FrontendError::from(e)
        }
    })?;

    let now_secs = Utc::now().timestamp().max(0) as u64;
    let parsed = humoco_wot::parse_and_verify_provider_filter(&raw_json, now_secs)
        .map_err(|e| FrontendError::from(format!("Invalid provider filter fetched: {}", e)))?;

    if parsed.payload.iss != can_issuer {
        return Err(FrontendError::from(format!(
            "Anti-hijack violation: fetched provider filter issuer ({}) does not match expected ({})",
            parsed.payload.iss, can_issuer
        )));
    }

    state.with_session_required(
        &session,
        "Wallet session changed before saving WoT provider filter",
        |service| {
            let mut reg = state
                .get_or_load_wot_register(service, pw)
                .map_err(FrontendError::from)?;

            if !reg.provider_filters.contains_key(&can_issuer) {
                return Err(FrontendError::from(format!(
                    "Provider filter {} was removed during refresh",
                    can_issuer
                )));
            }

            let iss = reg
                .import_provider_filter(&raw_json, now_secs)
                .map_err(|e| FrontendError::from(e.to_string()))?;

            state
                .save_wot_register(service, &reg, pw)
                .map_err(FrontendError::from)?;

            let prov = reg.provider_filters.get(&iss).ok_or_else(|| {
                FrontendError::from("Updated provider filter not found in register".to_string())
            })?;

            Ok(provider_filter_to_dto(
                prov,
                &reg,
                now_secs,
                reg.policy.freshness_grace_secs,
            ))
        },
    )
}

pub fn evaluate_voucher_trust_preview_impl(
    creator_id: String,
    guarantor_ids: Vec<String>,
    state: &AppState,
) -> Result<FrontendVoucherTrustDto, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let _ = state.get_or_load_wot_register(&mut service, None);
    let trust = service
        .with_wallet(|w| state.evaluate_voucher_trust_preview(w, &creator_id, &guarantor_ids))
        .map_err(FrontendError::from)?;
    Ok(trust)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FrontendContact, FrontendUserProfile};
    use human_money_core::app_service::AppService;
    use std::path::{Path, PathBuf};

    const ALICE_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const BOB_PHRASE: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
    const DAVE_PHRASE: &str =
        "vessel ladder alter error federal sibling chat ability sun glass valve picture";

    struct TempDirGuard(PathBuf);

    impl TempDirGuard {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("humoco_wot_test_{}_{}", name, uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn setup_test_app(dir: &Path, profile_name: &str, phrase: &str, password: &str) -> AppService {
        let mut service = AppService::new(dir).unwrap();
        service
            .create_profile(
                profile_name,
                phrase,
                None,
                Some("pc"),
                password,
                human_money_core::MnemonicLanguage::English,
                "device-wot-test".into(),
                true,
            )
            .unwrap();
        service
    }

    fn create_test_state(service: AppService) -> AppState {
        AppState::new(service)
    }

    fn make_contact(did: &str, name: &str, trust_level: u8, desired_hops: u8) -> FrontendContact {
        FrontendContact {
            did: did.to_string(),
            profile: FrontendUserProfile {
                first_name: Some(name.to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(trust_level),
            desired_hops: Some(desired_hops),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        }
    }

    fn sign_provider_filter_with(
        svc: &AppService,
        name: &str,
        version: u64,
        iat: u64,
        source_url: Option<String>,
        members: &[(String, u8)],
    ) -> String {
        svc.with_wallet_and_identity(|_w, id| {
            let signer = crate::state::DidWotSigner::from_identity(id);
            humoco_wot::create_and_sign_provider_filter(&signer, name, version, iat, source_url, members)
                .unwrap()
        })
        .unwrap()
    }

    fn spawn_single_provider_server(
        listener: tokio::net::TcpListener,
        body: String,
        hit_tx: tokio::sync::oneshot::Sender<()>,
        release_rx: tokio::sync::oneshot::Receiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("mock provider server must accept connection");
            let mut req_buf = [0u8; 1024];
            let n = stream
                .read(&mut req_buf)
                .await
                .expect("mock provider must read request");
            assert!(n > 0, "expected non-empty HTTP request");
            hit_tx
                .send(())
                .expect("test harness must await provider hit notification");
            release_rx
                .await
                .expect("test harness must release provider response");
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            let _ = stream.shutdown().await;
        })
    }

    fn spawn_chunked_oversized_server(
        listener: tokio::net::TcpListener,
    ) -> tokio::task::JoinHandle<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("mock oversized server must accept connection");
            let mut req_buf = [0u8; 1024];
            let n = stream
                .read(&mut req_buf)
                .await
                .expect("mock oversized server must read request");
            assert!(n > 0, "expected non-empty HTTP request");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n",
                )
                .await
                .expect("header write must succeed");
            let chunk = vec![b'a'; 64 * 1024];
            let hdr = format!("{:x}\r\n", chunk.len());
            let chunks_needed = (humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN / chunk.len()) + 2;
            let mut aborted_early = false;
            for _ in 0..chunks_needed {
                if stream.write_all(hdr.as_bytes()).await.is_err()
                    || stream.write_all(&chunk).await.is_err()
                    || stream.write_all(b"\r\n").await.is_err()
                {
                    aborted_early = true;
                    break;
                }
            }
            if !aborted_early {
                let _ = stream.write_all(b"0\r\n\r\n").await;
            }
        })
    }

    #[test]
    fn test_contact_trust_sync_canonical_dedup_and_delete() {
        let base_dir = TempDirGuard::new("trust_sync");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(
            &base_dir.path().join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        );
        let bob_service =
            setup_test_app(&base_dir.path().join("bob"), "Bob", BOB_PHRASE, password);

        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let state = create_test_state(alice_service);

        let contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                last_name: Some("Tester".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };

        {
            let mut s = state.service.lock().unwrap();
            state
                .save_contact(&mut s, contact.clone(), Some(password))
                .unwrap();
        }

        // Verify WoT register reflects level 2 and default desired_hops 2
        {
            let mut s = state.service.lock().unwrap();
            let reg = state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            let can_did = crate::state::canonicalize_did(&bob_did).unwrap();
            let entry = reg
                .own_contacts
                .get(&can_did)
                .expect("Bob should be in own_contacts");
            assert_eq!(entry.level, 2);
            assert_eq!(entry.desired_hops, 2);
        }

        // Verify summary
        let summary = get_wot_summary_impl(&state).unwrap();
        assert_eq!(summary.rated_contacts_count, 1);
        assert_eq!(summary.verified_humans_count, 0);

        // Update Bob's trust level to 0 (verified human only)
        let mut updated_contact = contact;
        updated_contact.trust_level = Some(0);
        {
            let mut s = state.service.lock().unwrap();
            state
                .save_contact(&mut s, updated_contact, Some(password))
                .unwrap();
        }
        {
            let mut s = state.service.lock().unwrap();
            let reg = state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            let can_did = crate::state::canonicalize_did(&bob_did).unwrap();
            let entry = reg.own_contacts.get(&can_did).unwrap();
            assert_eq!(entry.level, 0);
            assert_eq!(entry.desired_hops, 0);
        }

        let summary2 = get_wot_summary_impl(&state).unwrap();
        assert_eq!(summary2.rated_contacts_count, 0);
        assert_eq!(summary2.verified_humans_count, 1);

        // Delete contact -> verify removed from register
        {
            let mut s = state.service.lock().unwrap();
            state
                .delete_contact(&mut s, &bob_did, Some(password))
                .unwrap();
        }
        {
            let mut s = state.service.lock().unwrap();
            let reg = state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            let can_did = crate::state::canonicalize_did(&bob_did).unwrap();
            assert!(!reg.own_contacts.contains_key(&can_did));
        }
    }

    #[test]
    fn test_container_export_import_roundtrip_and_invalid_password() {
        let base_dir = TempDirGuard::new("container_roundtrip");
        let alice_dir = base_dir.path().join("alice");
        let bob_dir = base_dir.path().join("bob");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&alice_dir, "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&bob_dir, "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // Alice adds Bob as contact level 2
        let bob_contact = make_contact(&bob_did, "Bob", 2, 3);
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(&mut s, bob_contact, Some(password))
                .unwrap();
        }

        // 1. Password container export (must NOT commit sync receipt to sent_log)
        let container_pwd = "SecretContainerPass123!";
        let container_bytes = export_wot_package_impl(
            None,
            Some(container_pwd.to_string()),
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        {
            let mut s = alice_state.service.lock().unwrap();
            let reg = alice_state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            assert!(
                reg.sent_log.is_empty(),
                "password export must not commit single-recipient sync receipt"
            );
        }

        // Invariant: wrong container password must fail AEAD decryption specifically, not envelope/session checks.
        let wrong_pwd_err = import_wot_package_impl(
            container_bytes.clone(),
            Some("WrongPassword".to_string()),
            Some(password.to_string()),
            &bob_state,
        )
        .expect_err("wrong container password must fail decryption");
        assert!(
            wrong_pwd_err.message.contains("AEAD decryption failed"),
            "unexpected error for wrong container password: {}",
            wrong_pwd_err.message
        );

        // Positive baseline: Bob imports with correct container password -> succeeds
        let import_res = import_wot_package_impl(
            container_bytes,
            Some(container_pwd.to_string()),
            Some(password.to_string()),
            &bob_state,
        )
        .unwrap();

        assert_eq!(import_res.sender_did, alice_did);
        assert_eq!(import_res.imported, 1);

        // 2. TargetDid container export (targeted to Bob's DID -> MUST commit sync receipt)
        let target_container_bytes = export_wot_package_impl(
            Some(bob_did.clone()),
            None,
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        {
            let mut s = alice_state.service.lock().unwrap();
            let reg = alice_state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            let can_bob = crate::state::canonicalize_did(&bob_did).unwrap();
            assert!(
                reg.sent_log.contains_key(&can_bob),
                "single-recipient TargetDid export must commit sync receipt for recipient"
            );
        }

        // Bob imports without container password -> decrypted via Bob's DID key
        let target_import_res = import_wot_package_impl(
            target_container_bytes,
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .unwrap();

        assert_eq!(target_import_res.sender_did, alice_did);
    }

    async fn send_and_receive_offline_transfer(
        alice_state: &AppState,
        bob_state: &AppState,
        bob_did: &str,
        bob_identity: &human_money_core::models::profile::UserIdentity,
        standards: &std::collections::HashMap<String, String>,
        password: &str,
    ) -> (Option<String>, Option<usize>) {
        use crate::models::{FrontendSourceTransfer, ReceiveCommitMode};
        use crate::services::transfer_service::{create_transfer_bundle_impl, receive_bundle_impl};
        use human_money_core::services::bundle_processor::open_and_verify_bundle;

        let v_id = alice_state
            .service
            .lock()
            .unwrap()
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();
        let res = create_transfer_bundle_impl(
            None,
            bob_did.to_string(),
            vec![FrontendSourceTransfer {
                local_instance_id: v_id,
                amount_to_send: "10".into(),
            }],
            None,
            None,
            standards.clone(),
            None,
            Some(false),
            None,
            Some(password.to_string()),
            alice_state,
        )
        .await
        .unwrap();
        let opened = open_and_verify_bundle(bob_identity, &res.bundle_data).unwrap();
        let recv = receive_bundle_impl(
            None,
            res.bundle_data,
            standards.clone(),
            Some(password.to_string()),
            false,
            Some(ReceiveCommitMode::ForceTrustOffline),
            None,
            bob_state,
        )
        .await
        .unwrap();
        (opened.wot_sync_json, recv.wot_imported_lists)
    }

    #[tokio::test]
    async fn test_piggybacking_respects_contact_trust_level_end_to_end() {
        use human_money_core::models::profile::PublicProfile;
        use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
        use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
        use std::collections::HashMap;

        // Invariant: Sending a voucher via create_transfer_bundle_impl to a level-0 contact MUST NOT
        // piggyback Alice's WoT sync package (even when Alice has other rated contacts), whereas
        // upgrading the recipient to level 1 MUST piggyback and import Alice's WoT list on receive.
        let base_dir = TempDirGuard::new("piggyback_e2e");
        let password = "TestPassword123!";
        let mut alice_svc = setup_test_app(
            &base_dir.path().join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        );
        let mut bob_svc =
            setup_test_app(&base_dir.path().join("bob"), "Bob", BOB_PHRASE, password);
        let charlie_svc = setup_test_app(
            &base_dir.path().join("charlie"),
            "Charlie",
            crate::test_support::CHARLIE_PHRASE,
            password,
        );

        alice_svc.unlock_session(password, 3600).unwrap();
        bob_svc.unlock_session(password, 3600).unwrap();

        let alice_did = alice_svc
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let can_alice = crate::state::canonicalize_did(&alice_did).unwrap();
        let bob_did = bob_svc
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_identity = bob_svc
            .with_wallet_and_identity(|_, id| id.clone())
            .unwrap();
        let charlie_did = charlie_svc
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let std_toml = human_money_core::test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

        alice_svc
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_did),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "50".into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();

        let alice_state = create_test_state(alice_svc);
        let bob_state = create_test_state(bob_svc);

        // Give Alice a rated contact (Charlie level 1) so Alice's own_signed_list is non-empty,
        // and add Bob at level 0 (verified human only -> must NOT trigger piggybacking).
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(&mut s, make_contact(&charlie_did, "Charlie", 1, 2), Some(password))
                .unwrap();
            alice_state
                .save_contact(&mut s, make_contact(&bob_did, "Bob", 0, 0), Some(password))
                .unwrap();
        }

        let (wot_lvl0, imported_lvl0) = send_and_receive_offline_transfer(
            &alice_state,
            &bob_state,
            &bob_did,
            &bob_identity,
            &standards,
            password,
        )
        .await;
        assert!(
            wot_lvl0.is_none(),
            "level-0 contact must NOT receive piggybacked WoT sync package"
        );
        assert_eq!(imported_lvl0, None);

        // Upgrade Bob to level 1 -> positive baseline MUST piggyback and import Alice's list
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(&mut s, make_contact(&bob_did, "Bob", 1, 2), Some(password))
                .unwrap();
        }

        let (wot_lvl1, imported_lvl1) = send_and_receive_offline_transfer(
            &alice_state,
            &bob_state,
            &bob_did,
            &bob_identity,
            &standards,
            password,
        )
        .await;
        assert!(
            wot_lvl1.is_some(),
            "level-1 contact must receive piggybacked WoT sync package"
        );
        assert_eq!(imported_lvl1, Some(1));

        let mut s = bob_state.service.lock().unwrap();
        let bob_reg = bob_state
            .get_or_load_wot_register(&mut s, Some(password))
            .unwrap();
        assert!(
            bob_reg.raw_list_store.contains_key(&can_alice),
            "Bob's WoT register must store Alice's piggybacked signed list"
        );
    }

    #[test]
    fn test_set_wot_max_hops_updates_contacts_and_rejects_out_of_range() {
        let base_dir = TempDirGuard::new("max_hops");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(
            &base_dir.path().join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        );
        let bob_service =
            setup_test_app(&base_dir.path().join("bob"), "Bob", BOB_PHRASE, password);
        let dave_service = setup_test_app(
            &base_dir.path().join("dave"),
            "Dave",
            crate::test_support::CHARLIE_PHRASE,
            password,
        );

        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let dave_did = dave_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let alice_state = create_test_state(alice_service);

        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(&mut s, make_contact(&bob_did, "Bob", 2, 3), Some(password))
                .unwrap();
            alice_state
                .save_contact(&mut s, make_contact(&dave_did, "Dave", 0, 0), Some(password))
                .unwrap();
        }

        // Verify initial state: Bob has desired_hops 2 (default own_list_max_hops), Dave has 0
        {
            let mut s = alice_state.service.lock().unwrap();
            let reg = alice_state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            let bob_can = crate::state::canonicalize_did(&bob_did).unwrap();
            let dave_can = crate::state::canonicalize_did(&dave_did).unwrap();
            assert_eq!(reg.own_contacts.get(&bob_can).unwrap().desired_hops, 2);
            assert_eq!(reg.own_contacts.get(&dave_can).unwrap().desired_hops, 0);
        }

        // Invariant: max_hops outside 1..=3 must fail with range error; valid boundaries 1 and 3 succeed.
        let err0 = set_wot_max_hops_impl(0, Some(password.to_string()), &alice_state)
            .expect_err("max_hops=0 must be rejected");
        assert!(
            err0.message.contains("maxHops must be between 1 and 3"),
            "unexpected error for max_hops=0: {}",
            err0.message
        );

        let err4 = set_wot_max_hops_impl(4, Some(password.to_string()), &alice_state)
            .expect_err("max_hops=4 must be rejected");
        assert!(
            err4.message.contains("maxHops must be between 1 and 3"),
            "unexpected error for max_hops=4: {}",
            err4.message
        );

        // Lower valid boundary: set to 1
        let summary1 = set_wot_max_hops_impl(1, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(summary1.own_list_max_hops, 1);

        // Upper valid boundary: set to 3
        let summary = set_wot_max_hops_impl(3, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(summary.own_list_max_hops, 3);

        // Verify register: Bob updated to 3, Dave stayed 0
        {
            let mut s = alice_state.service.lock().unwrap();
            let reg = alice_state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            let bob_can = crate::state::canonicalize_did(&bob_did).unwrap();
            let dave_can = crate::state::canonicalize_did(&dave_did).unwrap();
            assert_eq!(reg.policy.own_list_max_hops, 3);
            assert_eq!(reg.own_contacts.get(&bob_can).unwrap().desired_hops, 3);
            assert_eq!(reg.own_contacts.get(&dave_can).unwrap().desired_hops, 0);
        }

        // Verify cached contacts reflect updated desired_hops
        let cached = alice_state.get_cached_contacts().unwrap();
        let bob_cached = cached.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob_cached.desired_hops, Some(3));
        let dave_cached = cached.iter().find(|c| c.did == dave_did).unwrap();
        assert_eq!(dave_cached.desired_hops, Some(0));
    }

    #[test]
    fn test_multi_recipient_share_with_all_contacts_roundtrip() {
        let base_dir = TempDirGuard::new("multi_recipient");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(
            &base_dir.path().join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        );
        let bob_service =
            setup_test_app(&base_dir.path().join("bob"), "Bob", BOB_PHRASE, password);
        let charlie_service = setup_test_app(
            &base_dir.path().join("charlie"),
            "Charlie",
            crate::test_support::CHARLIE_PHRASE,
            password,
        );
        let dave_service = setup_test_app(
            &base_dir.path().join("dave"),
            "Dave",
            DAVE_PHRASE,
            password,
        );

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);
        let charlie_state = create_test_state(charlie_service);
        let dave_state = create_test_state(dave_service);

        let alice_did = alice_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_did = bob_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let charlie_did = charlie_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let dave_did = dave_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        // Add level-0 contact Dave: level-0 contacts must NOT count as rated recipients.
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(&mut s, make_contact(&dave_did, "Dave", 0, 0), Some(password))
                .unwrap();
        }

        // Invariant: share_with_all_contacts with zero rated (level 1-3) contacts must fail closed.
        let empty_err = export_wot_package_impl(
            None,
            None,
            Some(true),
            Some(password.to_string()),
            &alice_state,
        )
        .expect_err("export with only level-0 contacts must fail");
        assert!(
            empty_err
                .message
                .contains("No rated contacts (level 1-3) available to encrypt for"),
            "unexpected error: {}",
            empty_err.message
        );

        // Alice adds Bob (level 2) and Charlie (level 1)
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(&mut s, make_contact(&bob_did, "Bob", 2, 3), Some(password))
                .unwrap();
            alice_state
                .save_contact(&mut s, make_contact(&charlie_did, "Charlie", 1, 3), Some(password))
                .unwrap();
        }

        let container_bytes = export_wot_package_impl(
            None,
            None,
            Some(true),
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        // Dave (level 0) must NOT be able to decrypt the multi-recipient container.
        let dave_err = import_wot_package_impl(
            container_bytes.clone(),
            None,
            Some(password.to_string()),
            &dave_state,
        )
        .expect_err("level-0 contact must not be encrypted for");
        assert!(
            dave_err
                .message
                .contains("not in the list of recipients"),
            "unexpected error for excluded level-0 recipient: {}",
            dave_err.message
        );

        // Both Bob and Charlie import the exact same container without any password.
        let bob_import = import_wot_package_impl(
            container_bytes.clone(),
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .unwrap();
        assert_eq!(bob_import.sender_did, alice_did);
        assert_eq!(bob_import.imported, 1);
        assert!(!bob_import.is_provider_filter);

        let charlie_import = import_wot_package_impl(
            container_bytes,
            None,
            Some(password.to_string()),
            &charlie_state,
        )
        .unwrap();
        assert_eq!(charlie_import.sender_did, alice_did);
        assert_eq!(charlie_import.imported, 1);
        assert!(!charlie_import.is_provider_filter);
    }

    #[tokio::test]
    async fn test_provider_filter_import_trust_and_voucher_preview() {
        let base_dir = TempDirGuard::new("provider_filter");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(
            &base_dir.path().join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        );
        let charlie_service = setup_test_app(
            &base_dir.path().join("charlie"),
            "Charlie",
            crate::test_support::CHARLIE_PHRASE,
            password,
        );
        let prov_service = setup_test_app(
            &base_dir.path().join("prov"),
            "Minuto Verband",
            crate::test_support::READER1_PHRASE,
            password,
        );

        let alice_state = create_test_state(alice_service);
        let charlie_state = create_test_state(charlie_service);

        let charlie_did = charlie_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let prov_did = prov_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let can_charlie = crate::state::canonicalize_did(&charlie_did).unwrap();
        let now_secs = Utc::now().timestamp() as u64;

        let raw_filter_json = sign_provider_filter_with(
            &prov_service,
            "Minuto Verband e.V.",
            1,
            now_secs,
            Some("https://minuto.org/filter.json".to_string()),
            &[(can_charlie.clone(), 2)],
        );

        // 1. Alice imports the raw JSON provider filter
        let import_res = import_wot_package_impl(
            raw_filter_json.as_bytes().to_vec(),
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        assert!(import_res.is_provider_filter);
        assert_eq!(
            import_res.provider_name.as_deref(),
            Some("Minuto Verband e.V.")
        );
        assert_eq!(import_res.provider_member_count, Some(1));
        let can_prov_did = crate::state::canonicalize_did(&prov_did).unwrap();
        assert_eq!(import_res.sender_did, can_prov_did);

        // 2. Verify summary count includes provider filter
        let summary = get_wot_summary_impl(&alice_state).unwrap();
        assert_eq!(summary.provider_filters_count, 1);

        // 3. Verify get_wot_providers returns the provider (initially trust level 0)
        let providers = get_wot_providers_impl(&alice_state).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].name, "Minuto Verband e.V.");
        assert_eq!(providers[0].issuer_did, can_prov_did);
        assert_eq!(providers[0].trust_level, 0);

        // 4. Alice sets provider trust level to 3
        let updated_provs = set_wot_provider_trust_impl(
            prov_did.clone(),
            3,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert_eq!(updated_provs[0].trust_level, 3);

        // 5. Evaluate voucher trust preview for Charlie
        let voucher_trust =
            evaluate_voucher_trust_preview_impl(charlie_did.clone(), vec![], &alice_state).unwrap();
        assert!(voucher_trust.score > 0.0);
        assert!(voucher_trust.trust_percent > 0);
        assert_eq!(
            voucher_trust.primary_bridge_name.as_deref(),
            Some("Minuto Verband e.V.")
        );

        // 6. Test removing provider filter
        let remaining =
            remove_wot_provider_impl(prov_did.clone(), Some(password.to_string()), &alice_state)
                .unwrap();
        assert_eq!(remaining.len(), 0);

        let summary_after = get_wot_summary_impl(&alice_state).unwrap();
        assert_eq!(summary_after.provider_filters_count, 0);

        // 7. Invariant: non-HTTPS provider sourceUrl must be rejected by SSRF/HTTPS guard.
        let insecure_filter_json = sign_provider_filter_with(
            &prov_service,
            "Minuto Insecure",
            2,
            now_secs,
            Some("ftp://insecure.example.com/filter.json".to_string()),
            &[(can_charlie, 2)],
        );
        import_wot_package_impl(
            insecure_filter_json.as_bytes().to_vec(),
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        let refresh_err =
            refresh_wot_provider_url_impl(prov_did, Some(password.to_string()), &alice_state)
                .await
                .expect_err("non-HTTPS provider sourceUrl must be rejected");
        assert!(
            refresh_err
                .message
                .contains("Security violation: provider sourceUrl failed SSRF/HTTPS validation"),
            "unexpected error: {}",
            refresh_err.message
        );
    }

    #[test]
    fn test_looks_like_provider_filter_enforces_max_len_and_typ() {
        // Invariant: Only UTF-8 JSON with typ == PROVIDER_FILTER_TYP and byte length <= MAX_PROVIDER_FILTER_RAW_LEN is recognized.
        let prefix = format!("{{\"typ\":\"{}\",\"pad\":\"", humoco_wot::PROVIDER_FILTER_TYP);
        let suffix = "\"}";
        let max_pad_len = humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN - prefix.len() - suffix.len();
        let exact_max = format!("{}{}{}", prefix, "a".repeat(max_pad_len), suffix);
        assert_eq!(exact_max.len(), humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN);
        assert_eq!(
            super::looks_like_provider_filter(exact_max.as_bytes()),
            Some(exact_max.as_str())
        );

        let over_max = format!("{}{}{}", prefix, "a".repeat(max_pad_len + 1), suffix);
        assert_eq!(over_max.len(), humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN + 1);
        assert!(super::looks_like_provider_filter(over_max.as_bytes()).is_none());

        let wrong_typ = br#"{"typ":"other-typ","pad":"ok"}"#;
        assert!(super::looks_like_provider_filter(wrong_typ).is_none());
    }

    #[test]
    fn test_wot_file_envelope_rejects_unknown_fields() {
        // Invariant: WotFileEnvelope must accept valid canonical JSON and reject any unknown field via #[serde(deny_unknown_fields)].
        let valid_json = r#"{"formatVersion":1,"senderDid":"did:key:z1","syncPackage":{"format_version":1,"lists":[]}}"#;
        let parsed = serde_json::from_str::<super::WotFileEnvelope>(valid_json)
            .expect("valid WotFileEnvelope baseline must deserialize");
        assert_eq!(parsed.format_version, 1);

        let bogus_json = r#"{"formatVersion":1,"senderDid":"did:key:z1","syncPackage":{"format_version":1,"lists":[]},"bogus":1}"#;
        let err = serde_json::from_str::<super::WotFileEnvelope>(bogus_json)
            .expect_err("unknown field on WotFileEnvelope must be rejected");
        assert!(
            err.to_string().contains("unknown field"),
            "expected unknown field serde error, got: {}",
            err
        );
    }

    #[test]
    fn test_import_wot_package_rejects_unsupported_envelope_version() {
        // Invariant: import_wot_package_impl must accept format_version=1 and reject any unsupported envelope version.
        let dir = TempDirGuard::new("wot_env_version");
        let pw = "pw-env-ver";
        let mut alice_svc =
            crate::test_support::setup_test_app(&dir.path().join("alice"), "Alice", ALICE_PHRASE, pw);
        alice_svc.unlock_session(pw, 3600).unwrap();
        let alice_state = create_test_state(alice_svc);
        let prov_svc = crate::test_support::setup_test_app(
            &dir.path().join("prov"),
            "Provider",
            BOB_PHRASE,
            pw,
        );
        let prov_did = prov_svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let (pkg, alice_did) = {
            let mut s = alice_state.service.lock().unwrap();
            let mut reg = alice_state.get_or_load_wot_register(&mut s, Some(pw)).unwrap();
            alice_state.ensure_wot_list_signed(&mut s, &mut reg).unwrap();
            let (pkg, _) = reg.prepare_sync_package(None, None, false).unwrap();
            let alice_did = s.with_wallet(|w| w.get_user_id().to_string()).unwrap();
            (pkg, alice_did)
        };

        let seal_for_alice = |version: u8| -> Vec<u8> {
            let env = super::WotFileEnvelope {
                format_version: version,
                sender_did: prov_did.clone(),
                sender_profile: None,
                sync_package: pkg.clone(),
            };
            prov_svc
                .seal_trust_container(
                    &serde_json::to_vec(&env).unwrap(),
                    ContainerConfig::TargetDid(alice_did.clone(), PrivacyMode::TrialDecryption),
                    Some(pw),
                )
                .unwrap()
        };

        let ok_res = import_wot_package_impl(
            seal_for_alice(1),
            None,
            Some(pw.to_string()),
            &alice_state,
        )
        .expect("format_version=1 envelope baseline must import");
        assert_eq!(ok_res.sender_did, prov_did);

        let v2_err = import_wot_package_impl(
            seal_for_alice(2),
            None,
            Some(pw.to_string()),
            &alice_state,
        )
        .expect_err("format_version=2 envelope must be rejected");
        assert!(
            v2_err
                .message
                .contains("Unsupported WoT file envelope version: 2"),
            "unexpected error for unsupported envelope version: {}",
            v2_err.message
        );
    }

    #[test]
    fn test_wot_service_rejects_invalid_did_fail_closed() {
        // Invariant: Export and provider-management operations must fail closed with Invalid DID on malformed DIDs/prefixes while valid DIDs succeed.
        let dir = TempDirGuard::new("wot_invalid_did");
        let pw = "pw-did-check";
        let mut alice_svc =
            crate::test_support::setup_test_app(&dir.path().join("alice"), "Alice", ALICE_PHRASE, pw);
        alice_svc.unlock_session(pw, 3600).unwrap();
        let alice_state = create_test_state(alice_svc);
        let prov_svc = crate::test_support::setup_test_app(
            &dir.path().join("prov"),
            "Provider",
            BOB_PHRASE,
            pw,
        );
        let prov_did = prov_svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let charlie_sk = ed25519_dalek::SigningKey::from_bytes(&[33u8; 32]);
        let charlie_did =
            human_money_core::crypto::create_user_id(&charlie_sk.verifying_key(), None).unwrap();
        let raw_filter = sign_provider_filter_with(
            prov_service_or_ref(&prov_svc),
            "Verband",
            1,
            Utc::now().timestamp().max(0) as u64,
            None,
            &[(humoco_wot::canonicalize_did(&charlie_did).unwrap(), 2)],
        );
        import_wot_package_impl(
            raw_filter.as_bytes().to_vec(),
            None,
            Some(pw.to_string()),
            &alice_state,
        )
        .unwrap();

        // Positive baselines with valid DID
        export_wot_package_impl(
            Some(prov_did.clone()),
            None,
            Some(false),
            Some(pw.to_string()),
            &alice_state,
        )
        .expect("valid recipient DID must export");
        set_wot_provider_trust_impl(prov_did.clone(), 2, Some(pw.to_string()), &alice_state)
            .expect("valid issuer DID must update trust");
        remove_wot_provider_impl(prov_did, Some(pw.to_string()), &alice_state)
            .expect("valid issuer DID must remove provider");

        // Negative cases with malformed DID / prefix
        let exp_err = export_wot_package_impl(
            Some("bad!prefix@did:key:z123".to_string()),
            None,
            Some(false),
            Some(pw.to_string()),
            &alice_state,
        )
        .expect_err("invalid recipient prefix must fail closed");
        assert!(
            exp_err.message.contains("Invalid DID:"),
            "unexpected export error: {}",
            exp_err.message
        );

        let trust_err = set_wot_provider_trust_impl(
            "not-a-did".to_string(),
            2,
            Some(pw.to_string()),
            &alice_state,
        )
        .expect_err("invalid issuer DID in set_wot_provider_trust must fail closed");
        assert!(
            trust_err.message.contains("Invalid DID:"),
            "unexpected set_wot_provider_trust error: {}",
            trust_err.message
        );

        let rem_err =
            remove_wot_provider_impl("not-a-did".to_string(), Some(pw.to_string()), &alice_state)
                .expect_err("invalid issuer DID in remove_wot_provider must fail closed");
        assert!(
            rem_err.message.contains("Invalid DID:"),
            "unexpected remove_wot_provider error: {}",
            rem_err.message
        );
    }

    fn prov_service_or_ref(svc: &AppService) -> &AppService {
        svc
    }

    #[test]
    fn test_evaluate_voucher_trust_preview_warms_cold_wot_cache() {
        // Invariant: evaluate_voucher_trust_preview_impl must reload TrustRegister from disk when cold-cached and resolve provider trust only for certified DIDs.
        let dir = TempDirGuard::new("wot_cold_cache");
        let pw = "pw-cold-cache";
        let mut alice_svc =
            crate::test_support::setup_test_app(&dir.path().join("alice"), "Alice", ALICE_PHRASE, pw);
        alice_svc.unlock_session(pw, 3600).unwrap();
        let alice_state = create_test_state(alice_svc);
        let prov_svc = crate::test_support::setup_test_app(
            &dir.path().join("prov"),
            "Provider",
            BOB_PHRASE,
            pw,
        );
        let prov_did = prov_svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let charlie_svc = setup_test_app(
            &dir.path().join("charlie"),
            "Charlie",
            crate::test_support::CHARLIE_PHRASE,
            pw,
        );
        let charlie_did = charlie_svc
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let can_charlie = crate::state::canonicalize_did(&charlie_did).unwrap();

        let raw_filter = sign_provider_filter_with(
            &prov_svc,
            "Verband",
            1,
            Utc::now().timestamp().max(0) as u64,
            None,
            &[(can_charlie, 3)],
        );
        import_wot_package_impl(
            raw_filter.as_bytes().to_vec(),
            None,
            Some(pw.to_string()),
            &alice_state,
        )
        .unwrap();
        set_wot_provider_trust_impl(prov_did, 3, Some(pw.to_string()), &alice_state).unwrap();

        // Drop in-memory WoT cache to force cold reload from encrypted storage
        {
            let svc = alice_state.service.lock().unwrap();
            alice_state.set_cached_wot_for_service(&svc, None);
        }

        let preview =
            evaluate_voucher_trust_preview_impl(charlie_did, vec![], &alice_state).unwrap();
        assert!(preview.score > 0.0);
        assert_eq!(preview.primary_bridge_name.as_deref(), Some("Verband"));

        let dave_sk = ed25519_dalek::SigningKey::from_bytes(&[88u8; 32]);
        let uncertified_did =
            human_money_core::crypto::create_user_id(&dave_sk.verifying_key(), None).unwrap();
        let unknown_preview =
            evaluate_voucher_trust_preview_impl(uncertified_did, vec![], &alice_state).unwrap();
        assert_eq!(unknown_preview.score, 0.0);
        assert!(unknown_preview.primary_bridge_name.is_none());
    }

    #[tokio::test]
    async fn test_refresh_wot_provider_aborts_when_removed_concurrently() {
        // Invariant: Deleting a provider while refresh_wot_provider_url_impl is awaiting HTTP MUST abort saving without resurrecting the deleted provider.
        let dir = TempDirGuard::new("wot_concurrent_remove");
        let pw = "pw-concurrent-remove";
        let mut alice_svc =
            crate::test_support::setup_test_app(&dir.path().join("alice"), "Alice", ALICE_PHRASE, pw);
        alice_svc.unlock_session(pw, 3600).unwrap();
        let alice_state = create_test_state(alice_svc);
        let prov_svc = crate::test_support::setup_test_app(
            &dir.path().join("prov"),
            "Provider",
            BOB_PHRASE,
            pw,
        );
        let prov_did = prov_svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let charlie_sk = ed25519_dalek::SigningKey::from_bytes(&[55u8; 32]);
        let charlie_did =
            human_money_core::crypto::create_user_id(&charlie_sk.verifying_key(), None).unwrap();
        let can_charlie = humoco_wot::canonicalize_did(&charlie_did).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_url = format!("http://127.0.0.1:{}/filter.json", listener.local_addr().unwrap().port());
        let now_secs = Utc::now().timestamp().max(0) as u64;
        let filter_v1 = sign_provider_filter_with(
            &prov_svc,
            "Verband",
            1,
            now_secs,
            Some(source_url.clone()),
            &[(can_charlie.clone(), 3)],
        );
        let filter_v2 = sign_provider_filter_with(
            &prov_svc,
            "Verband",
            2,
            now_secs + 1,
            Some(source_url),
            &[(can_charlie, 3)],
        );

        import_wot_package_impl(
            filter_v1.as_bytes().to_vec(),
            None,
            Some(pw.to_string()),
            &alice_state,
        )
        .unwrap();

        let (hit_tx, hit_rx) = tokio::sync::oneshot::channel::<()>();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let server = spawn_single_provider_server(listener, filter_v2, hit_tx, release_rx);

        let refresh_fut =
            refresh_wot_provider_url_impl(prov_did.clone(), Some(pw.to_string()), &alice_state);
        tokio::pin!(refresh_fut);
        let mut removed_during_wait = false;
        let refresh_res = tokio::select! {
            res = &mut refresh_fut => res,
            hit = hit_rx => {
                hit.expect("mock server must signal HTTP request arrival");
                remove_wot_provider_impl(prov_did, Some(pw.to_string()), &alice_state).unwrap();
                removed_during_wait = true;
                release_tx.send(()).expect("mock server must accept release signal");
                refresh_fut.await
            }
        };
        assert!(
            removed_during_wait,
            "concurrent deletion branch must execute while HTTP refresh is in flight"
        );
        let err = refresh_res
            .expect_err("Concurrent provider deletion must abort refresh without resurrection");
        assert!(
            err.message.contains("was removed during refresh"),
            "unexpected error: {}",
            err.message
        );
        assert!(get_wot_providers_impl(&alice_state).unwrap().is_empty());
        server.await.expect("mock server task must exit cleanly");
    }

    #[tokio::test]
    async fn test_refresh_wot_provider_updates_version_and_rejects_hijacked_issuer() {
        // Invariant: Valid v2 refresh updates stored provider metadata; a response signed by a different issuer must fail Anti-hijack validation without overwriting v2.
        let dir = TempDirGuard::new("wot_refresh_hijack");
        let pw = "pw-refresh-hijack";
        let mut alice_svc =
            crate::test_support::setup_test_app(&dir.path().join("alice"), "Alice", ALICE_PHRASE, pw);
        alice_svc.unlock_session(pw, 3600).unwrap();
        let alice_state = create_test_state(alice_svc);
        let prov_svc = crate::test_support::setup_test_app(
            &dir.path().join("prov"),
            "Provider",
            BOB_PHRASE,
            pw,
        );
        let rogue_svc = crate::test_support::setup_test_app(
            &dir.path().join("rogue"),
            "Rogue",
            DAVE_PHRASE,
            pw,
        );
        let prov_did = prov_svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let charlie_sk = ed25519_dalek::SigningKey::from_bytes(&[66u8; 32]);
        let charlie_did =
            human_money_core::crypto::create_user_id(&charlie_sk.verifying_key(), None).unwrap();
        let can_charlie = humoco_wot::canonicalize_did(&charlie_did).unwrap();

        let listener_ok = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_url = format!("http://127.0.0.1:{}/filter.json", listener_ok.local_addr().unwrap().port());
        let now_secs = Utc::now().timestamp().max(0) as u64;
        let filter_v1 = sign_provider_filter_with(
            &prov_svc,
            "Verband",
            1,
            now_secs,
            Some(source_url.clone()),
            &[(can_charlie.clone(), 2)],
        );
        let filter_v2 = sign_provider_filter_with(
            &prov_svc,
            "Verband",
            2,
            now_secs + 1,
            Some(source_url.clone()),
            &[(can_charlie.clone(), 3)],
        );
        let rogue_filter = sign_provider_filter_with(
            &rogue_svc,
            "Hijacked",
            99,
            now_secs + 2,
            Some(source_url),
            &[(can_charlie, 3)],
        );

        import_wot_package_impl(
            filter_v1.as_bytes().to_vec(),
            None,
            Some(pw.to_string()),
            &alice_state,
        )
        .unwrap();

        // Positive baseline: valid v2 filter updates stored provider to version 2
        let (hit_tx1, hit_rx1) = tokio::sync::oneshot::channel::<()>();
        let (rel_tx1, rel_rx1) = tokio::sync::oneshot::channel::<()>();
        let srv1 = spawn_single_provider_server(listener_ok, filter_v2, hit_tx1, rel_rx1);
        let fut1 = refresh_wot_provider_url_impl(prov_did.clone(), Some(pw.to_string()), &alice_state);
        tokio::pin!(fut1);
        let updated = tokio::select! {
            res = &mut fut1 => res,
            hit = hit_rx1 => {
                hit.unwrap();
                rel_tx1.send(()).unwrap();
                fut1.await
            }
        }
        .expect("valid v2 provider refresh must succeed");
        assert_eq!(updated.version, 2);
        srv1.await.expect("v2 server task must exit cleanly");

        // Negative test: hijacked issuer on same URL is rejected (reuse same port via rogue filter not possible on bound port -> test URL hijack directly by pointing provider source_url to rogue server)
        let listener_rogue = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let rogue_url = format!("http://127.0.0.1:{}/filter.json", listener_rogue.local_addr().unwrap().port());
        let filter_v3_pointing_to_rogue = sign_provider_filter_with(
            &prov_svc,
            "Verband",
            3,
            now_secs + 3,
            Some(rogue_url),
            &[],
        );
        import_wot_package_impl(
            filter_v3_pointing_to_rogue.as_bytes().to_vec(),
            None,
            Some(pw.to_string()),
            &alice_state,
        )
        .unwrap();

        let (hit_tx2, hit_rx2) = tokio::sync::oneshot::channel::<()>();
        let (rel_tx2, rel_rx2) = tokio::sync::oneshot::channel::<()>();
        let srv2 = spawn_single_provider_server(listener_rogue, rogue_filter, hit_tx2, rel_rx2);
        let fut2 = refresh_wot_provider_url_impl(prov_did, Some(pw.to_string()), &alice_state);
        tokio::pin!(fut2);
        let hijack_err = tokio::select! {
            res = &mut fut2 => res,
            hit = hit_rx2 => {
                hit.unwrap();
                rel_tx2.send(()).unwrap();
                fut2.await
            }
        }
        .expect_err("rogue issuer filter must be rejected by Anti-hijack guard");
        assert!(
            hijack_err.message.contains("Anti-hijack violation"),
            "unexpected error: {}",
            hijack_err.message
        );
        assert_eq!(get_wot_providers_impl(&alice_state).unwrap()[0].version, 3);
        srv2.await.expect("rogue server task must exit cleanly");
    }

    #[tokio::test]
    async fn test_refresh_wot_provider_rejects_oversized_response() {
        let dir = TempDirGuard::new("wot_provider_oversized");
        let pw = "pw-oversized";
        let mut alice_svc =
            crate::test_support::setup_test_app(&dir.path().join("alice"), "Alice", ALICE_PHRASE, pw);
        alice_svc.unlock_session(pw, 3600).unwrap();
        let alice_state = create_test_state(alice_svc);

        let prov_svc = crate::test_support::setup_test_app(
            &dir.path().join("prov"),
            "Provider",
            BOB_PHRASE,
            pw,
        );
        let prov_did = prov_svc
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let charlie_sk = ed25519_dalek::SigningKey::from_bytes(&[77u8; 32]);
        let charlie_did =
            human_money_core::crypto::create_user_id(&charlie_sk.verifying_key(), None).unwrap();
        let can_charlie = humoco_wot::canonicalize_did(&charlie_did).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let source_url = format!("http://127.0.0.1:{}/filter.json", port);

        let now_secs = Utc::now().timestamp().max(0) as u64;
        let filter_v1 = sign_provider_filter_with(
            &prov_svc,
            "Verband",
            1,
            now_secs,
            Some(source_url),
            &[(can_charlie, 3)],
        );

        import_wot_package_impl(
            filter_v1.as_bytes().to_vec(),
            None,
            Some(pw.to_string()),
            &alice_state,
        )
        .unwrap();

        let server = spawn_chunked_oversized_server(listener);

        let err = refresh_wot_provider_url_impl(prov_did, Some(pw.to_string()), &alice_state)
            .await
            .expect_err("oversized provider filter response must fail");
        assert_eq!(err.code, "error.sync.responseTooLarge");

        server.await.unwrap();
    }
}
