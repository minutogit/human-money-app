use log::info;

use crate::{
    models::{FrontendError, ReachPresetDto},
    services::market_service::{get_own_card_max_hops_inner, set_own_card_max_hops_inner},
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
    let prefs = {
        let _service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
        state
            .get_cached_ui_preferences()
            .map_err(FrontendError::from)?
    };
    let media_auto_load_max_hops = prefs.media_auto_load_max_hops.min(3);
    let share_min_level = prefs.share_min_level.min(3);

    let (own_list_max_hops, card_max_hops) = {
        let mut service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
        let reg = state
            .get_or_load_wot_register(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        let card_hops = get_own_card_max_hops_inner(state, &mut service, password.as_deref());
        (reg.policy.own_list_max_hops.clamp(1, 3), card_hops)
    };

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
    state.authorize(RequiredAccess::DailyPay, password.as_deref())?;
    let (own_list_max_hops, share_min_level, card_max_hops, media_auto_load_max_hops) =
        preset_values(&preset)?;

    // Called before locking state.service because set_wot_max_hops_impl acquires it internally.
    crate::services::wot_service::set_wot_max_hops_impl(
        own_list_max_hops,
        password.clone(),
        state,
    )?;

    {
        let mut service = state.authorize_and_lock(RequiredAccess::DailyPay, password.as_deref())?;
        set_own_card_max_hops_inner(state, &mut service, card_max_hops, password.as_deref())?;

        state
            .mutate_ui_preferences(&mut service, password.as_deref(), |prefs| {
                prefs.share_min_level = share_min_level.min(3);
                prefs.media_auto_load_max_hops = media_auto_load_max_hops.min(3);
            })
            .map_err(FrontendError::from)?;
    }

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
    let mut service = state.authorize_and_lock(RequiredAccess::DailyPay, password.as_deref())?;
    state
        .mutate_ui_preferences(&mut service, password.as_deref(), |prefs| {
            patch.apply_to(prefs);
        })
        .map_err(FrontendError::from)
}

pub fn patch_security_policy_impl(
    patch: SecurityPolicyPatch,
    password: Option<String>,
    state: &AppState,
) -> Result<SecurityPolicy, FrontendError> {
    info!("Patching SecurityPolicy...");
    let mut service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;
    let updated = state
        .mutate_security_policy(&mut service, password.as_deref(), |policy| {
            patch.apply_to(policy);
        })
        .map_err(FrontendError::from)?;
    if updated.session_timeout_seconds == 0 {
        service.lock_session();
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
    let mut service = state.authorize_and_lock(RequiredAccess::DailyPay, password.as_deref())?;

    service
        .set_conflict_local_override(&proof_id, value, note, password.as_deref())
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
        // Activate timed PIN session
        crate::commands::auth::unlock_session_with_pin_impl(
            &folder,
            "246810",
            300,
            &keystore_dir,
            "dev-settings-1",
            &state,
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
        {
            let mut svc = state.service.lock().unwrap();
            crate::state::save_encrypted_json(&mut svc, UI_PREFERENCES_KEY, &ui_base, Some(master_pw))
                .unwrap();
            crate::state::save_encrypted_json(&mut svc, SECURITY_POLICY_KEY, &sec_base, Some(master_pw))
                .unwrap();
            crate::state::save_encrypted_json(
                &mut svc,
                INTERNAL_LIFECYCLE_STATE_KEY,
                &lc_base,
                Some(master_pw),
            )
            .unwrap();
        }
        *state.ui_preferences.lock().unwrap() = Some(ui_base);
        *state.security_policy.lock().unwrap() = Some(sec_base);
        *state.internal_lifecycle.lock().unwrap() = Some(lc_base);

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
        assert!(get_app_config_impl(&state)
            .unwrap()
            .lifecycle
            .pending_restore_l2_reconciliation);

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
}
