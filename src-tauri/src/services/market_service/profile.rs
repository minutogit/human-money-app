use super::card_signing::*;
use super::storage::*;
use crate::models::{
    FieldAudience, FrontendError, FrontendMyMarketProfile, FrontendSaveMyMarketProfileRequest,
    StoredOwnMarketProfile,
};
use crate::state::AppState;
use human_money_core::app_service::AppService;

pub(super) fn build_my_market_profile_dto(
    service: &AppService,
    stored: StoredOwnMarketProfile,
    now: u64,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let core_profile = service
        .with_wallet(|w| w.profile.to_public_profile())
        .map_err(FrontendError::from)?;

    let media_used_count = stored
        .ads
        .iter()
        .filter(|a| a.is_published_active(now) && a.ad.image_token.is_some())
        .count();

    let ads = stored
        .ads
        .iter()
        .map(|a| crate::models::FrontendMyAd::from_own_ad(a, now))
        .collect();

    let (share_uri, file_wire, qr_svg, qr_is_partial) = if stored.published {
        if let Some(ref card) = stored.last_card {
            match build_share_artifacts(card) {
                Ok(art) => (
                    Some(art.share_uri),
                    Some(art.file_wire),
                    art.qr_svg,
                    art.qr_is_partial,
                ),
                Err(_) => (None, None, None, false),
            }
        } else {
            (None, None, None, false)
        }
    } else {
        (None, None, None, false)
    };

    let frontend_profile: crate::models::FrontendUserProfile = core_profile.into();

    Ok(FrontendMyMarketProfile {
        profile: frontend_profile,
        visibility: stored.visibility,
        skills: stored.skills,
        ads,
        max_hops: stored.max_hops,
        validity_years: stored.validity_years.clamp(1, 3),
        allow_inquiries: stored.allow_inquiries,
        published: stored.published,
        share_uri,
        file_wire,
        qr_svg,
        qr_is_partial,
        media_max_bytes: humoco_market::MAX_AD_MEDIA_BYTES,
        media_max_count: humoco_market::MAX_ADS_PER_CARD,
        media_used_count,
    })
}

pub fn get_my_market_profile_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let stored = load_own_market_profile_state(state, &mut service, pw)?;
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    build_my_market_profile_dto(&service, stored, now)
}

pub(super) fn coarsen_public_coordinates(raw: &str) -> String {
    let parts: Vec<&str> = raw.split(',').collect();
    if parts.len() == 2 {
        if let (Ok(lat), Ok(lon)) = (
            parts[0].trim().parse::<f64>(),
            parts[1].trim().parse::<f64>(),
        ) {
            if lat.is_finite() && lon.is_finite() {
                return format!(
                    "{:.2}, {:.2}",
                    (lat * 100.0).round() / 100.0,
                    (lon * 100.0).round() / 100.0
                );
            }
        }
    }
    raw.to_string()
}

pub fn save_my_market_profile_impl(
    request: FrontendSaveMyMarketProfileRequest,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let sanitized_picture_url = match request.profile.picture_url.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(pic_trimmed) => {
            if pic_trimmed.chars().any(char::is_control) {
                return Err(FrontendError::from(
                    "Profile picture URL must not contain control characters".to_string(),
                ));
            }
            if pic_trimmed
                .get(..5)
                .is_some_and(|p| p.eq_ignore_ascii_case("data:"))
            {
                humoco_market::validate_avatar_data_uri(pic_trimmed)
                    .map_err(|e| FrontendError::from(e.to_string()))?;
            } else {
                if pic_trimmed.len() > 200 || pic_trimmed.chars().any(char::is_whitespace) {
                    return Err(FrontendError::from(
                        "Profile picture URL must be <= 200 chars without whitespace".to_string(),
                    ));
                }
                let parsed = reqwest::Url::parse(pic_trimmed).map_err(|e| {
                    FrontendError::from(format!("Invalid profile picture URL: {}", e))
                })?;
                if parsed.scheme() != "https"
                    || parsed.host_str().is_none()
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                {
                    return Err(FrontendError::from(
                        "Profile picture URL must use https:// without userinfo".to_string(),
                    ));
                }
            }
            Some(pic_trimmed.to_string())
        }
    };

    if request.skills.len() > humoco_market::MAX_SKILLS_PER_CARD {
        return Err(FrontendError::from(format!(
            "Skill count {} exceeds maximum {}",
            request.skills.len(),
            humoco_market::MAX_SKILLS_PER_CARD
        )));
    }
    for skill in &request.skills {
        let title_count = skill.title.trim().chars().count();
        if title_count == 0 || skill.title.chars().count() > humoco_market::MAX_SKILL_TITLE_CHARS {
            return Err(FrontendError::from(format!(
                "Skill title length must be between 1 and {}",
                humoco_market::MAX_SKILL_TITLE_CHARS
            )));
        }
        if let Some(ref text) = skill.text {
            let text_count = text.chars().count();
            if text_count > humoco_market::MAX_SKILL_TEXT_CHARS {
                return Err(FrontendError::from(format!(
                    "Skill text length {} exceeds maximum {}",
                    text_count,
                    humoco_market::MAX_SKILL_TEXT_CHARS
                )));
            }
        }
        if let Some(ref pricing) = skill.pricing {
            let pricing_count = pricing.chars().count();
            if pricing_count > humoco_market::MAX_PRICING_CHARS {
                return Err(FrontendError::from(format!(
                    "Skill pricing length {} exceeds maximum {}",
                    pricing_count,
                    humoco_market::MAX_PRICING_CHARS
                )));
            }
        }
    }

    let mut service =
        state.authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;

    let mut core_profile = service
        .with_wallet(|w| w.profile.to_public_profile())
        .map_err(FrontendError::from)?;

    // Hard guardrails: street address must never be public; public coordinates are coarsened to ~1 km (2 decimals).
    let mut sanitized_visibility = request.visibility;
    if sanitized_visibility.street == FieldAudience::Everyone {
        sanitized_visibility.street = FieldAudience::Contacts;
    }

    core_profile.first_name = request.profile.first_name;
    core_profile.last_name = request.profile.last_name;
    core_profile.organization = request.profile.organization;
    core_profile.community = request.profile.community;
    core_profile.address = request.profile.address.map(Into::into);
    core_profile.gender = request.profile.gender;
    core_profile.email = request.profile.email;
    core_profile.phone = request.profile.phone;
    core_profile.coordinates = if sanitized_visibility.coordinates == FieldAudience::Everyone {
        request
            .profile
            .coordinates
            .map(|c| coarsen_public_coordinates(&c))
    } else {
        request.profile.coordinates
    };
    core_profile.url = request.profile.url;
    core_profile.picture_url = sanitized_picture_url;

    // Maintainer Decision 7: service_offer is derived from joined skill titles; needs is preserved!
    let service_offer = request
        .skills
        .iter()
        .map(|s| s.title.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    core_profile.service_offer = if service_offer.is_empty() {
        None
    } else {
        Some(service_offer)
    };

    // Build and sign the draft Market profile BEFORE committing Core profile so validation
    // errors fail cleanly and both stores commit inside one WalletTransaction.
    let mut stored = load_own_market_profile_for_write(&mut service, password.as_deref())?;

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let new_max_hops = request.max_hops.clamp(1, 3);
    update_shrink_window(&mut stored, new_max_hops, now);
    stored.visibility = sanitized_visibility;
    stored.skills = request.skills;
    stored.max_hops = new_max_hops;
    stored.validity_years = request.validity_years.clamp(1, 3);
    stored.allow_inquiries = request.allow_inquiries;
    for entry in &mut stored.ads {
        entry.ad.allow_inquiries = request.allow_inquiries;
    }

    if let Some(pub_flag) = request.published {
        if pub_flag {
            stored.published = true;
        } else if stored.published {
            stored.published = false;
            sign_unpublish_tombstone_card(&mut service, &mut stored, now)?;
        }
    }

    if stored.published {
        let drops = load_drops_from_settings(state, &mut service, None, password.as_deref())?;
        rebuild_and_sign_own_card_with_drops_and_profile(
            &mut service,
            &mut stored,
            now,
            Some(drops),
            Some(&core_profile),
        )?;
    }

    let stored_bytes = serde_json::to_vec(&stored).map_err(|e| {
        FrontendError::from(format!("Failed to serialize own market profile: {}", e))
    })?;
    service
        .update_public_profile_with_companions(
            core_profile,
            vec![(
                crate::state::MARKET_OWN_PROFILE_KEY.to_string(),
                stored_bytes,
            )],
            password.as_deref(),
        )
        .map_err(|e| {
            FrontendError::from(format!("Failed to update Core & market profile: {}", e))
        })?;
    state.set_cached_own_market_profile_for_service(&service, Some(stored.clone()));

    build_my_market_profile_dto(&service, stored, now)
}

pub async fn set_card_published_impl(
    published: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut stored = load_own_market_profile_for_write(&mut service, pw)?;

    if published {
        stored.published = true;
        let drops = load_drops_from_settings(state, &mut service, None, pw)?;
        rebuild_and_sign_own_card_with_drops(&mut service, &mut stored, now, Some(drops))?;
    } else if stored.published {
        stored.published = false;
        sign_unpublish_tombstone_card(&mut service, &mut stored, now)?;
    }

    save_own_market_profile_state(state, &mut service, &stored, pw)?;
    build_my_market_profile_dto(&service, stored, now)
}
