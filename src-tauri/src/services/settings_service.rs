use log::{error, info};

use crate::{
    models::{FrontendError, ReachPresetDto},
    services::market_service::{get_own_card_max_hops_inner, set_own_card_max_hops_inner},
    settings::AppSettings,
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
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let media_auto_load_max_hops = settings.media_auto_load_max_hops.min(3);
    let share_min_level = settings.share_min_level.min(3);

    let (own_list_max_hops, card_max_hops) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let reg = state
            .get_or_load_wot_register(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        let card_hops = get_own_card_max_hops_inner(&mut service, password.as_deref());
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
    let (own_list_max_hops, share_min_level, card_max_hops, media_auto_load_max_hops) =
        preset_values(&preset)?;

    // Called before locking state.service because set_wot_max_hops_impl acquires it internally.
    crate::services::wot_service::set_wot_max_hops_impl(
        own_list_max_hops,
        password.clone(),
        state,
    )?;

    {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        set_own_card_max_hops_inner(&mut service, card_max_hops, password.as_deref())?;

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
    state.get_cached_settings().map_err(FrontendError::from)
}

pub fn save_app_settings_impl(
    mut settings: AppSettings,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    settings.media_auto_load_max_hops = settings.media_auto_load_max_hops.min(3);
    settings.share_min_level = settings.share_min_level.min(3);
    info!("Saving app settings to disk...");
    let mut service = state.service.lock().unwrap();
    crate::services::common::ensure_access_mode(
        &mut service,
        human_money_core::AccessMode::DailyPay,
        password.as_deref(),
    )?;
    state
        .save_settings(&mut service, settings, password.as_deref())
        .map_err(FrontendError::from)
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
    let mut service = state.service.lock().unwrap();

    if let Some(ref pwd) = password {
        if let Err(e) = service.unlock_session(pwd, 300) {
            error!("Failed to unlock session for local override: {}", e);
        }
    }

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
}
