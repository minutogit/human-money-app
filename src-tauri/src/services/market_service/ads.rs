use super::card_signing::*;
use super::profile::*;
use super::storage::*;
use crate::models::{
    FrontendAdInput, FrontendError, FrontendMyMarketProfile, StoredOwnMarketProfile,
};
use crate::state::AppState;
use human_money_core::app_service::AppService;

pub(super) const MAX_LOCAL_DRAFT_ADS: usize = 30;

pub(super) fn compute_event_expiry(
    event_at: u64,
    event_all_day: bool,
    now: u64,
    allow_ongoing_start: bool,
) -> Result<u64, FrontendError> {
    if event_at == 0 {
        return Err(FrontendError::from(
            "Event date must be a valid positive timestamp".to_string(),
        ));
    }
    let max_expiry = now.saturating_add(humoco_market::AD_MAX_DURATION_SECS);
    if event_at > max_expiry {
        return Err(FrontendError::from(
            "Event date cannot be more than 180 days in the future".to_string(),
        ));
    }
    if !event_all_day && !allow_ongoing_start && event_at < now {
        return Err(FrontendError::from(
            "Event start time must be in the future".to_string(),
        ));
    }
    let raw_expiry = if event_all_day {
        // All-day events store noon UTC of the selected calendar date;
        // +18h (06:00 UTC next day) keeps the event active until end-of-day across timezones.
        event_at.saturating_add(64_800)
    } else {
        event_at.saturating_add(6 * 3_600)
    };
    let expires_at = raw_expiry.min(max_expiry);
    if expires_at <= now || event_at > expires_at {
        return Err(FrontendError::from(
            "Event date must not already be in the past".to_string(),
        ));
    }
    Ok(expires_at)
}

fn validate_ad_input(ad_input: &FrontendAdInput) -> Result<(), FrontendError> {
    if ad_input.kind != humoco_market::AdKind::Event
        && !humoco_market::AD_DURATION_CHOICES_DAYS.contains(&ad_input.duration_days)
    {
        return Err(FrontendError::from(
            "duration_days must be 30, 90, or 180".to_string(),
        ));
    }

    let title_trimmed = ad_input.title.trim();
    let title_count = title_trimmed.chars().count();
    if title_count == 0 || title_count > humoco_market::MAX_AD_TITLE_CHARS {
        return Err(FrontendError::from(format!(
            "Ad title length {} must be between 1 and {}",
            title_count,
            humoco_market::MAX_AD_TITLE_CHARS
        )));
    }

    if let Some(ref text) = ad_input.text {
        let text_count = text.chars().count();
        if text_count > humoco_market::MAX_AD_TEXT_CHARS {
            return Err(FrontendError::from(format!(
                "Ad text length {} exceeds maximum {}",
                text_count,
                humoco_market::MAX_AD_TEXT_CHARS
            )));
        }
    }

    if let Some(ref pricing) = ad_input.pricing {
        let pricing_count = pricing.chars().count();
        if pricing_count > humoco_market::MAX_PRICING_CHARS {
            return Err(FrontendError::from(format!(
                "Ad pricing length {} exceeds maximum {}",
                pricing_count,
                humoco_market::MAX_PRICING_CHARS
            )));
        }
    }
    Ok(())
}

fn prepare_ad_media_token(
    service: &mut AppService,
    image_webp_data_uri: &Option<Option<String>>,
    existing_token: Option<String>,
) -> Result<Option<(String, Vec<u8>)>, FrontendError> {
    match image_webp_data_uri {
        Some(None) => Ok(None),
        Some(Some(ref uri)) if uri.trim().is_empty() => Ok(None),
        Some(Some(ref uri)) => {
            let uri_trimmed = uri.trim();
            if uri_trimmed.len() > 140_000 || uri_trimmed.chars().any(char::is_control) {
                return Err(FrontendError::from(
                    "Ad image data URI exceeds maximum length or contains control characters"
                        .to_string(),
                ));
            }
            if !humoco_market::is_supported_media_data_uri(uri_trimmed) {
                return Err(FrontendError::from(format!(
                    "Ad image data URI must start with '{}', '{}', or '{}'",
                    humoco_market::MEDIA_DATA_URI_PREFIX,
                    humoco_market::JPEG_DATA_URI_PREFIX,
                    humoco_market::PNG_DATA_URI_PREFIX
                )));
            }
            let media_bytes = humoco_market::extract_media_bytes(uri_trimmed)
                .map_err(|e| FrontendError::from(e.to_string()))?;

            let raw_id = service
                .with_wallet(|w| w.get_user_id().to_string())
                .map_err(FrontendError::from)?;
            let validated_raw = human_money_core::ValidatedUserId::parse(&raw_id)
                .map_err(|e| FrontendError::from(format!("Failed to parse wallet DID: {}", e)))?;
            let pk = validated_raw.public_key();
            let root_did = human_money_core::crypto::create_user_id(pk, None)
                .map_err(|e| FrontendError::from(format!("Failed to recreate root DID: {}", e)))?;

            let (token, _blob_id, _ciphertext) =
                humoco_market::seal_media_blob(&root_did, &media_bytes)
                    .map_err(|e| FrontendError::from(e.to_string()))?;

            Ok(Some((token, media_bytes)))
        }
        None => Ok(existing_token.map(|t| (t, Vec::new()))),
    }
}

fn resolve_ad_event_expiry(
    ad_input: &FrontendAdInput,
    existing_ad: Option<&humoco_market::Ad>,
    now: u64,
) -> Result<Option<u64>, FrontendError> {
    match ad_input.kind {
        humoco_market::AdKind::Event => {
            let ev_at = ad_input.event_at.ok_or_else(|| {
                FrontendError::from("Event ads require an event date".to_string())
            })?;
            let allow_ongoing_start = existing_ad
                .map(|existing| {
                    existing.kind == humoco_market::AdKind::Event
                        && existing.event_at == Some(ev_at)
                        && existing.event_all_day == ad_input.event_all_day
                })
                .unwrap_or(false);
            Ok(Some(compute_event_expiry(
                ev_at,
                ad_input.event_all_day,
                now,
                allow_ongoing_start,
            )?))
        }
        humoco_market::AdKind::Offer | humoco_market::AdKind::Request => {
            if ad_input.event_at.is_some() || ad_input.event_all_day {
                return Err(FrontendError::from(
                    "Offer/Request ads must not specify an event date".to_string(),
                ));
            }
            Ok(None)
        }
    }
}

fn apply_existing_ad_update(
    stored: &mut StoredOwnMarketProfile,
    idx: usize,
    ad_input: FrontendAdInput,
    computed_event_expiry: Option<u64>,
    new_image_token: Option<String>,
    now: u64,
) -> Result<(bool, Option<String>), FrontendError> {
    let was_expired = stored.ads[idx].ad.expires_at <= now;
    let was_published_active = stored.ads[idx].is_published_active(now);
    let existing_id = stored.ads[idx].ad.id.clone();
    let was_draft = !stored.ads[idx].published;
    let target_published = ad_input.published.unwrap_or(!was_draft);
    let will_be_active = match computed_event_expiry {
        Some(exp) => exp > now,
        None => !was_expired || stored.ads[idx].ad.kind == humoco_market::AdKind::Event,
    };
    if target_published && will_be_active && !was_published_active {
        let other_active = stored
            .ads
            .iter()
            .filter(|a| a.ad.id != existing_id && a.is_published_active(now))
            .count();
        if other_active >= humoco_market::MAX_ADS_PER_CARD {
            return Err(FrontendError::with_code(
                "error.market.adLimitReached",
                "Maximum number of active ads reached.",
            ));
        }
    }
    let draft_count = stored.ads.iter().filter(|a| !a.published).count();
    if !target_published && !was_draft && draft_count >= MAX_LOCAL_DRAFT_ADS {
        return Err(FrontendError::from(format!(
            "Maximum number of {} local draft ads reached.",
            MAX_LOCAL_DRAFT_ADS
        )));
    }

    // Edit existing Offer/Request: preserve published_at, renewed_at, and expires_at.
    // For Events (or kind transitions involving Event), sync expires_at with event_at.
    let title_trimmed = ad_input.title.trim().to_string();
    let entry = &mut stored.ads[idx];
    entry.published = target_published;
    let existing = &mut entry.ad;
    let old_kind = existing.kind;
    let old_token = existing.image_token.clone();
    existing.kind = ad_input.kind;
    existing.title = title_trimmed;
    existing.text = ad_input.text;
    existing.pricing = ad_input.pricing;
    existing.allow_inquiries = ad_input.allow_inquiries;
    existing.is_remote = ad_input.is_remote;
    existing.image_token = new_image_token.clone();

    if let Some(new_event_expiry) = computed_event_expiry {
        existing.event_at = ad_input.event_at;
        existing.event_all_day = ad_input.event_all_day;
        if existing.expires_at != new_event_expiry || old_kind != humoco_market::AdKind::Event {
            existing.expires_at = new_event_expiry;
            existing.renewed_at = Some(now.max(existing.published_at));
        }
    } else {
        existing.event_at = None;
        existing.event_all_day = false;
        if old_kind == humoco_market::AdKind::Event {
            existing.renewed_at = Some(now.max(existing.published_at));
            existing.expires_at = now.saturating_add(ad_input.duration_days.saturating_mul(86_400));
        }
    }

    let now_published_active = entry.is_published_active(now);
    let needs_card_rebuild = was_published_active || now_published_active;
    let old_token_to_clean = old_token.filter(|old_t| Some(old_t) != new_image_token.as_ref());
    Ok((needs_card_rebuild, old_token_to_clean))
}

fn insert_new_ad_entry(
    stored: &mut StoredOwnMarketProfile,
    ad_input: FrontendAdInput,
    computed_event_expiry: Option<u64>,
    new_image_token: Option<String>,
    now: u64,
) -> Result<bool, FrontendError> {
    let publish = ad_input.published.unwrap_or(false);
    if publish {
        let active_count = stored
            .ads
            .iter()
            .filter(|a| a.is_published_active(now))
            .count();
        if active_count >= humoco_market::MAX_ADS_PER_CARD {
            return Err(FrontendError::with_code(
                "error.market.adLimitReached",
                "Maximum number of active ads reached.",
            ));
        }
    } else if stored.ads.iter().filter(|a| !a.published).count() >= MAX_LOCAL_DRAFT_ADS {
        return Err(FrontendError::from(format!(
            "Maximum number of {} local draft ads reached.",
            MAX_LOCAL_DRAFT_ADS
        )));
    }

    if ad_input.id.is_some() {
        return Err(FrontendError::from("Ad not found".to_string()));
    }
    let id = humoco_market::new_ad_id();
    let published_at = now;
    let expires_at = computed_event_expiry
        .unwrap_or_else(|| now.saturating_add(ad_input.duration_days.saturating_mul(86_400)));
    stored.ads.push(crate::models::StoredOwnAd {
        ad: humoco_market::Ad {
            id,
            kind: ad_input.kind,
            title: ad_input.title.trim().to_string(),
            text: ad_input.text,
            pricing: ad_input.pricing,
            published_at,
            renewed_at: None,
            expires_at,
            allow_inquiries: ad_input.allow_inquiries,
            is_remote: ad_input.is_remote,
            image_token: new_image_token,
            event_at: ad_input.event_at,
            event_all_day: ad_input.event_all_day,
        },
        published: publish,
    });
    Ok(publish)
}

pub fn upsert_ad_impl(
    ad_input: FrontendAdInput,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    validate_ad_input(&ad_input)?;

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut stored = load_own_market_profile_for_write(&mut service, pw)?;

    let existing_idx = ad_input
        .id
        .as_ref()
        .and_then(|id| stored.ads.iter().position(|a| &a.ad.id == id));

    let computed_event_expiry =
        resolve_ad_event_expiry(&ad_input, existing_idx.map(|idx| &stored.ads[idx].ad), now)?;

    let existing_token = existing_idx.and_then(|idx| stored.ads[idx].ad.image_token.clone());
    let new_media =
        prepare_ad_media_token(&mut service, &ad_input.image_webp_data_uri, existing_token)?;
    let new_image_token = new_media.as_ref().map(|(t, _)| t.clone());
    let pending_media_blob: Option<(String, Vec<u8>)> = new_media
        .filter(|(_, bytes)| !bytes.is_empty())
        .map(|(t, bytes)| (crate::state::own_media_storage_key(&t), bytes));

    let (needs_card_rebuild, old_token_to_clean) = if let Some(idx) = existing_idx {
        apply_existing_ad_update(
            &mut stored,
            idx,
            ad_input,
            computed_event_expiry,
            new_image_token,
            now,
        )?
    } else {
        let rebuild = insert_new_ad_entry(
            &mut stored,
            ad_input,
            computed_event_expiry,
            new_image_token,
            now,
        )?;
        (rebuild, None)
    };

    if stored.published && needs_card_rebuild {
        rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
    }

    let mut batch = crate::state::EncryptedBatchCommit::new();
    if let Some((media_key, media_bytes)) = pending_media_blob {
        batch.own_media_blob(media_key, media_bytes);
    }
    batch.own_market_profile(stored.clone());
    batch
        .commit(state, &mut service, pw)
        .map_err(FrontendError::from)?;

    if let Some(ref old_t) = old_token_to_clean {
        cleanup_unused_own_media(&mut service, &stored.ads, old_t, pw);
    }

    build_my_market_profile_dto(&service, stored, now)
}

pub fn set_ad_published_impl(
    id: String,
    published: bool,
    also_publish_card: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut stored = load_own_market_profile_for_write(&mut service, pw)?;

    let ad_idx = stored
        .ads
        .iter()
        .position(|a| a.ad.id == id)
        .ok_or_else(|| FrontendError::from("Ad not found".to_string()))?;

    let was_published_active = stored.ads[ad_idx].is_published_active(now);
    let was_draft = !stored.ads[ad_idx].published;
    let needs_card_rebuild;
    let mut card_just_published = false;

    if published {
        if !was_published_active {
            let other_active = stored
                .ads
                .iter()
                .filter(|a| a.ad.id != id && a.is_published_active(now))
                .count();
            if other_active >= humoco_market::MAX_ADS_PER_CARD {
                return Err(FrontendError::with_code(
                    "error.market.adLimitReached",
                    "Maximum number of active ads reached.",
                ));
            }
        }

        let entry = &mut stored.ads[ad_idx];
        let ad = &mut entry.ad;
        if ad.kind == humoco_market::AdKind::Event {
            if ad.expires_at <= now {
                return Err(FrontendError::with_code(
                    "error.market.eventDatePast",
                    "Event date is in the past; edit the event date before publishing.",
                ));
            }
        } else if ad.expires_at <= now {
            ad.renewed_at = Some(now.max(ad.published_at));
            ad.expires_at = now.saturating_add(90 * 86_400);
        } else if was_draft && ad.renewed_at.is_none() {
            let dur = ad
                .expires_at
                .saturating_sub(ad.published_at)
                .clamp(30 * 86_400, humoco_market::AD_MAX_DURATION_SECS);
            ad.published_at = now;
            ad.expires_at = now.saturating_add(dur);
        }

        entry.published = true;
        if also_publish_card.unwrap_or(false) && !stored.published {
            stored.published = true;
            card_just_published = true;
        }
        needs_card_rebuild = !was_published_active || card_just_published;
    } else {
        let draft_count = stored.ads.iter().filter(|a| !a.published).count();
        if !was_draft && draft_count >= MAX_LOCAL_DRAFT_ADS {
            return Err(FrontendError::from(format!(
                "Maximum number of {} local draft ads reached.",
                MAX_LOCAL_DRAFT_ADS
            )));
        }
        stored.ads[ad_idx].published = false;
        needs_card_rebuild = was_published_active;
    }

    if stored.published && needs_card_rebuild {
        if card_just_published {
            let drops = load_drops_from_settings(state, &mut service, None, pw)?;
            rebuild_and_sign_own_card_with_drops(&mut service, &mut stored, now, Some(drops))?;
        } else {
            rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
        }
    }

    save_own_market_profile_state(state, &mut service, &stored, pw)?;
    build_my_market_profile_dto(&service, stored, now)
}

pub fn renew_ad_impl(
    id: String,
    duration_days: u64,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    if !humoco_market::AD_DURATION_CHOICES_DAYS.contains(&duration_days) {
        return Err(FrontendError::from(
            "duration_days must be 30, 90, or 180".to_string(),
        ));
    }

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut stored = load_own_market_profile_for_write(&mut service, pw)?;

    let ad_idx = stored
        .ads
        .iter()
        .position(|a| a.ad.id == id)
        .ok_or_else(|| FrontendError::from("Ad not found".to_string()))?;

    if !stored.ads[ad_idx].published {
        return Err(FrontendError::from(
            "Draft ads cannot be renewed; publish the draft instead.".to_string(),
        ));
    }

    if stored.ads[ad_idx].ad.kind == humoco_market::AdKind::Event {
        return Err(FrontendError::from(
            "Event ads cannot be renewed by duration; edit the event date instead.".to_string(),
        ));
    }

    // If the ad was expired, renewing it will make it active -> enforce active ads limit
    if stored.ads[ad_idx].ad.expires_at <= now {
        let other_active = stored
            .ads
            .iter()
            .filter(|a| a.ad.id != id && a.is_published_active(now))
            .count();
        if other_active >= humoco_market::MAX_ADS_PER_CARD {
            return Err(FrontendError::with_code(
                "error.market.adLimitReached",
                "Maximum number of active ads reached.",
            ));
        }
    }

    let ad = &mut stored.ads[ad_idx].ad;
    ad.renewed_at = Some(now);
    ad.expires_at = now.saturating_add(duration_days.saturating_mul(86_400));

    if stored.published {
        rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
    }

    save_own_market_profile_state(state, &mut service, &stored, pw)?;

    build_my_market_profile_dto(&service, stored, now)
}

pub fn delete_ad_impl(
    id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut stored = load_own_market_profile_for_write(&mut service, pw)?;

    let ad_idx = stored
        .ads
        .iter()
        .position(|a| a.ad.id == id)
        .ok_or_else(|| FrontendError::from("Ad not found".to_string()))?;
    let was_published_active = stored.ads[ad_idx].is_published_active(now);
    let old_token_opt = stored.ads[ad_idx].ad.image_token.clone();
    stored.ads.remove(ad_idx);

    if stored.published && was_published_active {
        rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
    }

    save_own_market_profile_state(state, &mut service, &stored, pw)?;

    if let Some(ref old_t) = old_token_opt {
        cleanup_unused_own_media(&mut service, &stored.ads, old_t, pw);
    }

    build_my_market_profile_dto(&service, stored, now)
}
