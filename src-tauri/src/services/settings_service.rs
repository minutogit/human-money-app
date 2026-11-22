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

    let wot_bytes = serde_json::to_vec(&reg)
        .map_err(|e| FrontendError::from(format!("Failed to serialize WoT register: {}", e)))?;
    let own_bytes = serde_json::to_vec(&stored).map_err(|e| {
        FrontendError::from(format!("Failed to serialize own market profile: {}", e))
    })?;
    let ui_bytes = serde_json::to_vec(&prefs)
        .map_err(|e| FrontendError::from(format!("Failed to serialize UiPreferences: {}", e)))?;

    service
        .save_encrypted_batch(
            &[
                (crate::state::WOT_DATA_NAME, wot_bytes.as_slice()),
                (crate::state::MARKET_OWN_PROFILE_KEY, own_bytes.as_slice()),
                (crate::settings::UI_PREFERENCES_KEY, ui_bytes.as_slice()),
            ],
            pw,
        )
        .map_err(|e| FrontendError::from(format!("Failed to persist reach preset: {}", e)))?;

    state.set_cached_wot_for_service(&service, Some(reg.clone()));
    state.set_cached_own_market_profile_for_service(&service, Some(stored));
    state.set_cached_ui_preferences_for_service(&service, Some(prefs));
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
    use crate::settings::{
        InternalLifecycleState, INTERNAL_LIFECYCLE_STATE_KEY, SECURITY_POLICY_KEY,
        UI_PREFERENCES_KEY,
    };

    #[test]
    fn test_reach_preset_values_and_detection() {
        assert_eq!(preset_values("near").unwrap(), (1, 2, 1, 1));
        assert_eq!(preset_values("medium").unwrap(), (2, 1, 2, 2));
        assert_eq!(preset_values("far").unwrap(), (3, 1, 3, 2));
        assert!(preset_values("ultra").is_err());

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
    fn test_split_patch_role_isolation_timeout_zero_double_option_and_lifecycle_intact() {
        let dir = crate::test_support::temp_test_dir("settings_guard_test");
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
        // Activate timed PIN session
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

        // 1. DailyPay can patch UiPreferences (clamping works) while InternalLifecycleState stays intact
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
        assert!(cfg.lifecycle.pending_restore_l2_reconciliation);

        // 2. DailyPay is rejected when patching SecurityPolicy without master password
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

        // 3. Full access succeeds; omitted backup_sync_folder preserves existing, explicit Some(None) clears it
        let sec_after_omit = patch_security_policy_impl(
            SecurityPolicyPatch {
                auto_backup_enabled: Some(false),
                ..Default::default()
            },
            Some(master_pw.to_string()),
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
            Some(master_pw.to_string()),
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

        // 4. Saving session_timeout_seconds == 0 immediately clears active session cache & demotes timed elevation
        crate::commands::auth::elevate_session_with_password_impl(master_pw, &state).unwrap();
        assert!(state.service.lock().unwrap().is_session_active());
        let sec_zero = patch_security_policy_impl(
            SecurityPolicyPatch {
                session_timeout_seconds: Some(0),
                ..Default::default()
            },
            Some(master_pw.to_string()),
            &state,
        )
        .unwrap();
        assert_eq!(sec_zero.session_timeout_seconds, 0);

        let status_after = crate::commands::auth::get_session_status_impl(&state).unwrap();
        assert!(!status_after.has_cached_session);
        assert_eq!(status_after.access_mode, "dailyPay");
        assert_eq!(status_after.base_access_mode, "dailyPay");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn s11_f3_atomic_reach_preset_rollback_wake_parity_and_one_shot_pin() {
        let dir = crate::test_support::temp_test_dir("s11_f3_settings");
        let pw = "MasterPass-S11-2026!";
        let service = crate::test_support::setup_test_app(
            &dir.join("wallet"),
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

        // 1. Atomic rollback when seal.enc write fails during apply_reach_preset_impl
        let before_preset = get_reach_preset_impl(Some(pw.to_string()), &state).unwrap();
        assert_eq!(before_preset.preset, "medium");

        {
            let _fault_guard = crate::test_support::WRITE_FAULT_TEST_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            human_money_core::storage::file_storage::inject_write_fault_once("seal.enc");
            assert!(
                apply_reach_preset_impl("near".to_string(), Some(pw.to_string()), &state).is_err(),
                "fault must fail apply_reach_preset_impl"
            );
        }

        let after_fault = get_reach_preset_impl(Some(pw.to_string()), &state).unwrap();
        assert_eq!(after_fault, before_preset);
        assert_eq!(
            state.get_cached_ui_preferences().unwrap().share_min_level,
            1
        );
        assert_eq!(state.cache_wot().unwrap().policy.own_list_max_hops, 2);

        // 2. Clean apply updates all 3 stores + caches atomically
        let applied =
            apply_reach_preset_impl("near".to_string(), Some(pw.to_string()), &state).unwrap();
        assert_eq!(applied.preset, "near");
        assert_eq!(applied.own_list_max_hops, 1);
        assert_eq!(applied.card_max_hops, 1);
        assert_eq!(applied.share_min_level, 2);
        assert_eq!(applied.media_auto_load_max_hops, 1);

        // 3. OneShotPinGrant works on cold cache + locked session in get_reach_preset_impl
        let grant = {
            let mut svc = state.service.lock().unwrap();
            svc.lock_session();
            state
                .issue_one_shot_pin_grant(&svc, zeroize::Zeroizing::new(pw.to_string()))
                .unwrap()
        };
        state.set_cached_ui_preferences(None);
        {
            let svc = state.service.lock().unwrap();
            state.set_cached_wot_for_service(&svc, None);
            state.set_cached_own_market_profile_for_service(&svc, None);
        }
        let via_grant = get_reach_preset_impl(Some(grant), &state).unwrap();
        assert_eq!(via_grant.preset, "near");

        // Re-unlock session for coordinator wake parity checks
        {
            let mut svc = state.service.lock().unwrap();
            svc.unlock_session(pw, 600).unwrap();
        }
        state.clear_one_shot_pin_grant();

        let reset_wake = |st: &AppState| {
            let mut guard = st.sync_control.lock().unwrap();
            guard.pending_wake = Default::default();
        };

        reset_wake(&state);
        patch_ui_preferences_impl(
            UiPreferencesPatch {
                market_sync_enabled: Some(true),
                ..Default::default()
            },
            Some(pw.to_string()),
            &state,
        )
        .unwrap();
        assert!(state.sync_control_snapshot().pending_wake.market);

        reset_wake(&state);
        patch_ui_preferences_impl(
            UiPreferencesPatch {
                wot_share_via_sync: Some(true),
                ..Default::default()
            },
            Some(pw.to_string()),
            &state,
        )
        .unwrap();
        assert!(state.sync_control_snapshot().pending_wake.market);

        reset_wake(&state);
        patch_ui_preferences_impl(
            UiPreferencesPatch {
                post_accept_from_contacts: Some(true),
                ..Default::default()
            },
            Some(pw.to_string()),
            &state,
        )
        .unwrap();
        assert!(state.sync_control_snapshot().pending_wake.post);

        reset_wake(&state);
        patch_security_policy_impl(
            SecurityPolicyPatch {
                sync_backup_enabled: Some(true),
                ..Default::default()
            },
            Some(pw.to_string()),
            &state,
        )
        .unwrap();
        assert!(state.sync_control_snapshot().pending_wake.sync_backup);

        reset_wake(&state);
        patch_security_policy_impl(
            SecurityPolicyPatch {
                auto_backup_enabled: Some(true),
                ..Default::default()
            },
            Some(pw.to_string()),
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
            Some(pw.to_string()),
            &state,
        )
        .unwrap();
        assert!(state.sync_control_snapshot().pending_wake.local_backup);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn s11_q3_shortening_nonzero_session_timeout_takes_effect_immediately() {
        let dir = crate::test_support::temp_test_dir("s11_q3_shorten_timeout");
        let pw = "MasterPass-Timeout-2026!";
        let state = AppState::new(crate::test_support::setup_test_app(
            &dir.join("wallet"),
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

        // 1. Lengthening timeout without password leaves active session untouched
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

        let _ = std::fs::remove_dir_all(&dir);
    }
}
