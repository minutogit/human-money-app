use log::info;

use crate::{
    models::{FrontendError, ReachPresetDto},
    services::market_service::{get_own_card_max_hops_inner, set_own_card_max_hops_inner},
    settings::AppSettings,
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
    let settings = {
        let _service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
        state.get_cached_settings().map_err(FrontendError::from)?
    };
    let media_auto_load_max_hops = settings.media_auto_load_max_hops.min(3);
    let share_min_level = settings.share_min_level.min(3);

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

        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        settings.share_min_level = share_min_level;
        settings.media_auto_load_max_hops = media_auto_load_max_hops;
        state
            .save_settings(&mut service, settings, password.as_deref())
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

pub fn get_app_settings_impl(state: &AppState) -> Result<AppSettings, FrontendError> {
    info!("Getting app settings from cache...");
    state.authorize(RequiredAccess::Unlocked, None)?;
    state.get_cached_settings().map_err(FrontendError::from)
}

fn is_security_setting_changed(next: &AppSettings, existing: &AppSettings) -> bool {
    next.session_timeout_seconds != existing.session_timeout_seconds
        || next.auto_backup_enabled != existing.auto_backup_enabled
        || next.backup_sync_folder != existing.backup_sync_folder
        || next.backup_protection_mode != existing.backup_protection_mode
        || next.sync_backup_enabled != existing.sync_backup_enabled
        || next.dev_mode_unlocked != existing.dev_mode_unlocked
        || next.own_nominated_lock_server != existing.own_nominated_lock_server
        || next.l2_anchor_templates_meta != existing.l2_anchor_templates_meta
        || next.public_syncs != existing.public_syncs
        || next.market_selected_syncs != existing.market_selected_syncs
}

pub fn save_app_settings_impl(
    mut settings: AppSettings,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    settings.media_auto_load_max_hops = settings.media_auto_load_max_hops.min(3);
    settings.share_min_level = settings.share_min_level.min(3);
    info!("Saving app settings to disk...");
    let mut service = state.authorize_and_lock(RequiredAccess::DailyPay, password.as_deref())?;
    let existing = match state.get_cached_settings() {
        Ok(cached) => cached,
        Err(_) => state
            .load_settings(&mut service, password.as_deref())
            .map_err(FrontendError::from)?,
    };
    if existing.pending_restore_l2_reconciliation {
        settings.pending_restore_l2_reconciliation = true;
    }
    if is_security_setting_changed(&settings, &existing) {
        service
            .require_access_mode(human_money_core::AccessMode::Full, password.as_deref())
            .map_err(FrontendError::from)?;
    }
    let timeout_zero = settings.session_timeout_seconds == 0;
    state
        .save_settings(&mut service, settings, password.as_deref())
        .map_err(FrontendError::from)?;
    if timeout_zero {
        service.lock_session();
    }
    Ok(())
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
    fn test_settings_security_field_guard_timeout_zero_and_restore_flag_preservation() {
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

        let base = AppSettings {
            session_timeout_seconds: 300,
            pending_restore_l2_reconciliation: true,
            ..Default::default()
        };
        *state.settings.lock().unwrap() = Some(base.clone());
        {
            let mut svc = state.service.lock().unwrap();
            crate::state::save_encrypted_json(
                &mut svc,
                crate::settings::SETTINGS_KEY,
                &base,
                Some(master_pw),
            )
            .unwrap();
        }

        // 1. DailyPay can save pure UI preference while keeping pending_restore_l2_reconciliation = true
        let mut ui_only = base.clone();
        ui_only.ui_mode = "expert".to_string();
        ui_only.pending_restore_l2_reconciliation = false;
        save_app_settings_impl(ui_only, None, &state).unwrap();
        let saved_ui = get_app_settings_impl(&state).unwrap();
        assert_eq!(saved_ui.ui_mode, "expert");
        assert!(saved_ui.pending_restore_l2_reconciliation);

        // 2. DailyPay is rejected when changing security settings without master password
        for mutate_security in [
            |s: &mut AppSettings| s.auto_backup_enabled = false,
            |s: &mut AppSettings| s.session_timeout_seconds = 60,
            |s: &mut AppSettings| s.dev_mode_unlocked = true,
            |s: &mut AppSettings| s.backup_sync_folder = Some("/tmp/backup".to_string()),
        ] {
            let mut attempt = saved_ui.clone();
            mutate_security(&mut attempt);
            let err = save_app_settings_impl(attempt, None, &state).unwrap_err();
            assert_eq!(err.code, "error.auth.insufficientAccessMode");
        }

        // 3. Security change succeeds with valid master password override
        let mut with_override = saved_ui.clone();
        with_override.auto_backup_enabled = false;
        save_app_settings_impl(with_override.clone(), Some(master_pw.to_string()), &state)
            .unwrap();
        assert!(!get_app_settings_impl(&state).unwrap().auto_backup_enabled);

        // 4. Saving session_timeout_seconds == 0 immediately clears active session cache & demotes timed elevation
        crate::commands::auth::elevate_session_with_password_impl(master_pw, &state).unwrap();
        assert!(state.service.lock().unwrap().is_session_active());
        let mut timeout_zero = get_app_settings_impl(&state).unwrap();
        timeout_zero.session_timeout_seconds = 0;
        save_app_settings_impl(timeout_zero, Some(master_pw.to_string()), &state).unwrap();

        let status_after = crate::commands::auth::get_session_status_impl(&state).unwrap();
        assert!(!status_after.has_cached_session);
        assert_eq!(status_after.access_mode, "dailyPay");
        assert_eq!(status_after.base_access_mode, "dailyPay");
    }
}
