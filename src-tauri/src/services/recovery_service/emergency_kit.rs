use super::storage::*;
use super::*;
use std::path::Path;

use humoco_market::{
    build_emergency_kit_uri, canonicalize_owner_account_id, code_to_mnemonic_language,
    encode_qr_svg, mnemonic_language_to_code, parse_emergency_kit_uri, RecoveryKdfParams,
    MAX_RECOVERY_SYNC_DESCRIPTORS,
};

use crate::models::{
    ConfirmEmergencySheetResultDto, ExportedMnemonicKitDto, FrontendError,
    GuardianAssignmentStatus, GuardianStatusDto, GuardianWardDto, InspectedEmergencyKitDto,
    RecoveryProtectionStatusDto, RehearsalCheckResultDto,
};
use crate::services::{
    backup_service::sync_backup_impl, common::collect_backup_sync_targets_with_service,
};
use crate::state::AppState;

pub fn inspect_emergency_kit_uri_impl(
    uri: String,
) -> Result<InspectedEmergencyKitDto, FrontendError> {
    let kit = parse_emergency_kit_uri(&uri).map_err(FrontendError::from)?;
    let entropy_bytes = zeroize::Zeroizing::new(
        hex::decode(&kit.entropy_hex).map_err(|e| FrontendError::from(e.to_string()))?,
    );
    let core_lang = code_to_mnemonic_language(kit.language_code).map_err(FrontendError::from)?;
    let phrase = human_money_core::services::mnemonic::MnemonicProcessor::from_entropy(
        &entropy_bytes,
        core_lang,
    )
    .map_err(|e| FrontendError::from(e.to_string()))?;
    let word_count = phrase.split_whitespace().count();
    let sync_urls: Vec<String> = kit
        .sync_descriptors
        .iter()
        .filter(|d| !d.write_url.trim().is_empty())
        .map(|d| d.write_url.clone())
        .collect();
    let sync_descriptors_json: Vec<String> = kit
        .sync_descriptors
        .iter()
        .filter(|d| !d.write_url.trim().is_empty())
        .filter_map(|d| serde_json::to_string(d).ok())
        .collect();

    Ok(InspectedEmergencyKitDto {
        phrase,
        word_count,
        language: core_lang.into(),
        has_bip39_passphrase: kit.has_bip39_passphrase,
        user_prefix: kit.user_prefix.clone(),
        sync_urls,
        sync_descriptors_json,
    })
}

// ============================================================================
// Service Implementations
// ============================================================================

pub fn export_profile_mnemonic_impl(
    password: String,
    state: &AppState,
) -> Result<Option<ExportedMnemonicKitDto>, FrontendError> {
    let (phrase, lang, has_bip39_passphrase, user_prefix, active_descriptors, guardian_names) = {
        let mut service =
            state.authorize_and_lock(crate::state::RequiredAccess::Full, Some(&password))?;
        let exported = service
            .export_mnemonic(&password)
            .map_err(FrontendError::from)?;
        let Some((phrase, lang, has_bip39_passphrase)) = exported else {
            return Ok(None);
        };
        let user_prefix = service
            .with_wallet_and_identity(|_w, id| {
                human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string())
            })
            .ok()
            .flatten();
        let active_descriptors = collect_backup_sync_targets_with_service(
            state,
            &mut service,
            Some(&password),
            Some(MAX_RECOVERY_SYNC_DESCRIPTORS),
        )?;
        let guardian_names = load_recovery_owner_state(&mut service, Some(&password))?
            .map(|s| s.guardians.into_iter().map(|g| g.display_name).collect())
            .unwrap_or_default();
        (
            phrase,
            lang,
            has_bip39_passphrase,
            user_prefix,
            active_descriptors,
            guardian_names,
        )
    };

    let entropy_bytes = zeroize::Zeroizing::new(
        human_money_core::services::mnemonic::MnemonicProcessor::to_entropy(&phrase, lang)
            .map_err(|e| FrontendError::from(e.to_string()))?,
    );
    let entropy_hex = zeroize::Zeroizing::new(hex::encode(entropy_bytes.as_slice()));

    let kit = humoco_market::EmergencyKitData {
        version: 1,
        entropy_hex: (*entropy_hex).clone(),
        language_code: mnemonic_language_to_code(lang),
        user_prefix: user_prefix.clone(),
        has_bip39_passphrase,
        sync_descriptors: active_descriptors.clone(),
    };

    let kit_uri = build_emergency_kit_uri(&kit).map_err(FrontendError::from)?;
    let qr_svg = encode_qr_svg(kit_uri.as_bytes()).map_err(FrontendError::from)?;
    let sync_urls = active_descriptors
        .into_iter()
        .map(|d| d.write_url)
        .collect();

    Ok(Some(ExportedMnemonicKitDto {
        phrase,
        language_code: kit.language_code,
        has_bip39_passphrase,
        user_prefix,
        kit_uri,
        qr_svg,
        sync_urls,
        guardian_names,
    }))
}

pub async fn confirm_emergency_sheet_saved_impl(
    password: String,
    word_index: u8,
    control_word: String,
    state: &AppState,
    app: Option<&tauri::AppHandle>,
) -> Result<ConfirmEmergencySheetResultDto, FrontendError> {
    let (phrase, session) = {
        let service =
            state.authorize_and_lock(crate::state::RequiredAccess::Full, Some(&password))?;
        let session = state.session_token_for_service(&service)?;
        let exp = service
            .export_mnemonic(&password)
            .map_err(FrontendError::from)?;
        let (ph, _, _) = exp.ok_or_else(|| {
            FrontendError::from(
                "Wiederherstellungsphrase nicht auf diesem Gerät gespeichert.".to_string(),
            )
        })?;
        (ph, session)
    };

    let words: Vec<&str> = phrase.split_whitespace().collect();
    if word_index < 1 || word_index as usize > words.len() {
        return Err(FrontendError::from(format!(
            "Ungültiger Wortindex {}: muss zwischen 1 und {} liegen",
            word_index,
            words.len()
        )));
    }

    let expected = words[(word_index - 1) as usize];
    if control_word.trim().to_lowercase() != expected.trim().to_lowercase() {
        return Err(FrontendError::from(
            "Das Kontrollwort stimmt nicht mit deinem Notfall-Blatt überein.".to_string(),
        ));
    }

    let now_secs = crate::sync_client::current_unix_timestamp();
    {
        let _recovery_guard = state.recovery_op_lock.lock().await;
        state.with_session_required(
            &session,
            "Active wallet session changed before confirming emergency sheet",
            |service| {
                let mut owner_state =
                    load_recovery_owner_state(service, Some(&password))?.unwrap_or_default();
                owner_state.emergency_sheet_confirmed_at = Some(now_secs);
                save_recovery_owner_state_with_cache(state, service, &owner_state, Some(&password))
            },
        )?;
    }

    if !state.is_generation_current(session.generation) {
        return Err(FrontendError::from(
            "Active wallet session changed before backup sync could run".to_string(),
        ));
    }

    let backup_sync_result = match app {
        Some(handle) => sync_backup_impl(Some(password), None, state, handle)
            .await
            .ok(),
        None => None,
    };

    Ok(ConfirmEmergencySheetResultDto {
        confirmed_at: now_secs,
        backup_sync_result,
    })
}

pub fn get_recovery_protection_status_impl(
    password: Option<&str>,
    state: &AppState,
) -> Result<RecoveryProtectionStatusDto, FrontendError> {
    let now_secs = crate::sync_client::current_unix_timestamp();

    let (
        has_voucher,
        owner_state,
        guardian_vault,
        contacts,
        security_policy,
        lifecycle,
        candidate_syncs,
    ) = {
        let (mut service, resolved_pw) =
            state.authorize_and_lock_secret(crate::state::RequiredAccess::Unlocked, password)?;
        let pw = resolved_pw.as_deref().map(String::as_str);
        let has_voucher = service
            .with_wallet(|w| !w.voucher_store.vouchers.is_empty())
            .unwrap_or_else(|_| {
                state
                    .get_cached_history()
                    .map(|h| !h.is_empty())
                    .unwrap_or(false)
            });
        let owner =
            load_recovery_owner_state_with_cache(state, &mut service, pw)?.unwrap_or_default();
        let vault = load_guardian_vault_with_cache(state, &mut service, pw)?;
        let contacts = state
            .load_contacts(&mut service, pw)
            .or_else(|_| state.get_cached_contacts())
            .unwrap_or_default();
        let security_policy = state
            .get_or_load_security_policy(&mut service, pw)
            .map_err(FrontendError::from)?;
        let lifecycle = state
            .get_or_load_internal_lifecycle(&mut service, pw)
            .map_err(FrontendError::from)?;
        let candidate_syncs = collect_backup_sync_targets_with_service(
            state,
            &mut service,
            pw,
            Some(MAX_RECOVERY_SYNC_DESCRIPTORS),
        )?;
        (
            has_voucher,
            owner,
            vault,
            contacts,
            security_policy,
            lifecycle,
            candidate_syncs,
        )
    };

    let confirmed_guardians_count = owner_state
        .guardians
        .iter()
        .filter(|g| matches!(g.status, GuardianAssignmentStatus::Confirmed { .. }))
        .count();

    let required_threshold = owner_state.threshold_m;

    let guardians: Vec<GuardianStatusDto> = owner_state
        .guardians
        .into_iter()
        .map(|g| {
            let last_contacted_at = match g.status {
                GuardianAssignmentStatus::Confirmed { acked_at } => Some(acked_at),
                _ => None,
            };
            let last_activity = last_contacted_at.unwrap_or(g.assigned_at);
            let is_stale_90d = now_secs.saturating_sub(last_activity) > 90 * 86400;
            GuardianStatusDto {
                user_id: g.user_id,
                display_name: g.display_name,
                share_index: g.share_index,
                status: g.status,
                assigned_at: g.assigned_at,
                last_contacted_at,
                is_stale_90d,
                updated_at: g.sealed_entry.updated_at,
            }
        })
        .collect();

    let emergency_sheet_confirmed_at = owner_state.emergency_sheet_confirmed_at;
    let has_identity_recovery = (confirmed_guardians_count >= required_threshold as usize
        && required_threshold >= 2)
        || emergency_sheet_confirmed_at.is_some();

    let sync_backup_count = candidate_syncs.len();
    let has_sync_backup =
        security_policy.sync_backup_enabled && lifecycle.last_sync_backup_at.is_some();
    let has_local_backup = lifecycle.last_backup_at.is_some()
        && security_policy
            .backup_sync_folder
            .as_ref()
            .is_some_and(|f| !f.trim().is_empty() && Path::new(f).is_dir());
    let has_voucher_backup = has_sync_backup || has_local_backup;

    let guardian_wards: Vec<GuardianWardDto> = guardian_vault
        .wards
        .values()
        .map(|e| GuardianWardDto {
            owner_did: e.owner_did.clone(),
            owner_display_name: resolve_ward_display_name(
                &e.owner_did,
                &e.owner_display_name,
                &contacts,
            ),
            rotation_epoch: e.rotation_epoch,
            threshold_m: e.threshold_m,
            total_n: e.total_n,
            updated_at: e.updated_at,
        })
        .collect();

    let rehearsal_due = !guardians.is_empty()
        && owner_state.answer_verifiers.is_some()
        && owner_state
            .next_rehearsal_at
            .is_some_and(|ts| now_secs >= ts);
    let question_ids = (!guardians.is_empty()).then_some(owner_state.question_ids);
    let rehearsal_stage = owner_state.rehearsal_stage;
    let next_rehearsal_at = owner_state.next_rehearsal_at;

    Ok(RecoveryProtectionStatusDto {
        has_voucher,
        confirmed_guardians_count,
        required_threshold,
        guardians,
        emergency_sheet_confirmed_at,
        has_identity_recovery,
        sync_backup_count,
        has_sync_backup,
        has_local_backup,
        has_voucher_backup,
        guardian_wards,
        rehearsal_due,
        question_ids,
        rehearsal_stage,
        next_rehearsal_at,
    })
}

pub async fn verify_recovery_rehearsal_with_password_impl(
    answers: [String; 3],
    now_secs: u64,
    password: Option<&str>,
    state: &AppState,
) -> Result<RehearsalCheckResultDto, FrontendError> {
    let auth = state.authorize_session(crate::state::RequiredAccess::DailyPay, password)?;
    let session = &auth.token;
    let pw = auth.pw();

    let (
        owner_account_id,
        rotation_epoch,
        question_ids,
        expected,
        kdf_params,
        current_stage,
        current_next_at,
    ) = state.with_session_required(
        session,
        "Active wallet session changed before starting recovery rehearsal verification",
        |service| {
            let owner_state = load_recovery_owner_state(service, pw)?.ok_or_else(|| {
                FrontendError::from(
                    "Social recovery is not configured for this profile.".to_string(),
                )
            })?;
            let expected = owner_state.answer_verifiers.ok_or_else(|| {
                FrontendError::from(
                    "Recovery rehearsal verifiers are not configured. Please update your recovery questions."
                        .to_string(),
                )
            })?;
            let user_id = service
                .with_wallet_and_identity(|_, id| id.user_id.clone())
                .map_err(FrontendError::from)?;
            let owner_account_id =
                canonicalize_owner_account_id(&user_id).map_err(FrontendError::from)?;
            let kdf_params = RecoveryKdfParams {
                m_kib: owner_state.rehearsal_kdf_m_kib,
                t_cost: owner_state.rehearsal_kdf_t_cost,
                p_cost: 1,
                salt: [0u8; 16],
            };
            Ok((
                owner_account_id,
                owner_state.rotation_epoch,
                owner_state.question_ids,
                expected,
                kdf_params,
                owner_state.rehearsal_stage,
                owner_state.next_rehearsal_at,
            ))
        },
    )?;

    // Run heavy Argon2id verification outside ALL mutexes on the blocking threadpool.
    let question_matches = tokio::task::spawn_blocking(move || {
        let ans_refs = [
            answers[0].as_str(),
            answers[1].as_str(),
            answers[2].as_str(),
        ];
        humoco_market::verify_rehearsal_answers(
            &owner_account_id,
            rotation_epoch,
            &question_ids,
            &ans_refs,
            &expected,
            &kdf_params,
        )
    })
    .await
    .map_err(|e| {
        FrontendError::from(format!(
            "Recovery rehearsal verification task failed: {}",
            e
        ))
    })?
    .map_err(FrontendError::from)?;

    let all_correct = question_matches.iter().all(|&m| m);
    if all_correct {
        let (next_stage, next_at) = state.with_session_required(
            session,
            "Active wallet session changed during recovery rehearsal verification",
            |service| {
                let mut owner_state = load_recovery_owner_state(service, pw)?.ok_or_else(|| {
                    FrontendError::from(
                        "Social recovery is not configured for this profile.".to_string(),
                    )
                })?;
                if owner_state.rotation_epoch != rotation_epoch {
                    return Err(FrontendError::from(
                        "Social recovery configuration changed during rehearsal verification."
                            .to_string(),
                    ));
                }
                let next_stage = owner_state
                    .rehearsal_stage
                    .saturating_add(1)
                    .min((REHEARSAL_INTERVAL_SECS.len() - 1) as u8);
                let interval = REHEARSAL_INTERVAL_SECS[next_stage as usize];
                let next_at = now_secs.saturating_add(interval);
                owner_state.rehearsal_stage = next_stage;
                owner_state.next_rehearsal_at = Some(next_at);
                save_recovery_owner_state_with_cache(state, service, &owner_state, pw)?;
                Ok((next_stage, next_at))
            },
        )?;
        Ok(RehearsalCheckResultDto {
            all_correct: true,
            question_matches,
            rehearsal_stage: next_stage,
            next_rehearsal_at: next_at,
        })
    } else {
        Ok(RehearsalCheckResultDto {
            all_correct: false,
            question_matches,
            rehearsal_stage: current_stage,
            next_rehearsal_at: current_next_at.unwrap_or(now_secs),
        })
    }
}

pub fn snooze_recovery_rehearsal_with_password_impl(
    hours: Option<u64>,
    now_secs: u64,
    password: Option<&str>,
    state: &AppState,
) -> Result<u64, FrontendError> {
    let (mut service, resolved_pw) =
        state.authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password)?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let mut owner_state = load_recovery_owner_state(&mut service, pw)?.ok_or_else(|| {
        FrontendError::from("Social recovery is not configured for this profile.".to_string())
    })?;
    let snooze_secs = hours.unwrap_or(24).clamp(1, 168) * 3600;
    let next_at = now_secs.saturating_add(snooze_secs);
    owner_state.next_rehearsal_at = Some(next_at);
    save_recovery_owner_state_with_cache(state, &mut service, &owner_state, pw)?;
    Ok(next_at)
}
