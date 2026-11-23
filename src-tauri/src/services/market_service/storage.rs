use super::card_signing::*;
use crate::models::{FrontendError, FrontendSyncDescriptor, StoredOwnMarketProfile};
use crate::state::{AppState, MARKET_OWN_PROFILE_KEY};
use human_money_core::app_service::AppService;

pub(crate) fn load_drops_from_settings(
    state: &AppState,
    service: &mut AppService,
    selected_override: Option<&[FrontendSyncDescriptor]>,
    password: Option<&str>,
) -> Result<Vec<humoco_market::DropDescriptor>, FrontendError> {
    let security_policy = state
        .get_or_load_security_policy(service, password)
        .map_err(FrontendError::from)?;
    let my_prefix = service
        .with_wallet_and_identity(|_w, id| {
            human_money_core::crypto::get_prefix_from_user_id(&id.user_id).map(|s| s.to_string())
        })
        .ok()
        .flatten();
    let selected = selected_override.unwrap_or(&security_policy.market_selected_syncs);
    Ok(build_card_drops(
        selected,
        &security_policy.public_syncs,
        my_prefix.as_deref(),
    ))
}

pub(crate) fn load_own_market_profile_state(
    state: &AppState,
    service: &mut AppService,
    password: Option<&str>,
) -> Result<StoredOwnMarketProfile, FrontendError> {
    match crate::state::load_encrypted_json_or_default::<StoredOwnMarketProfile>(
        service,
        MARKET_OWN_PROFILE_KEY,
        password,
    ) {
        Ok(stored) => {
            state.set_cached_own_market_profile_for_service(service, Some(stored.clone()));
            Ok(stored)
        }
        Err(
            human_money_core::app_service::AppFacadeError::SessionNotActive(_)
            | human_money_core::app_service::AppFacadeError::SessionExpired(_)
            | human_money_core::app_service::AppFacadeError::App(
                human_money_core::error::AppError::SessionNotActive { .. }
                | human_money_core::error::AppError::SessionExpired { .. },
            ),
        ) if password.is_none() => {
            if let Some(cached) = state.cache_own_market_profile() {
                return Ok(cached);
            }
            Err(FrontendError::from(
                human_money_core::app_service::AppFacadeError::App(
                    human_money_core::error::AppError::SessionNotActive {
                        reason: "Password required. Please use 'unlock_session'.".to_string(),
                    },
                ),
            ))
        }
        Err(e) => Err(FrontendError::from(e)),
    }
}

pub(super) fn load_own_market_profile_for_write(
    service: &mut AppService,
    password: Option<&str>,
) -> Result<StoredOwnMarketProfile, FrontendError> {
    crate::state::load_encrypted_json_or_default::<StoredOwnMarketProfile>(
        service,
        MARKET_OWN_PROFILE_KEY,
        password,
    )
    .map_err(FrontendError::from)
}

pub(super) fn save_own_market_profile_state(
    state: &AppState,
    service: &mut AppService,
    stored: &StoredOwnMarketProfile,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, MARKET_OWN_PROFILE_KEY, stored, password)
        .map_err(FrontendError::from)?;
    state.set_cached_own_market_profile_for_service(service, Some(stored.clone()));
    Ok(())
}

pub(super) fn cleanup_unused_own_media(
    service: &mut AppService,
    ads: &[crate::models::StoredOwnAd],
    old_token: &str,
    password: Option<&str>,
) {
    let is_used = ads
        .iter()
        .any(|a| a.ad.image_token.as_deref() == Some(old_token));
    if !is_used {
        let _ = crate::state::save_encrypted_bytes(
            service,
            &crate::state::own_media_storage_key(old_token),
            &[],
            password,
        );
    }
}

pub(super) fn encode_media_data_uri(media_bytes: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let prefix = humoco_market::detect_media_data_uri_prefix(media_bytes);
    format!("{}{}", prefix, STANDARD.encode(media_bytes))
}

pub(super) fn try_load_cached_media_bytes(
    service: &mut AppService,
    storage_key: &str,
    is_local_sidecar: bool,
    password: Option<&str>,
) -> Result<Option<Vec<u8>>, FrontendError> {
    let media_bytes_opt = if is_local_sidecar {
        crate::state::load_local_sidecar_bytes_opt(service, storage_key, password)
    } else {
        crate::state::load_encrypted_bytes_opt(service, storage_key, password)
    }
    .map_err(FrontendError::from)?;
    let Some(media_bytes) = media_bytes_opt else {
        return Ok(None);
    };
    if media_bytes.is_empty() || humoco_market::validate_ad_media(&media_bytes).is_err() {
        return Ok(None);
    }
    Ok(Some(media_bytes))
}

pub(crate) fn get_own_card_max_hops_inner(
    state: &AppState,
    service: &mut AppService,
    password: Option<&str>,
) -> Result<u8, FrontendError> {
    load_own_market_profile_state(state, service, password).map(|s| s.max_hops.clamp(1, 3))
}

pub(crate) fn update_shrink_window(
    stored: &mut StoredOwnMarketProfile,
    new_max_hops: u8,
    now: u64,
) {
    const SHRINK_PROPAGATION_SECS: u64 = 30 * 86_400;
    let prev_signed = stored
        .last_card
        .as_ref()
        .map(|c| c.payload.max_hops.clamp(1, 3))
        .unwrap_or(0);
    let active_prev = if now < stored.shrink_until {
        prev_signed.max(stored.shrink_hops)
    } else {
        prev_signed
    };
    if active_prev > new_max_hops {
        stored.shrink_hops = active_prev;
        stored.shrink_until = now.saturating_add(SHRINK_PROPAGATION_SECS);
    } else {
        stored.shrink_hops = 0;
        stored.shrink_until = 0;
    }
}
