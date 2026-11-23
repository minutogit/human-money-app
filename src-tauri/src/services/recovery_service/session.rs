use super::storage::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use humoco_market::{
    build_recovery_request_uri, compute_recovery_sas, open_and_verify_guardian_release_reply,
    GuardianReleasePayload,
};

use crate::models::{
    ActiveRecoverySessionDto, ActiveRecoverySessionFile, FrontendContact, FrontendError,
};
use crate::services::{common::are_dids_equal, fs_helpers::write_file_atomic};

// ============================================================================
// Active Session Helpers
// ============================================================================

pub(crate) fn get_session_file_path(app: &tauri::AppHandle) -> Result<PathBuf, FrontendError> {
    use tauri::Manager;
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let tmp_dir = app_dir.join("tmp_restore");
    fs::create_dir_all(&tmp_dir).map_err(|e| FrontendError::from(e.to_string()))?;
    Ok(tmp_dir.join("active_recovery_session.json"))
}

pub(crate) fn load_active_session_from_path(
    path: &Path,
) -> Result<Option<ActiveRecoverySessionFile>, FrontendError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|e| {
        FrontendError::from(format!("Failed to read active recovery session: {}", e))
    })?;
    serde_json::from_slice::<ActiveRecoverySessionFile>(&bytes)
        .map(Some)
        .map_err(|e| FrontendError::from(format!("Failed to parse active recovery session: {}", e)))
}

pub(crate) fn save_active_session_to_path(
    path: &Path,
    session: &ActiveRecoverySessionFile,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec_pretty(session).map_err(|e| {
        FrontendError::from(format!(
            "Failed to serialize active recovery session: {}",
            e
        ))
    })?;
    write_file_atomic(path, &bytes)
}

pub(crate) fn delete_active_session_from_path(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub(super) fn format_contact_display_name(contact: &FrontendContact) -> Option<String> {
    let first = contact.profile.first_name.as_deref().unwrap_or("").trim();
    let last = contact.profile.last_name.as_deref().unwrap_or("").trim();
    let full = format!("{} {}", first, last).trim().to_string();
    (!full.is_empty()).then_some(full)
}

pub(super) fn resolve_ward_display_name(
    owner_did: &str,
    fallback_name: &str,
    contacts: &[FrontendContact],
) -> String {
    let trimmed_owner = owner_did.trim();
    if let Some(exact_name) = contacts
        .iter()
        .find(|c| c.did.trim() == trimmed_owner)
        .and_then(format_contact_display_name)
    {
        return exact_name;
    }

    let trimmed_fallback = fallback_name.trim();
    let is_prefixed_subaccount =
        human_money_core::crypto::get_prefix_from_user_id(trimmed_owner).is_some();
    if is_prefixed_subaccount && !trimmed_fallback.is_empty() && trimmed_fallback != trimmed_owner {
        return trimmed_fallback.to_string();
    }

    contacts
        .iter()
        .find(|c| are_dids_equal(&c.did, trimmed_owner))
        .and_then(format_contact_display_name)
        .unwrap_or_else(|| fallback_name.to_string())
}

pub(crate) fn build_active_session_dto(
    session: &ActiveRecoverySessionFile,
) -> ActiveRecoverySessionDto {
    let mut groups: BTreeMap<(String, u32), Vec<&GuardianReleasePayload>> = BTreeMap::new();
    for reply in &session.replies {
        groups
            .entry((reply.entry.owner_did.clone(), reply.entry.rotation_epoch))
            .or_default()
            .push(reply);
    }

    let best_group = groups
        .into_iter()
        .max_by_key(|((_, epoch), list)| (*epoch, list.len()));

    if let Some(((_, epoch), list)) = best_group {
        let first = list[0];
        let threshold_m = first.entry.threshold_m;
        let total_n = first.entry.total_n;
        let mut collected_share_indices: Vec<u8> =
            list.iter().map(|r| r.entry.share.index).collect();
        collected_share_indices.sort_unstable();
        let mut discovered_sync_urls = Vec::new();
        for r in &list {
            for d in r
                .entry
                .backup_sync_descriptors
                .iter()
                .chain(r.extra_sync_descriptors.iter())
            {
                if !d.write_url.is_empty() && !discovered_sync_urls.contains(&d.write_url) {
                    discovered_sync_urls.push(d.write_url.clone());
                }
            }
        }

        ActiveRecoverySessionDto {
            sas_code: session.sas_code.clone(),
            created_at: session.created_at,
            name_hint: session.name_hint.clone(),
            request_uri: session.request_uri.clone(),
            session_mode: session.session_mode.clone(),
            target_folder_name: session.target_folder_name.clone(),
            owner_did: Some(first.entry.owner_did.clone()),
            owner_display_name: Some(first.entry.owner_display_name.clone()),
            rotation_epoch: Some(epoch),
            threshold_m: Some(threshold_m),
            total_n: Some(total_n),
            collected_count: list.len(),
            collected_share_indices,
            question_ids: Some(first.entry.question_ids),
            has_enough_shares: list.len() >= threshold_m as usize,
            discovered_sync_urls,
            last_import_status: None,
        }
    } else {
        ActiveRecoverySessionDto {
            sas_code: session.sas_code.clone(),
            created_at: session.created_at,
            name_hint: session.name_hint.clone(),
            request_uri: session.request_uri.clone(),
            session_mode: session.session_mode.clone(),
            target_folder_name: session.target_folder_name.clone(),
            owner_did: None,
            owner_display_name: None,
            rotation_epoch: None,
            threshold_m: None,
            total_n: None,
            collected_count: 0,
            collected_share_indices: Vec::new(),
            question_ids: None,
            has_enough_shares: false,
            discovered_sync_urls: Vec::new(),
            last_import_status: None,
        }
    }
}

// ============================================================================
// Disaster Mode Session Logic (Pre-login compatible)
// ============================================================================

pub fn start_social_recovery_session_impl(
    name_hint: Option<String>,
    session_mode: Option<String>,
    target_folder_name: Option<String>,
    session_file_path: &Path,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let sk_temp = x25519_dalek::StaticSecret::random_from_rng(rand::rngs::OsRng);
    let pk_temp = x25519_dalek::PublicKey::from(&sk_temp);

    let sas_code = compute_recovery_sas(pk_temp.as_bytes());
    let now_secs = crate::sync_client::current_unix_timestamp();
    let request_uri =
        build_recovery_request_uri(pk_temp.as_bytes(), now_secs, name_hint.as_deref());
    let resolved_mode = match session_mode.as_deref() {
        Some("reset_local_password") => "reset_local_password".to_string(),
        _ => "restore_new_device".to_string(),
    };

    let session = ActiveRecoverySessionFile {
        sk_temp: sk_temp.to_bytes(),
        pk_temp: *pk_temp.as_bytes(),
        sas_code,
        created_at: now_secs,
        name_hint,
        request_uri,
        session_mode: resolved_mode,
        target_folder_name,
        replies: Vec::new(),
    };

    save_active_session_to_path(session_file_path, &session)?;
    Ok(build_active_session_dto(&session))
}

pub fn get_active_recovery_session_impl(
    session_file_path: &Path,
) -> Result<Option<ActiveRecoverySessionDto>, FrontendError> {
    let session_opt = load_active_session_from_path(session_file_path)?;
    Ok(session_opt.as_ref().map(build_active_session_dto))
}

pub fn cancel_social_recovery_session_impl(session_file_path: &Path) -> Result<(), FrontendError> {
    delete_active_session_from_path(session_file_path);
    Ok(())
}

pub fn import_guardian_recovery_reply_impl(
    reply_uri: String,
    session_file_path: &Path,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let mut session = load_active_session_from_path(session_file_path)?.ok_or_else(|| {
        FrontendError::with_code(
            "error.recovery.noActiveSession",
            "No active recovery session found.",
        )
    })?;

    let payload = open_and_verify_guardian_release_reply(&session.sk_temp, &reply_uri)
        .map_err(FrontendError::from)?;

    let mut superseded_epoch = false;
    if !session.replies.is_empty() {
        if !is_same_account_id(
            &session.replies[0].entry.owner_did,
            &payload.entry.owner_did,
        ) {
            return Err(FrontendError::with_code(
                "error.recovery.wrongProfile",
                format!(
                    "This release belongs to a different profile ({}) than the already collected releases ({}).",
                    payload.entry.owner_display_name, session.replies[0].entry.owner_display_name
                ),
            ));
        }
        let max_epoch = session
            .replies
            .iter()
            .map(|r| r.entry.rotation_epoch)
            .max()
            .unwrap_or(0);
        if payload.entry.rotation_epoch < max_epoch {
            return Err(FrontendError::with_code(
                "error.recovery.staleEpoch",
                format!(
                    "This contact still holds an outdated recovery state (epoch {} instead of {}).",
                    payload.entry.rotation_epoch, max_epoch
                ),
            ));
        }
        if payload.entry.rotation_epoch > max_epoch {
            superseded_epoch = !session.replies.is_empty();
            session
                .replies
                .retain(|r| r.entry.rotation_epoch == payload.entry.rotation_epoch);
        }
        if let Some(existing) = session.replies.first() {
            if payload.entry.threshold_m != existing.entry.threshold_m
                || payload.entry.total_n != existing.entry.total_n
                || payload.entry.question_ids != existing.entry.question_ids
                || payload.entry.kdf_params != existing.entry.kdf_params
                || payload.entry.r_enc != existing.entry.r_enc
                || payload.entry.share_commitments != existing.entry.share_commitments
            {
                return Err(FrontendError::with_code(
                    "error.recovery.inconsistentEpochMetadata",
                    "This guardian release has conflicting recovery metadata for the same epoch.",
                ));
            }
        }
    }

    // Deduplicate reply
    let already_present = session.replies.iter().any(|r| {
        is_same_account_id(&r.entry.owner_did, &payload.entry.owner_did)
            && r.entry.rotation_epoch == payload.entry.rotation_epoch
            && r.entry.share.index == payload.entry.share.index
    });

    if !already_present {
        session.replies.push(payload);
        save_active_session_to_path(session_file_path, &session)?;
    }

    let mut dto = build_active_session_dto(&session);
    dto.last_import_status = Some(if superseded_epoch {
        "superseded_epoch".to_string()
    } else if already_present {
        "duplicate".to_string()
    } else {
        "added".to_string()
    });
    Ok(dto)
}
