use chrono::Utc;
use serde::{Deserialize, Serialize};
use human_money_core::models::secure_container::{ContainerConfig, PrivacyMode};
use crate::models::{
    FrontendError, FrontendUserProfile, FrontendVoucherTrustDto, FrontendWotImportResultDto,
    FrontendWotProviderDto, FrontendWotSummaryDto,
};
use crate::state::AppState;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct WotFileEnvelope {
    pub format_version: u8,
    pub sender_did: String,
    pub sender_profile: Option<FrontendUserProfile>,
    pub sync_package: humoco_wot::WotSyncPackage,
}

fn looks_like_provider_filter(bytes: &[u8]) -> Option<&str> {
    let s = std::str::from_utf8(bytes).ok()?.trim();
    let val: serde_json::Value = serde_json::from_str(s).ok()?;
    if val.get("typ").and_then(|v| v.as_str()) == Some(humoco_wot::PROVIDER_FILTER_TYP) {
        Some(s)
    } else {
        None
    }
}

fn import_provider_filter_to_dto(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    raw_json: &str,
    password: Option<&str>,
) -> Result<FrontendWotImportResultDto, FrontendError> {
    let now_secs = Utc::now().timestamp() as u64;
    let mut reg = state
        .get_or_load_wot_register(service, password)
        .map_err(FrontendError::from)?;

    let iss = reg
        .import_provider_filter(raw_json, now_secs)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    state
        .save_wot_register(service, &reg, password)
        .map_err(FrontendError::from)?;

    let stored = reg
        .provider_filters
        .get(&iss)
        .ok_or_else(|| FrontendError::from("Imported provider filter not found in register".to_string()))?;

    let contacts_cache = state.contacts.lock().unwrap();
    let is_known_contact = contacts_cache.as_ref().map(|book| {
        book.contacts.keys().any(|k| {
            k == &iss || crate::state::canonicalize_did(k).ok().as_ref() == Some(&iss)
        })
    }).unwrap_or(false);

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

#[tauri::command]
pub fn get_wot_summary(state: tauri::State<AppState>) -> Result<FrontendWotSummaryDto, FrontendError> {
    get_wot_summary_impl(&state)
}

pub fn get_wot_summary_impl(state: &AppState) -> Result<FrontendWotSummaryDto, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let reg = state.get_or_load_wot_register(&mut service, None).map_err(FrontendError::from)?;

    let rated_contacts_count = reg.own_contacts.values().filter(|e| e.level >= 1).count();
    let verified_humans_count = reg.own_contacts.values().filter(|e| e.level == 0).count();
    let close_confidants_count = reg.own_contacts.values().filter(|e| e.level == 3).count();
    let imported_peer_lists_count = reg.raw_list_store.len();
    let own_list_version = reg.own_signed_list.as_ref().map(|l| l.v);
    let own_list_max_hops = reg.policy.own_list_max_hops;
    let provider_filters_count = reg.provider_filters.len();

    Ok(FrontendWotSummaryDto {
        rated_contacts_count,
        verified_humans_count,
        close_confidants_count,
        imported_peer_lists_count,
        own_list_version,
        own_list_max_hops,
        provider_filters_count,
    })
}

#[tauri::command]
pub fn set_wot_max_hops(
    max_hops: u8,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendWotSummaryDto, FrontendError> {
    set_wot_max_hops_impl(max_hops, password, &state)
}

pub fn set_wot_max_hops_impl(
    max_hops: u8,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendWotSummaryDto, FrontendError> {
    if !(1..=4).contains(&max_hops) {
        return Err(FrontendError::from("maxHops must be between 1 and 4".to_string()));
    }

    let mut service = state.service.lock().unwrap();
    let mut reg = state
        .get_or_load_wot_register(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;

    let now_secs = Utc::now().timestamp() as u64;
    reg.set_own_list_max_hops(max_hops, now_secs)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
    state
        .save_wot_register(&mut service, &reg, password.as_deref())
        .map_err(FrontendError::from)?;

    if let Some(book) = state.contacts.lock().unwrap().as_mut() {
        for c in book.contacts.values_mut() {
            if let Ok(can) = crate::state::canonicalize_did(&c.did) {
                if let Some(entry) = reg.own_contacts.get(&can) {
                    c.desired_hops = Some(entry.desired_hops);
                }
            }
        }
    }

    drop(service);
    get_wot_summary_impl(state)
}

#[tauri::command]
pub fn export_wot_package(
    recipient_did: Option<String>,
    container_password: Option<String>,
    share_with_all_contacts: Option<bool>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    export_wot_package_impl(
        recipient_did,
        container_password,
        share_with_all_contacts,
        password,
        &state,
    )
}

pub fn export_wot_package_impl(
    recipient_did: Option<String>,
    container_password: Option<String>,
    share_with_all_contacts: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<u8>, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let (sender_did, sender_profile) = service
        .with_wallet(|w| (w.get_user_id().to_string(), Some(w.profile.to_public_profile().into())))
        .map_err(FrontendError::from)?;

    let mut reg = state
        .get_or_load_wot_register(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;

    let signed_changed = state
        .ensure_wot_list_signed(&mut service, &mut reg)
        .map_err(FrontendError::from)?;
    if signed_changed {
        state
            .save_wot_register(&mut service, &reg, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    let share_all = share_with_all_contacts.unwrap_or(false);

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
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| crate::state::canonicalize_did(s).unwrap_or_else(|_| s.to_string()));

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
        .seal_trust_container(&envelope_bytes, config)
        .map_err(FrontendError::from)?;

    if !share_all {
        let rcpt_can = recipient_did
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| crate::state::canonicalize_did(s).unwrap_or_else(|_| s.to_string()));
        if rcpt_can.is_some() {
            let now_secs = Utc::now().timestamp() as u64;
            reg.commit_sync_receipt(receipt, now_secs);
            let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
        }
    }

    Ok(container_bytes)
}

#[tauri::command]
pub fn import_wot_package(
    container_bytes: Vec<u8>,
    container_password: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendWotImportResultDto, FrontendError> {
    import_wot_package_impl(container_bytes, container_password, password, &state)
}

pub fn import_wot_package_impl(
    container_bytes: Vec<u8>,
    container_password: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendWotImportResultDto, FrontendError> {
    // Case 1: Unencrypted public provider filter JSON
    if let Some(raw_json) = looks_like_provider_filter(&container_bytes) {
        let mut service = state.service.lock().unwrap();
        return import_provider_filter_to_dto(state, &mut service, raw_json, password.as_deref());
    }

    // Case 2: SecureContainer
    let mut service = state.service.lock().unwrap();
    let plaintext = service
        .open_trust_container(&container_bytes, container_password.as_deref(), |bytes| {
            if let Some(raw_json) = looks_like_provider_filter(bytes) {
                let now_secs = Utc::now().timestamp() as u64;
                let parsed = humoco_wot::parse_and_verify_provider_filter(raw_json, now_secs)
                    .map_err(|e| human_money_core::Error::Generic(e.to_string()))?;
                Ok(parsed.payload.iss)
            } else {
                let env: WotFileEnvelope = serde_json::from_slice(bytes)
                    .map_err(|e| human_money_core::Error::Json(e.to_string()))?;
                Ok(env.sender_did)
            }
        })
        .map_err(FrontendError::from)?;

    if let Some(raw_json) = looks_like_provider_filter(&plaintext) {
        return import_provider_filter_to_dto(state, &mut service, raw_json, password.as_deref());
    }

    let envelope: WotFileEnvelope = serde_json::from_slice(&plaintext)
        .map_err(|e| FrontendError::from(format!("Failed to parse envelope: {}", e)))?;

    let mut reg = state
        .get_or_load_wot_register(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;

    let can_sender = crate::state::canonicalize_did(&envelope.sender_did).ok();
    let now_secs = Utc::now().timestamp() as u64;
    let stats = reg
        .import_sync_package(&envelope.sync_package, can_sender.as_deref(), now_secs)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    if reg.own_list_dirty {
        let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
    }

    state
        .save_wot_register(&mut service, &reg, password.as_deref())
        .map_err(FrontendError::from)?;

    let contacts_cache = state.contacts.lock().unwrap();
    let is_known_contact = contacts_cache.as_ref().map(|book| {
        book.contacts.keys().any(|k| {
            k == &envelope.sender_did
                || (can_sender.is_some() && crate::state::canonicalize_did(k).ok() == can_sender)
        })
    }).unwrap_or(false);

    let existing_trust_level = can_sender.and_then(|c| reg.own_contacts.get(&c).map(|e| e.level));

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

#[tauri::command]
pub fn get_wot_providers(state: tauri::State<AppState>) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    get_wot_providers_impl(&state)
}

pub fn get_wot_providers_impl(state: &AppState) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let reg = state.get_or_load_wot_register(&mut service, None).map_err(FrontendError::from)?;
    let now_secs = Utc::now().timestamp() as u64;

    let mut list: Vec<FrontendWotProviderDto> = reg
        .provider_filters
        .values()
        .map(|p| {
            let is_stale = now_secs.saturating_sub(p.payload.iat) > reg.policy.freshness_grace_secs;
            let direct_trust = reg.own_contacts.get(&p.payload.iss).map(|e| e.level).unwrap_or(0);
            let trust_level = p.local_trust_level.max(direct_trust);

            FrontendWotProviderDto {
                issuer_did: p.payload.iss.clone(),
                name: p.payload.name.clone(),
                version: p.payload.v,
                issued_at: p.payload.iat,
                source_url: p.payload.source_url.clone(),
                member_count: p.payload.member_count,
                trust_level,
                imported_at: p.imported_at,
                is_stale,
            }
        })
        .collect();

    list.sort_by(|a, b| {
        a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.issuer_did.cmp(&b.issuer_did))
    });

    Ok(list)
}

#[tauri::command]
pub fn set_wot_provider_trust(
    issuer_did: String,
    level: u8,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    set_wot_provider_trust_impl(issuer_did, level, password, &state)
}

pub fn set_wot_provider_trust_impl(
    issuer_did: String,
    level: u8,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let mut reg = state
        .get_or_load_wot_register(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;

    reg.set_provider_trust(&issuer_did, level)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    state
        .save_wot_register(&mut service, &reg, password.as_deref())
        .map_err(FrontendError::from)?;

    drop(service);
    get_wot_providers_impl(state)
}

#[tauri::command]
pub fn remove_wot_provider(
    issuer_did: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    remove_wot_provider_impl(issuer_did, password, &state)
}

pub fn remove_wot_provider_impl(
    issuer_did: String,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendWotProviderDto>, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let mut reg = state
        .get_or_load_wot_register(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;

    let removed = reg.remove_provider_filter(&issuer_did)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    if !removed {
        return Err(FrontendError::from(format!("Provider filter {} not found", issuer_did)));
    }

    state
        .save_wot_register(&mut service, &reg, password.as_deref())
        .map_err(FrontendError::from)?;

    drop(service);
    get_wot_providers_impl(state)
}

#[tauri::command]
pub async fn refresh_wot_provider_url(
    issuer_did: String,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendWotProviderDto, FrontendError> {
    refresh_wot_provider_url_impl(issuer_did, password, &state).await
}

pub async fn refresh_wot_provider_url_impl(
    issuer_did: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendWotProviderDto, FrontendError> {
    let can_issuer = crate::state::canonicalize_did(&issuer_did)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let (source_url, grace_secs) = {
        let mut service = state.service.lock().unwrap();
        let reg = state
            .get_or_load_wot_register(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        let prov = reg
            .provider_filters
            .get(&can_issuer)
            .ok_or_else(|| FrontendError::from(format!("Provider filter {} not found", can_issuer)))?;
        let url = prov
            .payload
            .source_url
            .clone()
            .ok_or_else(|| FrontendError::from("Provider filter has no sourceUrl configured".to_string()))?;
        (url, reg.policy.freshness_grace_secs)
    };

    let is_valid_scheme = source_url.starts_with("https://")
        || source_url.starts_with("http://127.0.0.1")
        || source_url.starts_with("http://localhost");
    if !is_valid_scheme {
        return Err(FrontendError::from(
            "Security violation: provider sourceUrl must use https:// (or localhost for dev/testing)".to_string(),
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| FrontendError::from(format!("Failed to build HTTP client: {}", e)))?;

    let response = client
        .get(&source_url)
        .send()
        .await
        .map_err(|e| FrontendError::from(format!("Failed to fetch provider filter: {}", e)))?;

    if !response.status().is_success() {
        return Err(FrontendError::from(format!(
            "HTTP request failed with status: {}",
            response.status()
        )));
    }

    if let Some(content_len) = response.content_length() {
        if content_len > humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN as u64 {
            return Err(FrontendError::from(format!(
                "Provider filter response exceeds {} byte limit (Content-Length: {})",
                humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN, content_len
            )));
        }
    }

    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| FrontendError::from(format!("Failed to read response body: {}", e)))?;

    if body_bytes.len() > humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN {
        return Err(FrontendError::from(format!(
            "Provider filter response exceeds {} byte limit",
            humoco_wot::MAX_PROVIDER_FILTER_RAW_LEN
        )));
    }

    let raw_json = std::str::from_utf8(&body_bytes)
        .map_err(|e| FrontendError::from(format!("Provider filter is not valid UTF-8: {}", e)))?;

    let now_secs = Utc::now().timestamp() as u64;
    let parsed = humoco_wot::parse_and_verify_provider_filter(raw_json, now_secs)
        .map_err(|e| FrontendError::from(format!("Invalid provider filter fetched: {}", e)))?;

    if parsed.payload.iss != can_issuer {
        return Err(FrontendError::from(format!(
            "Anti-hijack violation: fetched provider filter issuer ({}) does not match expected ({})",
            parsed.payload.iss, can_issuer
        )));
    }

    let mut service = state.service.lock().unwrap();
    let mut reg = state
        .get_or_load_wot_register(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;

    let iss = reg
        .import_provider_filter(raw_json, now_secs)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    state
        .save_wot_register(&mut service, &reg, password.as_deref())
        .map_err(FrontendError::from)?;

    let prov = reg
        .provider_filters
        .get(&iss)
        .ok_or_else(|| FrontendError::from("Updated provider filter not found in register".to_string()))?;

    let is_stale = now_secs.saturating_sub(prov.payload.iat) > grace_secs;
    let direct_trust = reg.own_contacts.get(&prov.payload.iss).map(|e| e.level).unwrap_or(0);
    let trust_level = prov.local_trust_level.max(direct_trust);

    Ok(FrontendWotProviderDto {
        issuer_did: prov.payload.iss.clone(),
        name: prov.payload.name.clone(),
        version: prov.payload.v,
        issued_at: prov.payload.iat,
        source_url: prov.payload.source_url.clone(),
        member_count: prov.payload.member_count,
        trust_level,
        imported_at: prov.imported_at,
        is_stale,
    })
}

#[tauri::command]
pub fn evaluate_voucher_trust_preview(
    creator_id: String,
    guarantor_ids: Vec<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendVoucherTrustDto, FrontendError> {
    evaluate_voucher_trust_preview_impl(creator_id, guarantor_ids, &state)
}

pub fn evaluate_voucher_trust_preview_impl(
    creator_id: String,
    guarantor_ids: Vec<String>,
    state: &AppState,
) -> Result<FrontendVoucherTrustDto, FrontendError> {
    let service = state.service.lock().unwrap();
    let trust = service
        .with_wallet(|w| state.evaluate_voucher_trust_preview(w, &creator_id, &guarantor_ids))
        .map_err(FrontendError::from)?;
    Ok(trust)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use human_money_core::app_service::AppService;
    use crate::models::{FrontendContact, FrontendUserProfile};

    const ALICE_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const BOB_PHRASE: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";

    fn temp_test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("humoco_wot_test_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_test_app(dir: &Path, profile_name: &str, phrase: &str, password: &str) -> AppService {
        let mut service = AppService::new(dir).unwrap();
        service.create_profile(
            profile_name,
            phrase,
            None,
            Some("pc"),
            password,
            human_money_core::MnemonicLanguage::English,
            "device-wot-test".into(),
        ).unwrap();
        service
    }

    fn create_test_state(service: AppService) -> AppState {
        AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(None),
            regional_views: Mutex::new(None),
            wot: Mutex::new(None),
        }
    }

    #[test]
    fn test_contact_trust_sync_canonical_dedup_and_delete() {
        let base_dir = temp_test_dir("trust_sync");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
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
            drops: vec![],
        };

        {
            let mut s = state.service.lock().unwrap();
            state.save_contact(&mut s, contact.clone(), Some(password)).unwrap();
        }

        // Verify WoT register reflects level 2 and desired_hops 3
        {
            let mut s = state.service.lock().unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let can_did = crate::state::canonicalize_did(&bob_did).unwrap();
            let entry = reg.own_contacts.get(&can_did).expect("Bob should be in own_contacts");
            assert_eq!(entry.level, 2);
            assert_eq!(entry.desired_hops, 3);
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
            state.save_contact(&mut s, updated_contact, Some(password)).unwrap();
        }
        {
            let mut s = state.service.lock().unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
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
            state.delete_contact(&mut s, &bob_did, Some(password)).unwrap();
        }
        {
            let mut s = state.service.lock().unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let can_did = crate::state::canonicalize_did(&bob_did).unwrap();
            assert!(!reg.own_contacts.contains_key(&can_did));
        }
    }

    #[test]
    fn test_container_export_import_roundtrip_and_invalid_password() {
        let base_dir = temp_test_dir("container_roundtrip");
        let alice_dir = base_dir.join("alice");
        let bob_dir = base_dir.join("bob");
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
        let bob_contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            drops: vec![],
        };
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();
        }

        // 1. Password container export
        let container_pwd = "SecretContainerPass123!";
        let container_bytes = export_wot_package_impl(
            None,
            Some(container_pwd.to_string()),
            None,
            Some(password.to_string()),
            &alice_state,
        ).unwrap();

        // Bob attempts import with wrong container password -> should fail
        let wrong_pwd_res = import_wot_package_impl(
            container_bytes.clone(),
            Some("WrongPassword".to_string()),
            Some(password.to_string()),
            &bob_state,
        );
        assert!(wrong_pwd_res.is_err());

        // Bob imports with correct container password -> should succeed
        let import_res = import_wot_package_impl(
            container_bytes,
            Some(container_pwd.to_string()),
            Some(password.to_string()),
            &bob_state,
        ).unwrap();

        assert_eq!(import_res.sender_did, alice_did);
        assert_eq!(import_res.imported, 1);

        // 2. TargetDid container export (targeted to Bob's DID)
        let target_container_bytes = export_wot_package_impl(
            Some(bob_did.clone()),
            None,
            None,
            Some(password.to_string()),
            &alice_state,
        ).unwrap();

        // Bob imports without container password -> decrypted via Bob's DID key
        let target_import_res = import_wot_package_impl(
            target_container_bytes,
            None,
            Some(password.to_string()),
            &bob_state,
        ).unwrap();

        assert_eq!(target_import_res.sender_did, alice_did);
    }

    #[test]
    fn test_piggybacking_condition() {
        let base_dir = temp_test_dir("piggyback_cond");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let state = create_test_state(alice_service);

        // Bob added with level 0
        let contact_lvl0 = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(0),
            desired_hops: Some(0),
            has_peer_list: false,
            peer_list_updated_at: None,
            drops: vec![],
        };
        {
            let mut s = state.service.lock().unwrap();
            state.save_contact(&mut s, contact_lvl0, Some(password)).unwrap();
        }

        // Test piggybacking evaluation logic: level 0 => must NOT piggyback
        {
            let mut s = state.service.lock().unwrap();
            let can_rcpt = crate::state::canonicalize_did(&bob_did).unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let should_piggyback = reg.own_contacts.get(&can_rcpt).is_some_and(|e| e.level >= 1);
            assert!(!should_piggyback);
        }

        // Update Bob to level 1 => must piggyback
        let contact_lvl1 = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(1),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            drops: vec![],
        };
        {
            let mut s = state.service.lock().unwrap();
            state.save_contact(&mut s, contact_lvl1, Some(password)).unwrap();
        }

        {
            let mut s = state.service.lock().unwrap();
            let can_rcpt = crate::state::canonicalize_did(&bob_did).unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let should_piggyback = reg.own_contacts.get(&can_rcpt).is_some_and(|e| e.level >= 1);
            assert!(should_piggyback);
        }
    }

    #[test]
    fn test_set_wot_max_hops_updates_contacts_and_rejects_out_of_range() {
        let base_dir = temp_test_dir("max_hops");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        const CHARLIE_PHRASE: &str = "legal winner thank year wave sausage worth useful legal winner thank yellow";
        let dave_service = setup_test_app(&base_dir.join("dave"), "Dave", CHARLIE_PHRASE, password);

        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let dave_did = dave_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let alice_state = create_test_state(alice_service);

        // Add Bob with level 2
        let bob_contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            drops: vec![],
        };
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();
        }

        // Add Dave with level 0
        let dave_contact = FrontendContact {
            did: dave_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Dave".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(0),
            desired_hops: Some(0),
            has_peer_list: false,
            peer_list_updated_at: None,
            drops: vec![],
        };
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut s, dave_contact, Some(password)).unwrap();
        }

        // Verify initial state: Bob has desired_hops 3 (default own_list_max_hops), Dave has 0
        {
            let mut s = alice_state.service.lock().unwrap();
            let reg = alice_state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let bob_can = crate::state::canonicalize_did(&bob_did).unwrap();
            let dave_can = crate::state::canonicalize_did(&dave_did).unwrap();
            assert_eq!(reg.own_contacts.get(&bob_can).unwrap().desired_hops, 3);
            assert_eq!(reg.own_contacts.get(&dave_can).unwrap().desired_hops, 0);
        }

        // Verify bounds: 0 and 5 fail
        let err0 = set_wot_max_hops_impl(0, Some(password.to_string()), &alice_state);
        assert!(err0.is_err());
        assert!(err0.unwrap_err().message.contains("maxHops must be between 1 and 4"));

        let err5 = set_wot_max_hops_impl(5, Some(password.to_string()), &alice_state);
        assert!(err5.is_err());
        assert!(err5.unwrap_err().message.contains("maxHops must be between 1 and 4"));

        // Valid update: set to 2
        let summary = set_wot_max_hops_impl(2, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(summary.own_list_max_hops, 2);

        // Verify register: Bob updated to 2, Dave stayed 0
        {
            let mut s = alice_state.service.lock().unwrap();
            let reg = alice_state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let bob_can = crate::state::canonicalize_did(&bob_did).unwrap();
            let dave_can = crate::state::canonicalize_did(&dave_did).unwrap();
            assert_eq!(reg.policy.own_list_max_hops, 2);
            assert_eq!(reg.own_contacts.get(&bob_can).unwrap().desired_hops, 2);
            assert_eq!(reg.own_contacts.get(&dave_can).unwrap().desired_hops, 0);
        }

        // Verify cached contacts reflect updated desired_hops
        let cached = alice_state.get_cached_contacts().unwrap();
        let bob_cached = cached.iter().find(|c| c.did == bob_did).unwrap();
        assert_eq!(bob_cached.desired_hops, Some(2));
        let dave_cached = cached.iter().find(|c| c.did == dave_did).unwrap();
        assert_eq!(dave_cached.desired_hops, Some(0));
    }

    #[test]
    fn test_multi_recipient_share_with_all_contacts_roundtrip() {
        let base_dir = temp_test_dir("multi_recipient");
        let password = "TestPassword123!";
        const CHARLIE_PHRASE: &str = "legal winner thank year wave sausage worth useful legal winner thank yellow";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let charlie_service = setup_test_app(&base_dir.join("charlie"), "Charlie", CHARLIE_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);
        let charlie_state = create_test_state(charlie_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let charlie_did = {
            let s = charlie_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // If Alice has no rated contacts, share_with_all_contacts should fail with clear error
        let empty_err = export_wot_package_impl(
            None,
            None,
            Some(true),
            Some(password.to_string()),
            &alice_state,
        );
        assert!(empty_err.is_err());
        assert!(empty_err.unwrap_err().message.contains("No rated contacts (level 1-3) available to encrypt for"));

        // Alice adds Bob (level 2) and Charlie (level 1)
        let bob_contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            drops: vec![],
        };
        let charlie_contact = FrontendContact {
            did: charlie_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Charlie".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(1),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            drops: vec![],
        };
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();
            alice_state.save_contact(&mut s, charlie_contact, Some(password)).unwrap();
        }

        // Alice exports a single .wot package with share_with_all_contacts: Some(true) and NO password
        let container_bytes = export_wot_package_impl(
            None,
            None,
            Some(true),
            Some(password.to_string()),
            &alice_state,
        ).unwrap();

        // BOTH Bob and Charlie successfully import that EXACT SAME container_bytes without any password!
        let bob_import = import_wot_package_impl(
            container_bytes.clone(),
            None,
            Some(password.to_string()),
            &bob_state,
        ).unwrap();
        assert_eq!(bob_import.sender_did, alice_did);
        assert_eq!(bob_import.imported, 1);
        assert!(!bob_import.is_provider_filter);

        let charlie_import = import_wot_package_impl(
            container_bytes,
            None,
            Some(password.to_string()),
            &charlie_state,
        ).unwrap();
        assert_eq!(charlie_import.sender_did, alice_did);
        assert_eq!(charlie_import.imported, 1);
        assert!(!charlie_import.is_provider_filter);
    }

    #[test]
    fn test_provider_filter_import_trust_and_voucher_preview() {
        let base_dir = temp_test_dir("provider_filter");
        let password = "TestPassword123!";
        const CHARLIE_PHRASE: &str = "legal winner thank year wave sausage worth useful legal winner thank yellow";
        const PROV_PHRASE: &str = "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let charlie_service = setup_test_app(&base_dir.join("charlie"), "Charlie", CHARLIE_PHRASE, password);
        let prov_service = setup_test_app(&base_dir.join("prov"), "Minuto Verband", PROV_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let charlie_state = create_test_state(charlie_service);

        let charlie_did = {
            let s = charlie_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let prov_did = prov_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let can_charlie = crate::state::canonicalize_did(&charlie_did).unwrap();
        let now_secs = Utc::now().timestamp() as u64;

        // Provider signs a filter certifying Charlie at level 2
        let raw_filter_json = prov_service.with_wallet_and_identity(|_w, id| {
            let signer = crate::state::CoreIdentitySigner::new(id);
            humoco_wot::create_and_sign_provider_filter(
                &signer,
                "Minuto Verband e.V.",
                1,
                now_secs,
                Some("https://minuto.org/filter.json".to_string()),
                &[(can_charlie.clone(), 2)],
            ).unwrap()
        }).unwrap();

        // 1. Alice imports the raw JSON provider filter
        let import_res = import_wot_package_impl(
            raw_filter_json.as_bytes().to_vec(),
            None,
            Some(password.to_string()),
            &alice_state,
        ).unwrap();

        assert!(import_res.is_provider_filter);
        assert_eq!(import_res.provider_name.as_deref(), Some("Minuto Verband e.V."));
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
        ).unwrap();
        assert_eq!(updated_provs[0].trust_level, 3);

        // 5. Evaluate voucher trust preview for Charlie
        let voucher_trust = evaluate_voucher_trust_preview_impl(
            charlie_did.clone(),
            vec![],
            &alice_state,
        ).unwrap();
        assert!(voucher_trust.score > 0.0);
        assert!(voucher_trust.trust_percent > 0);
        assert_eq!(voucher_trust.primary_bridge_name.as_deref(), Some("Minuto Verband e.V."));

        // 6. Test removing provider filter
        let remaining = remove_wot_provider_impl(
            prov_did.clone(),
            Some(password.to_string()),
            &alice_state,
        ).unwrap();
        assert_eq!(remaining.len(), 0);

        let summary_after = get_wot_summary_impl(&alice_state).unwrap();
        assert_eq!(summary_after.provider_filters_count, 0);

        // 7. Security check: import provider filter with non-HTTPS scheme and verify refresh rejects
        let insecure_filter_json = prov_service.with_wallet_and_identity(|_w, id| {
            let signer = crate::state::CoreIdentitySigner::new(id);
            humoco_wot::create_and_sign_provider_filter(
                &signer,
                "Minuto Insecure",
                2,
                now_secs,
                Some("ftp://insecure.example.com/filter.json".to_string()),
                &[(can_charlie.clone(), 2)],
            ).unwrap()
        }).unwrap();
        import_wot_package_impl(
            insecure_filter_json.as_bytes().to_vec(),
            None,
            Some(password.to_string()),
            &alice_state,
        ).unwrap();

        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let refresh_err = tokio_rt.block_on(refresh_wot_provider_url_impl(
            prov_did.clone(),
            Some(password.to_string()),
            &alice_state,
        ));
        assert!(refresh_err.is_err());
        assert!(refresh_err.unwrap_err().message.contains("Security violation"));
    }
}
