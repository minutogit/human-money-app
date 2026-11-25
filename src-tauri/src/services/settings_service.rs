use log::info;

use crate::{
    models::{FrontendError, ReachPresetDto},
    services::market_service::{get_own_card_max_hops_inner, prepare_own_card_max_hops_for_write},
    settings::{
        AppConfigDto, SecurityPolicy, SecurityPolicyPatch, UiPreferences, UiPreferencesPatch,
    },
    state::RequiredAccess,
    AppState,
};

pub fn preset_values(preset: &str) -> Result<(u8, u8, u8, u8), FrontendError> {
    match preset {
        "near" => Ok((1, 2, 1, 1)),
        "medium" => Ok((2, 1, 2, 2)),
        "far" => Ok((3, 1, 3, 2)),
        _ => Err(FrontendError::from(format!(
            "Invalid reach preset: {}",
            preset
        ))),
    }
}

pub fn detect_reach_preset(
    own_list_max_hops: u8,
    share_min_level: u8,
    card_max_hops: u8,
    media_auto_load_max_hops: u8,
) -> String {
    for preset in ["near", "medium", "far"] {
        if let Ok(expected) = preset_values(preset) {
            if expected
                == (
                    own_list_max_hops,
                    share_min_level,
                    card_max_hops,
                    media_auto_load_max_hops,
                )
            {
                return preset.to_string();
            }
        }
    }
    "custom".to_string()
}

pub fn get_reach_preset_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<ReachPresetDto, FrontendError> {
    let (mut service, resolved_pw) =
        state.authorize_and_lock_secret(RequiredAccess::Unlocked, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let prefs = state
        .get_or_load_ui_preferences(&mut service, pw)
        .map_err(FrontendError::from)?;
    let media_auto_load_max_hops = prefs.media_auto_load_max_hops.min(3);
    let share_min_level = prefs.share_min_level.min(3);

    let reg = state
        .get_or_load_wot_register(&mut service, pw)
        .map_err(FrontendError::from)?;
    let card_max_hops = get_own_card_max_hops_inner(state, &mut service, pw)?;
    let own_list_max_hops = reg.policy.own_list_max_hops.clamp(1, 3);

    let preset = detect_reach_preset(
        own_list_max_hops,
        share_min_level,
        card_max_hops,
        media_auto_load_max_hops,
    );

    Ok(ReachPresetDto {
        preset,
        own_list_max_hops,
        card_max_hops,
        share_min_level,
        media_auto_load_max_hops,
    })
}

pub fn apply_reach_preset_impl(
    preset: String,
    password: Option<String>,
    state: &AppState,
) -> Result<ReachPresetDto, FrontendError> {
    let (own_list_max_hops, share_min_level, card_max_hops, media_auto_load_max_hops) =
        preset_values(&preset)?;
    let (mut service, resolved_pw) =
        state.authorize_and_lock_secret(RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let reg = crate::services::wot_service::prepare_wot_max_hops(
        state,
        &mut service,
        own_list_max_hops,
        pw,
    )?;

    let mut prefs = state
        .get_or_load_ui_preferences(&mut service, pw)
        .map_err(FrontendError::from)?;
    let prev_share_min = prefs.share_min_level;
    prefs.share_min_level = share_min_level.min(3);
    prefs.media_auto_load_max_hops = media_auto_load_max_hops.min(3);
    let bump_for_eviction = prefs.share_min_level > prev_share_min;

    let stored = prepare_own_card_max_hops_for_write(
        state,
        &mut service,
        card_max_hops,
        bump_for_eviction,
        pw,
    )?;

    let mut batch = crate::state::EncryptedBatchCommit::new();
    batch
        .wot(reg.clone())
        .own_market_profile(stored)
        .ui_preferences(prefs);
    batch
        .commit(state, &mut service, pw)
        .map_err(|e| FrontendError::from(format!("Failed to persist reach preset: {}", e)))?;

    crate::services::wot_service::apply_desired_hops_to_cached_contacts(state, &reg);

    Ok(ReachPresetDto {
        preset,
        own_list_max_hops,
        card_max_hops,
        share_min_level,
        media_auto_load_max_hops,
    })
}

pub fn get_app_config_impl(state: &AppState) -> Result<AppConfigDto, FrontendError> {
    info!("Getting split app config from cache...");
    state.authorize(RequiredAccess::Unlocked, None)?;
    state.get_cached_app_config().map_err(FrontendError::from)
}

pub fn patch_ui_preferences_impl(
    patch: UiPreferencesPatch,
    password: Option<String>,
    state: &AppState,
) -> Result<UiPreferences, FrontendError> {
    info!("Patching UiPreferences...");
    patch.validate().map_err(FrontendError::from)?;
    let (mut service, resolved_pw) =
        state.authorize_and_lock_secret(RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut wake = crate::state::SyncWakeRequest::default();
    let updated = state
        .mutate_ui_preferences(&mut service, pw, |prefs| {
            let was_wifi_only = prefs.market_poll_wifi_only;
            let was_market = prefs.market_sync_enabled;
            let was_wot_sync = prefs.wot_share_via_sync;
            let was_post_contacts = prefs.post_accept_from_contacts;
            patch.apply_to(prefs);
            let wifi_unlocked = was_wifi_only && !prefs.market_poll_wifi_only;
            wake = crate::state::SyncWakeRequest {
                seal: wifi_unlocked,
                sync_backup: wifi_unlocked,
                market: wifi_unlocked
                    || (!was_market && prefs.market_sync_enabled)
                    || (!was_wot_sync && prefs.wot_share_via_sync),
                post: wifi_unlocked || (!was_post_contacts && prefs.post_accept_from_contacts),
                ..Default::default()
            };
        })
        .map_err(FrontendError::from)?;
    if !wake.is_empty() {
        state.wake_sync_coordinator(wake);
    }
    Ok(updated)
}

pub fn patch_security_policy_impl(
    patch: SecurityPolicyPatch,
    password: Option<String>,
    state: &AppState,
) -> Result<SecurityPolicy, FrontendError> {
    info!("Patching SecurityPolicy...");
    patch.validate().map_err(FrontendError::from)?;
    let password_z = password.map(zeroize::Zeroizing::new);
    let pw = password_z.as_deref().map(String::as_str);
    let mut service = state.authorize_and_lock(RequiredAccess::Full, pw)?;
    let mut wake = crate::state::SyncWakeRequest::default();
    let mut prev_timeout = 0u64;
    let updated = state
        .mutate_security_policy(&mut service, pw, |policy| {
            let was_sync_backup = policy.sync_backup_enabled;
            let was_auto_backup = policy.auto_backup_enabled;
            let prev_folder = policy.backup_sync_folder.clone();
            prev_timeout = policy.session_timeout_seconds;
            patch.apply_to(policy);
            wake = crate::state::SyncWakeRequest {
                sync_backup: !was_sync_backup && policy.sync_backup_enabled,
                local_backup: (!was_auto_backup && policy.auto_backup_enabled)
                    || (policy.auto_backup_enabled
                        && policy.backup_sync_folder.is_some()
                        && policy.backup_sync_folder != prev_folder),
                ..Default::default()
            };
        })
        .map_err(FrontendError::from)?;
    if updated.session_timeout_seconds == 0 {
        service.lock_session();
        state.clear_one_shot_pin_grant();
    } else if updated.session_timeout_seconds < prev_timeout && service.is_session_active() {
        let re_unlocked = pw.is_some_and(|secret| {
            service
                .unlock_session(secret, updated.session_timeout_seconds)
                .is_ok()
        });
        if !re_unlocked {
            service.lock_session();
            state.clear_one_shot_pin_grant();
        }
    }
    if service.is_session_active() && !wake.is_empty() {
        state.wake_sync_coordinator(wake);
    }
    Ok(updated)
}

pub fn set_conflict_local_override_impl(
    proof_id: String,
    value: bool,
    note: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    info!(
        "Setting local override for conflict {} to {} with note: {:?}",
        proof_id, value, note
    );
    let (mut service, resolved_pw) =
        state.authorize_and_lock_secret(RequiredAccess::Full, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    service
        .set_conflict_local_override(&proof_id, value, note, pw)
        .map_err(FrontendError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::fs_helpers::TempDirGuard;
    use crate::settings::{
        InternalLifecycleState, INTERNAL_LIFECYCLE_STATE_KEY, SECURITY_POLICY_KEY,
        UI_PREFERENCES_KEY,
    };

    fn setup_dailypay_pin_fixture(tag: &str) -> (TempDirGuard, AppState, String) {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir(tag));
        let dir = &dir_guard.0;
        let keystore_dir = dir.join("keystore");
        let master_pw = "MasterPass-Settings-2026!";

        let service = crate::test_support::setup_test_app(
            &dir.join("wallet"),
            "Settings Tester",
            crate::test_support::ALICE_PHRASE,
            master_pw,
        );
        let folder = service.list_profiles().unwrap()[0].folder_name.clone();
        let state = AppState::new(service);

        crate::commands::auth::enable_profile_pin_impl(
            master_pw,
            "246810",
            "dailyPay",
            &folder,
            &keystore_dir,
            "dev-settings-1",
            &state,
        )
        .unwrap();
        state.service.lock().unwrap().logout();
        crate::commands::auth::login_with_pin_impl(
            &folder,
            "246810",
            false,
            "device-drops-test".into(),
            &keystore_dir,
            "dev-settings-1",
            &state,
            None,
        )
        .unwrap();

        let ui_base = UiPreferences::default();
        let sec_base = SecurityPolicy {
            session_timeout_seconds: 300,
            backup_sync_folder: Some("/tmp/initial-backup".to_string()),
            ..Default::default()
        };
        let lc_base = InternalLifecycleState {
            pending_restore_l2_reconciliation: true,
            ..Default::default()
        };
        state.set_cached_security_policy(Some(sec_base.clone()));
        crate::commands::auth::unlock_session_with_pin_impl(
            &folder,
            "246810",
            &keystore_dir,
            "dev-settings-1",
            &state,
        )
        .unwrap();
        {
            let mut svc = state.service.lock().unwrap();
            crate::state::save_encrypted_json(
                &mut svc,
                UI_PREFERENCES_KEY,
                &ui_base,
                Some(master_pw),
            )
            .unwrap();
            crate::state::save_encrypted_json(
                &mut svc,
                SECURITY_POLICY_KEY,
                &sec_base,
                Some(master_pw),
            )
            .unwrap();
            crate::state::save_local_sidecar_json(
                &mut svc,
                INTERNAL_LIFECYCLE_STATE_KEY,
                &lc_base,
                Some(master_pw),
            )
            .unwrap();
        }
        state.set_cached_ui_preferences(Some(ui_base));
        state.set_cached_security_policy(Some(sec_base));
        state.set_cached_internal_lifecycle(Some(lc_base));

        (dir_guard, state, master_pw.to_string())
    }

    fn setup_s11_settings_fixture(tag: &str) -> (TempDirGuard, AppState, String) {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir(tag));
        let pw = "MasterPass-S11-2026!";
        let service = crate::test_support::setup_test_app(
            &dir_guard.0.join("wallet"),
            "S11 Settings Tester",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let state = crate::test_support::create_test_state(service);

        {
            let mut svc = state.service.lock().unwrap();
            let ui_init = UiPreferences {
                market_sync_enabled: false,
                wot_share_via_sync: false,
                post_accept_from_contacts: false,
                market_poll_wifi_only: true,
                ..Default::default()
            };
            let sec_init = SecurityPolicy {
                session_timeout_seconds: 600,
                sync_backup_enabled: false,
                auto_backup_enabled: false,
                backup_sync_folder: None,
                ..Default::default()
            };
            state
                .save_ui_preferences(&mut svc, ui_init, Some(pw))
                .unwrap();
            state
                .save_security_policy(&mut svc, sec_init, Some(pw))
                .unwrap();
            let reg = state.load_wot_register(&mut svc, Some(pw)).unwrap();
            state.save_wot_register(&mut svc, &reg, Some(pw)).unwrap();
        }

        (dir_guard, state, pw.to_string())
    }

    fn reset_wake(st: &AppState) {
        let mut guard = st.sync_control.lock().unwrap();
        guard.pending_wake = Default::default();
    }

    #[test]
    fn test_reach_preset_values_and_detection() {
        assert_eq!(preset_values("near").unwrap(), (1, 2, 1, 1));
        assert_eq!(preset_values("medium").unwrap(), (2, 1, 2, 2));
        assert_eq!(preset_values("far").unwrap(), (3, 1, 3, 2));
        let invalid_err = preset_values("ultra").unwrap_err();
        assert_eq!(invalid_err.message, "Invalid reach preset: ultra");

        assert_eq!(detect_reach_preset(1, 2, 1, 1), "near");
        assert_eq!(detect_reach_preset(2, 1, 2, 2), "medium");
        assert_eq!(detect_reach_preset(3, 1, 3, 2), "far");

        assert_eq!(detect_reach_preset(1, 1, 1, 1), "custom");
        assert_eq!(detect_reach_preset(2, 1, 3, 2), "custom");
        assert_eq!(detect_reach_preset(2, 2, 2, 2), "custom");
        assert_eq!(detect_reach_preset(2, 1, 2, 0), "custom");
        assert_eq!(detect_reach_preset(3, 1, 3, 3), "custom");
    }

    #[test]
    fn test_dailypay_can_patch_ui_preferences_with_clamping_and_preserves_lifecycle() {
        let (dir_guard, state, master_pw) = setup_dailypay_pin_fixture("settings_ui_patch");
        assert!(dir_guard.0.exists());
        assert!(!master_pw.is_empty());

        let updated_ui = patch_ui_preferences_impl(
            UiPreferencesPatch {
                ui_mode: Some("expert".to_string()),
                share_min_level: Some(9),
                media_auto_load_max_hops: Some(7),
                ..Default::default()
            },
            None,
            &state,
        )
        .unwrap();
        assert_eq!(updated_ui.ui_mode, "expert");
        assert_eq!(updated_ui.share_min_level, 3);
        assert_eq!(updated_ui.media_auto_load_max_hops, 3);

        let cfg = get_app_config_impl(&state).unwrap();
        assert_eq!(cfg.ui_preferences.ui_mode, "expert");
        assert_eq!(cfg.ui_preferences.share_min_level, 3);
        assert_eq!(cfg.ui_preferences.media_auto_load_max_hops, 3);
        assert!(cfg.lifecycle.pending_restore_l2_reconciliation);

        let bad_dir_err = patch_ui_preferences_impl(
            UiPreferencesPatch {
                last_used_directory: Some(Some("/tmp/bad\ndir".to_string())),
                ..Default::default()
            },
            None,
            &state,
        )
        .unwrap_err();
        assert!(bad_dir_err.message.contains("control characters"));
    }

    #[test]
    fn test_dailypay_rejected_for_security_policy_and_conflict_override_full_gates() {
        let (dir_guard, state, master_pw) = setup_dailypay_pin_fixture("settings_full_gates");
        assert!(dir_guard.0.exists());

        for patch in [
            SecurityPolicyPatch {
                auto_backup_enabled: Some(false),
                ..Default::default()
            },
            SecurityPolicyPatch {
                session_timeout_seconds: Some(60),
                ..Default::default()
            },
            SecurityPolicyPatch {
                dev_mode_unlocked: Some(true),
                ..Default::default()
            },
            SecurityPolicyPatch {
                backup_sync_folder: Some(Some("/tmp/other".to_string())),
                ..Default::default()
            },
        ] {
            let err = patch_security_policy_impl(patch, None, &state).unwrap_err();
            assert_eq!(err.code, "error.auth.insufficientAccessMode");
        }

        let override_daily_err = set_conflict_local_override_impl(
            "missing-proof-id".to_string(),
            true,
            Some("review note".to_string()),
            None,
            &state,
        )
        .unwrap_err();
        assert_eq!(override_daily_err.code, "error.auth.insufficientAccessMode");

        let override_full_err = set_conflict_local_override_impl(
            "missing-proof-id".to_string(),
            true,
            Some("review note".to_string()),
            Some(master_pw),
            &state,
        )
        .unwrap_err();
        assert_ne!(override_full_err.code, "error.auth.insufficientAccessMode");
        assert!(override_full_err.message.contains("missing-proof-id"));
    }

    #[test]
    fn test_full_access_patch_security_policy_double_option_and_timeout_zero_clears_session() {
        let (dir_guard, state, master_pw) = setup_dailypay_pin_fixture("settings_sec_patch");
        assert!(dir_guard.0.exists());

        let sec_after_omit = patch_security_policy_impl(
            SecurityPolicyPatch {
                auto_backup_enabled: Some(false),
                ..Default::default()
            },
            Some(master_pw.clone()),
            &state,
        )
        .unwrap();
        assert!(!sec_after_omit.auto_backup_enabled);
        assert_eq!(
            sec_after_omit.backup_sync_folder.as_deref(),
            Some("/tmp/initial-backup")
        );

        let sec_after_clear = patch_security_policy_impl(
            SecurityPolicyPatch {
                backup_sync_folder: Some(None),
                ..Default::default()
            },
            Some(master_pw.clone()),
            &state,
        )
        .unwrap();
        assert_eq!(sec_after_clear.backup_sync_folder, None);
        assert!(
            get_app_config_impl(&state)
                .unwrap()
                .lifecycle
                .pending_restore_l2_reconciliation
        );

        crate::commands::auth::elevate_session_with_password_impl(&master_pw, &state).unwrap();
        assert!(state.service.lock().unwrap().is_session_active());
        let sec_zero = patch_security_policy_impl(
            SecurityPolicyPatch {
                session_timeout_seconds: Some(0),
                ..Default::default()
            },
            Some(master_pw),
            &state,
        )
        .unwrap();
        assert_eq!(sec_zero.session_timeout_seconds, 0);

        let status_after = crate::commands::auth::get_session_status_impl(&state).unwrap();
        assert!(!status_after.has_cached_session);
        assert_eq!(status_after.access_mode, "dailyPay");
        assert_eq!(status_after.base_access_mode, "dailyPay");
    }

    #[test]
    fn test_apply_reach_preset_atomic_rollback_and_clean_apply() {
        let (dir_guard, state, pw) = setup_s11_settings_fixture("s11_f3_reach_preset");
        assert!(dir_guard.0.exists());

        let invalid_err =
            apply_reach_preset_impl("ultra".to_string(), Some(pw.clone()), &state).unwrap_err();
        assert_eq!(invalid_err.message, "Invalid reach preset: ultra");

        let before_preset = get_reach_preset_impl(Some(pw.clone()), &state).unwrap();
        assert_eq!(before_preset.preset, "medium");

        let fault_guard = crate::test_support::WRITE_FAULT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        human_money_core::storage::file_storage::inject_write_fault_once("seal.enc");
        let fault_err =
            apply_reach_preset_impl("near".to_string(), Some(pw.clone()), &state).unwrap_err();
        drop(fault_guard);
        assert!(
            fault_err
                .message
                .starts_with("Failed to persist reach preset:"),
            "unexpected fault error: {}",
            fault_err.message
        );

        let after_fault = get_reach_preset_impl(Some(pw.clone()), &state).unwrap();
        assert_eq!(after_fault, before_preset);
        assert_eq!(
            state.get_cached_ui_preferences().unwrap().share_min_level,
            1
        );
        assert_eq!(state.cache_wot().unwrap().policy.own_list_max_hops, 2);

        let applied_near = apply_reach_preset_impl("near".to_string(), Some(pw.clone()), &state)
            .unwrap();
        assert_eq!(applied_near.preset, "near");
        assert_eq!(applied_near.own_list_max_hops, 1);
        assert_eq!(applied_near.card_max_hops, 1);
        assert_eq!(applied_near.share_min_level, 2);
        assert_eq!(applied_near.media_auto_load_max_hops, 1);
        assert_eq!(
            get_reach_preset_impl(Some(pw.clone()), &state).unwrap(),
            applied_near
        );

        let applied_far = apply_reach_preset_impl("far".to_string(), Some(pw.clone()), &state)
            .unwrap();
        assert_eq!(applied_far.preset, "far");
        assert_eq!(applied_far.own_list_max_hops, 3);
        assert_eq!(applied_far.card_max_hops, 3);
        assert_eq!(applied_far.share_min_level, 1);
        assert_eq!(applied_far.media_auto_load_max_hops, 2);
        assert_eq!(get_reach_preset_impl(Some(pw), &state).unwrap(), applied_far);
    }

    #[test]
    fn test_apply_reach_preset_bumps_unpublished_tombstone_only_on_share_min_increase() {
        let (dir_guard, state, pw) = setup_s11_settings_fixture("s11_f3_tombstone_bump");
        assert!(dir_guard.0.exists());

        let v0 = {
            let mut svc = state.service.lock().unwrap();
            let mut prof = crate::services::market_service::load_own_market_profile_state(
                &state,
                &mut svc,
                Some(&pw),
            )
            .unwrap();
            let now = chrono::Utc::now().timestamp().max(0) as u64;
            crate::services::market_service::rebuild_and_sign_own_card(&mut svc, &mut prof, now)
                .unwrap();
            prof.published = false;
            let v = prof.last_card.as_ref().unwrap().payload.v;
            let mut batch = crate::state::EncryptedBatchCommit::new();
            batch.own_market_profile(prof);
            batch.commit(&state, &mut svc, Some(&pw)).unwrap();
            v
        };

        let read_v = || -> u64 {
            let mut svc = state.service.lock().unwrap();
            crate::services::market_service::load_own_market_profile_state(
                &state,
                &mut svc,
                Some(&pw),
            )
            .unwrap()
            .last_card
            .unwrap()
            .payload
            .v
        };

        // share_min_level increases (1 -> 2): MUST bump unpublished tombstone card version
        apply_reach_preset_impl("near".to_string(), Some(pw.clone()), &state).unwrap();
        let v_after_inc = read_v();
        assert!(
            v_after_inc > v0,
            "increasing share_min_level (1 -> 2) must bump tombstone version ({v_after_inc} vs {v0})"
        );

        // Equal share_min_level (2 -> 2): MUST NOT bump tombstone version
        apply_reach_preset_impl("near".to_string(), Some(pw.clone()), &state).unwrap();
        assert_eq!(read_v(), v_after_inc);

        // Decreasing share_min_level (2 -> 1): MUST NOT bump tombstone version
        apply_reach_preset_impl("far".to_string(), Some(pw.clone()), &state).unwrap();
        assert_eq!(read_v(), v_after_inc);
    }

    #[test]
    fn test_get_reach_preset_via_one_shot_pin_grant_on_cold_cache() {
        let (dir_guard, state, pw) = setup_s11_settings_fixture("s11_f3_one_shot");
        assert!(dir_guard.0.exists());

        apply_reach_preset_impl("near".to_string(), Some(pw.clone()), &state).unwrap();
        let grant = {
            let mut svc = state.service.lock().unwrap();
            svc.lock_session();
            state
                .issue_one_shot_pin_grant(&svc, zeroize::Zeroizing::new(pw))
                .unwrap()
        };
        state.set_cached_ui_preferences(None);
        {
            let svc = state.service.lock().unwrap();
            state.set_cached_wot_for_service(&svc, None);
            state.set_cached_own_market_profile_for_service(&svc, None);
        }

        let via_grant = get_reach_preset_impl(Some(grant.clone()), &state).unwrap();
        assert_eq!(via_grant.preset, "near");
        assert_eq!(via_grant.own_list_max_hops, 1);
        assert_eq!(via_grant.share_min_level, 2);

        let reuse_err = get_reach_preset_impl(Some(grant), &state).unwrap_err();
        assert_eq!(reuse_err.code, "error.auth.sessionTimeout");
    }

    #[test]
    fn test_patch_ui_preferences_wake_flags_including_wifi_unlock() {
        let (dir_guard, state, pw) = setup_s11_settings_fixture("s11_f3_ui_wake");
        assert!(dir_guard.0.exists());

        reset_wake(&state);
        patch_ui_preferences_impl(
            UiPreferencesPatch {
                market_sync_enabled: Some(true),
                ..Default::default()
            },
            Some(pw.clone()),
            &state,
        )
        .unwrap();
        let wake_market = state.sync_control_snapshot().pending_wake;
        assert!(wake_market.market);
        assert!(!wake_market.seal && !wake_market.sync_backup && !wake_market.post);

        reset_wake(&state);
        patch_ui_preferences_impl(
            UiPreferencesPatch {
                wot_share_via_sync: Some(true),
                ..Default::default()
            },
            Some(pw.clone()),
            &state,
        )
        .unwrap();
        let wake_wot = state.sync_control_snapshot().pending_wake;
        assert!(wake_wot.market);
        assert!(!wake_wot.seal && !wake_wot.sync_backup && !wake_wot.post);

        reset_wake(&state);
        patch_ui_preferences_impl(
            UiPreferencesPatch {
                post_accept_from_contacts: Some(true),
                ..Default::default()
            },
            Some(pw.clone()),
            &state,
        )
        .unwrap();
        let wake_post = state.sync_control_snapshot().pending_wake;
        assert!(wake_post.post);
        assert!(!wake_post.seal && !wake_post.sync_backup && !wake_post.market);

        reset_wake(&state);
        patch_ui_preferences_impl(
            UiPreferencesPatch {
                market_poll_wifi_only: Some(true),
                ..Default::default()
            },
            Some(pw.clone()),
            &state,
        )
        .unwrap();
        assert!(state.sync_control_snapshot().pending_wake.is_empty());

        reset_wake(&state);
        patch_ui_preferences_impl(
            UiPreferencesPatch {
                market_poll_wifi_only: Some(false),
                ..Default::default()
            },
            Some(pw),
            &state,
        )
        .unwrap();
        let wake_wifi = state.sync_control_snapshot().pending_wake;
        assert!(wake_wifi.seal);
        assert!(wake_wifi.sync_backup);
        assert!(wake_wifi.market);
        assert!(wake_wifi.post);
    }

    #[test]
    fn test_patch_security_policy_wake_flags_and_locked_session_suppression() {
        let (dir_guard, state, pw) = setup_s11_settings_fixture("s11_f3_sec_wake");
        assert!(dir_guard.0.exists());
        state.service.lock().unwrap().unlock_session(&pw, 600).unwrap();

        reset_wake(&state);
        patch_security_policy_impl(
            SecurityPolicyPatch {
                sync_backup_enabled: Some(true),
                ..Default::default()
            },
            Some(pw.clone()),
            &state,
        )
        .unwrap();
        let wake_sync_backup = state.sync_control_snapshot().pending_wake;
        assert!(wake_sync_backup.sync_backup);
        assert!(!wake_sync_backup.local_backup);

        reset_wake(&state);
        patch_security_policy_impl(
            SecurityPolicyPatch {
                auto_backup_enabled: Some(true),
                ..Default::default()
            },
            Some(pw.clone()),
            &state,
        )
        .unwrap();
        let wake_auto = state.sync_control_snapshot().pending_wake;
        assert!(wake_auto.local_backup);
        assert!(!wake_auto.sync_backup);

        reset_wake(&state);
        patch_security_policy_impl(
            SecurityPolicyPatch {
                backup_sync_folder: Some(Some("/tmp/s11-new-backup".to_string())),
                ..Default::default()
            },
            Some(pw.clone()),
            &state,
        )
        .unwrap();
        assert!(state.sync_control_snapshot().pending_wake.local_backup);

        reset_wake(&state);
        patch_security_policy_impl(
            SecurityPolicyPatch {
                backup_sync_folder: Some(Some("/tmp/s11-new-backup".to_string())),
                ..Default::default()
            },
            Some(pw.clone()),
            &state,
        )
        .unwrap();
        assert!(state.sync_control_snapshot().pending_wake.is_empty());

        reset_wake(&state);
        state.service.lock().unwrap().lock_session();
        patch_security_policy_impl(
            SecurityPolicyPatch {
                backup_sync_folder: Some(Some("/tmp/s11-locked-backup".to_string())),
                ..Default::default()
            },
            Some(pw),
            &state,
        )
        .unwrap();
        assert!(state.sync_control_snapshot().pending_wake.is_empty());
    }

    #[test]
    fn s11_q3_shortening_nonzero_session_timeout_takes_effect_immediately() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("s11_q3_shorten_timeout"));
        let pw = "MasterPass-Timeout-2026!";
        let state = AppState::new(crate::test_support::setup_test_app(
            &dir_guard.0.join("wallet"),
            "Timeout Tester",
            crate::test_support::ALICE_PHRASE,
            pw,
        ));

        {
            let mut svc = state.service.lock().unwrap();
            let mut sec = state
                .get_or_load_security_policy(&mut svc, Some(pw))
                .unwrap();
            sec.session_timeout_seconds = 3600;
            state.save_security_policy(&mut svc, sec, Some(pw)).unwrap();
            svc.unlock_session(pw, 3600).unwrap();
            assert!(svc.is_session_active());
        }

        // 1. Lengthening or keeping equal timeout without password leaves active session untouched
        let unchanged = patch_security_policy_impl(
            SecurityPolicyPatch {
                session_timeout_seconds: Some(3600),
                ..Default::default()
            },
            None,
            &state,
        )
        .unwrap();
        assert_eq!(unchanged.session_timeout_seconds, 3600);
        assert!(
            state.service.lock().unwrap().is_session_active(),
            "Equal non-zero timeout without password must keep active session open"
        );

        let longer = patch_security_policy_impl(
            SecurityPolicyPatch {
                session_timeout_seconds: Some(7200),
                ..Default::default()
            },
            None,
            &state,
        )
        .unwrap();
        assert_eq!(longer.session_timeout_seconds, 7200);
        assert!(state.service.lock().unwrap().is_session_active());

        // 2. Shortening timeout WITH master password re-unlocks under shorter duration (stays active)
        let shorter_with_pw = patch_security_policy_impl(
            SecurityPolicyPatch {
                session_timeout_seconds: Some(1800),
                ..Default::default()
            },
            Some(pw.to_string()),
            &state,
        )
        .unwrap();
        assert_eq!(shorter_with_pw.session_timeout_seconds, 1800);
        assert!(state.service.lock().unwrap().is_session_active());

        // 3. Shortening timeout WITHOUT password immediately locks the active session
        let shorter_without_pw = patch_security_policy_impl(
            SecurityPolicyPatch {
                session_timeout_seconds: Some(60),
                ..Default::default()
            },
            None,
            &state,
        )
        .unwrap();
        assert_eq!(shorter_without_pw.session_timeout_seconds, 60);
        assert!(
            !state.service.lock().unwrap().is_session_active(),
            "Shortening timeout without password must lock session immediately"
        );
    }
}
