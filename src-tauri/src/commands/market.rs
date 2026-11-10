use tauri::State;

use human_money_core::app_service::AppService;
use crate::models::{
    FieldAudience, FrontendAdInput, FrontendBoardItem,
    FrontendError, FrontendForwardCardExport, FrontendImportCardResult,
    FrontendMarketPerson, FrontendMarketSnapshot, FrontendMyMarketProfile,
    FrontendSaveMyMarketProfileRequest, StoredOwnMarketProfile,
};
use crate::state::{AppState, MARKET_CARD_STORE_KEY, MARKET_OWN_PROFILE_KEY};
use crate::sync_client;

// =========================================================================
//  HELPERS & ARTIFACT BUILDERS
// =========================================================================

pub struct ShareArtifacts {
    pub file_wire: String,
    pub share_uri: String,
    pub qr_svg: Option<String>,
    pub qr_is_partial: bool,
}

pub fn build_share_artifacts(card: &humoco_market::SdCard) -> Result<ShareArtifacts, FrontendError> {
    let file_wire = humoco_market::render_wire(card, 1, humoco_market::WireFilter::Full)
        .ok_or_else(|| FrontendError::from("Failed to render full wire".to_string()))?;

    let qr_wire = humoco_market::render_wire(card, 1, humoco_market::WireFilter::FitBytes(2100))
        .ok_or_else(|| FrontendError::from("Failed to render fit wire for QR".to_string()))?;

    let qr_is_partial = qr_wire.len() < file_wire.len();

    let qr_env = humoco_market::own_envelope(qr_wire);
    let qr_env_json = serde_json::to_string(&qr_env)
        .map_err(|e| FrontendError::from(format!("Failed to serialize CardEnvelope: {}", e)))?;

    let share_uri = format!(
        "humoco://card?env={}",
        human_money_core::crypto::encode_base64(qr_env_json.as_bytes())
    );

    let qr_svg = if share_uri.len() <= humoco_market::MAX_QR_SHARE_BYTES {
        humoco_market::encode_qr_svg(share_uri.as_bytes()).ok()
    } else {
        None
    };

    Ok(ShareArtifacts {
        file_wire,
        share_uri,
        qr_svg,
        qr_is_partial,
    })
}

pub fn rebuild_and_sign_own_card(
    service: &mut AppService,
    stored: &mut StoredOwnMarketProfile,
    now: u64,
) -> Result<(), FrontendError> {
    let (_my_identity, my_signing_key) = service
        .with_wallet_and_identity(|_w, id| (id.clone(), id.signing_key.clone()))
        .map_err(FrontendError::from)?;
    let root_did = human_money_core::crypto::create_user_id(&my_signing_key.verifying_key(), None)
        .map_err(|e| FrontendError::from(format!("Failed to create user ID: {}", e)))?;

    let profile = service
        .with_wallet(|w| w.profile.to_public_profile())
        .map_err(FrontendError::from)?;

    let max_hops = stored.max_hops.clamp(1, 3);
    let audience_dist = |aud: FieldAudience| -> Option<u8> {
        match aud {
            FieldAudience::Everyone => Some(max_hops),
            FieldAudience::Contacts => Some(1),
            FieldAudience::Nobody => None,
        }
    };

    let mut items: Vec<(humoco_market::CardItem, u8)> = Vec::new();

    // Name
    if profile.first_name.is_some() || profile.last_name.is_some() {
        if let Some(dist) = audience_dist(stored.visibility.name) {
            items.push((
                humoco_market::CardItem::Name {
                    first_name: profile.first_name.clone(),
                    last_name: profile.last_name.clone(),
                },
                dist,
            ));
        }
    }

    // Organization
    if let Some(ref org) = profile.organization {
        if !org.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.organization) {
                items.push((humoco_market::CardItem::Organization(org.trim().to_string()), dist));
            }
        }
    }

    // Community
    if let Some(ref comm) = profile.community {
        if !comm.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.community) {
                items.push((humoco_market::CardItem::Community(comm.trim().to_string()), dist));
            }
        }
    }

    // Address -> City & Street
    if let Some(ref addr) = profile.address {
        let zip = if addr.zip_code.trim().is_empty() { None } else { Some(addr.zip_code.trim().to_string()) };
        let city = if addr.city.trim().is_empty() { None } else { Some(addr.city.trim().to_string()) };
        let country = if addr.country.trim().is_empty() { None } else { Some(addr.country.trim().to_string()) };
        if zip.is_some() || city.is_some() || country.is_some() {
            if let Some(dist) = audience_dist(stored.visibility.city) {
                items.push((
                    humoco_market::CardItem::City {
                        zip_code: zip,
                        city,
                        country,
                    },
                    dist,
                ));
            }
        }
        let street_trimmed = addr.street.trim();
        if !street_trimmed.is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.street) {
                let house_num = if addr.house_number.trim().is_empty() { None } else { Some(addr.house_number.trim().to_string()) };
                items.push((
                    humoco_market::CardItem::Street {
                        street: street_trimmed.to_string(),
                        house_number: house_num,
                    },
                    dist,
                ));
            }
        }
    }

    // Coordinates
    if let Some(ref coords) = profile.coordinates {
        if !coords.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.coordinates) {
                items.push((humoco_market::CardItem::Coordinates(coords.trim().to_string()), dist));
            }
        }
    }

    // Email
    if let Some(ref email) = profile.email {
        if !email.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.email) {
                items.push((humoco_market::CardItem::Email(email.trim().to_string()), dist));
            }
        }
    }

    // Phone
    if let Some(ref phone) = profile.phone {
        if !phone.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.phone) {
                items.push((humoco_market::CardItem::Phone(phone.trim().to_string()), dist));
            }
        }
    }

    // Url
    if let Some(ref url) = profile.url {
        if !url.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.url) {
                items.push((humoco_market::CardItem::Url(url.trim().to_string()), dist));
            }
        }
    }

    // Picture
    if let Some(ref pic) = profile.picture_url {
        if !pic.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.picture) {
                items.push((humoco_market::CardItem::Picture(pic.trim().to_string()), dist));
            }
        }
    }

    // Gender
    if let Some(ref gender) = profile.gender {
        if !gender.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.gender) {
                items.push((humoco_market::CardItem::Gender(gender.trim().to_string()), dist));
            }
        }
    }

    // Maintainer Decision 7 & Rule 2: DO NOT include service_offer or needs as CardItem!

    // Skills
    for skill in &stored.skills {
        items.push((humoco_market::CardItem::Skill(skill.clone()), max_hops));
    }

    // Active Ads
    let mut active_ad_count = 0u8;
    for ad in &stored.ads {
        if ad.expires_at > now {
            items.push((humoco_market::CardItem::Ad(ad.clone()), max_hops));
            active_ad_count += 1;
        }
    }

    let last_v = stored.last_card.as_ref().map(|c| c.payload.v);
    let v = humoco_market::next_card_version(last_v, now);
    let valid_until = now.saturating_add(humoco_market::CARD_DEFAULT_VALIDITY_SECS);

    let drops = stored
        .last_card
        .as_ref()
        .map(|c| c.payload.drops.clone())
        .unwrap_or_default();

    let payload_input = humoco_market::CardPayloadInput {
        did: root_did,
        v,
        valid_until,
        max_hops,
        ad_count: active_ad_count,
        allow_inquiries: stored.allow_inquiries,
        drops,
    };

    let signed_card = humoco_market::sign_sd_card(payload_input, items, &my_signing_key)
        .map_err(|e| FrontendError::from(format!("Failed to sign SD card: {}", e)))?;

    stored.last_card = Some(signed_card);
    Ok(())
}

async fn delete_remote_card_slots(state: &AppState, password: Option<&str>) {
    let (my_ed_sk, master_seed, drops) = {
        let mut service = match state.service.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let my_ed_sk = match service.with_wallet_and_identity(|_w, id| id.signing_key.clone()) {
            Ok(k) => k,
            Err(_) => return,
        };
        let master_seed = my_ed_sk.to_bytes();
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password))
            .unwrap_or_default();
        let mut drops = settings.market_selected_syncs;
        for pd in settings.public_syncs {
            if pd.enabled {
                drops.push(crate::models::FrontendSyncDescriptor {
                    write_url: pd.write_url,
                    read_url: pd.read_url,
                    slot_id: Some(pd.access_pubkey),
                    drop_pub: pd.drop_pub,
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                });
            }
        }
        (my_ed_sk, master_seed, drops)
    };
    let client = sync_client::create_sync_http_client();
    let now = sync_client::current_unix_timestamp();
    for drop_desc in &drops {
        let nonce = sync_client::generate_sync_nonce();
        let signed_res = if drop_desc.slot_id.is_some() {
            match humoco_market::derive_public_drop_keypair(&master_seed, &drop_desc.write_url) {
                Ok((r_sk, _, _)) => {
                    Ok(humoco_market::sign_member_request_direct(&r_sk, "DELETE", "card", now, &nonce, b""))
                }
                Err(e) => Err(e.to_string()),
            }
        } else {
            humoco_market::sign_member_request_stealth(
                &my_ed_sk,
                &drop_desc.drop_pub,
                "DELETE",
                "card",
                now,
                &nonce,
                b"",
            )
            .map_err(|e| e.to_string())
        };
        if let Ok(signed) = signed_res {
            let _ = sync_client::delete_slot(&client, &drop_desc.write_url, &signed, "card").await;
        }
    }
}

// =========================================================================
//  COMMAND IMPLEMENTATIONS
// =========================================================================

pub fn get_my_market_profile_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let stored: StoredOwnMarketProfile = service
        .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let core_profile = service
        .with_wallet(|w| w.profile.to_public_profile())
        .map_err(FrontendError::from)?;

    let now = chrono::Utc::now().timestamp().max(0) as u64;

    let ads = stored
        .ads
        .iter()
        .map(|a| crate::models::FrontendMyAd::from_ad(a, now))
        .collect();

    let (share_uri, file_wire, qr_svg, qr_is_partial) = if stored.published {
        if let Some(ref card) = stored.last_card {
            match build_share_artifacts(card) {
                Ok(art) => (Some(art.share_uri), Some(art.file_wire), art.qr_svg, art.qr_is_partial),
                Err(_) => (None, None, None, false),
            }
        } else {
            (None, None, None, false)
        }
    } else {
        (None, None, None, false)
    };

    Ok(FrontendMyMarketProfile {
        profile: core_profile.into(),
        visibility: stored.visibility,
        skills: stored.skills,
        ads,
        max_hops: stored.max_hops,
        allow_inquiries: stored.allow_inquiries,
        published: stored.published,
        share_uri,
        file_wire,
        qr_svg,
        qr_is_partial,
    })
}

#[tauri::command]
pub fn get_my_market_profile(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    get_my_market_profile_impl(password, &state)
}

pub fn save_my_market_profile_impl(
    request: FrontendSaveMyMarketProfileRequest,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let mut service = state.service.lock().unwrap();

    let mut core_profile = service
        .with_wallet(|w| w.profile.to_public_profile())
        .map_err(FrontendError::from)?;

    core_profile.first_name = request.profile.first_name;
    core_profile.last_name = request.profile.last_name;
    core_profile.organization = request.profile.organization;
    core_profile.community = request.profile.community;
    core_profile.address = request.profile.address.map(Into::into);
    core_profile.gender = request.profile.gender;
    core_profile.email = request.profile.email;
    core_profile.phone = request.profile.phone;
    core_profile.coordinates = request.profile.coordinates;
    core_profile.url = request.profile.url;
    core_profile.picture_url = request.profile.picture_url;

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

    service
        .update_public_profile(core_profile, password.as_deref())
        .map_err(|e| FrontendError::from(format!("Failed to update Core public profile: {}", e)))?;

    let mut stored: StoredOwnMarketProfile = service
        .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    stored.visibility = request.visibility;
    stored.skills = request.skills;
    stored.max_hops = request.max_hops.clamp(1, 3);
    stored.allow_inquiries = request.allow_inquiries;

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if stored.published {
        rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
    }

    let bytes = serde_json::to_vec(&stored)
        .map_err(|e| FrontendError::from(format!("Failed to serialize StoredOwnMarketProfile: {}", e)))?;
    service
        .save_encrypted_data(MARKET_OWN_PROFILE_KEY, &bytes, password.as_deref())
        .map_err(FrontendError::from)?;

    drop(service);
    get_my_market_profile_impl(password, state)
}

#[tauri::command]
pub fn save_my_market_profile(
    request: FrontendSaveMyMarketProfileRequest,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    save_my_market_profile_impl(request, password, &state)
}

pub async fn set_card_published_impl(
    published: bool,
    delete_remote: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    {
        let mut service = state.service.lock().unwrap();
        let mut stored: StoredOwnMarketProfile = service
            .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();

        if published {
            stored.published = true;
            rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
        } else {
            stored.published = false;
            stored.last_card = None;
        }

        let bytes = serde_json::to_vec(&stored)
            .map_err(|e| FrontendError::from(format!("Failed to serialize StoredOwnMarketProfile: {}", e)))?;
        service
            .save_encrypted_data(MARKET_OWN_PROFILE_KEY, &bytes, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    if !published && delete_remote {
        delete_remote_card_slots(state, password.as_deref()).await;
    }

    get_my_market_profile_impl(password, state)
}

#[tauri::command]
pub async fn set_card_published(
    published: bool,
    delete_remote: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    set_card_published_impl(published, delete_remote, password, &state).await
}

pub fn upsert_ad_impl(
    ad_input: FrontendAdInput,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    if !humoco_market::AD_DURATION_CHOICES_DAYS.contains(&ad_input.duration_days) {
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

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let mut service = state.service.lock().unwrap();
    let mut stored: StoredOwnMarketProfile = service
        .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let existing_idx = ad_input
        .id
        .as_ref()
        .and_then(|id| stored.ads.iter().position(|a| &a.id == id));

    if let Some(idx) = existing_idx {
        // Edit existing ad: MUST keep published_at, renewed_at, and expires_at unchanged!
        let existing = &mut stored.ads[idx];
        existing.kind = ad_input.kind;
        existing.title = title_trimmed.to_string();
        existing.text = ad_input.text;
        existing.pricing = ad_input.pricing;
        existing.allow_inquiries = ad_input.allow_inquiries;
    } else {
        // Create new ad: enforce active ads limit (max 5)
        let active_count = stored.ads.iter().filter(|a| a.expires_at > now).count();
        if active_count >= humoco_market::MAX_ADS_PER_CARD {
            return Err(FrontendError::from("market.adLimitReached".to_string()));
        }

        let id = ad_input.id.unwrap_or_else(humoco_market::new_ad_id);
        let published_at = now;
        let expires_at = now.saturating_add(ad_input.duration_days.saturating_mul(86_400));
        stored.ads.push(humoco_market::Ad {
            id,
            kind: ad_input.kind,
            title: title_trimmed.to_string(),
            text: ad_input.text,
            pricing: ad_input.pricing,
            published_at,
            renewed_at: None,
            expires_at,
            allow_inquiries: ad_input.allow_inquiries,
        });
    }

    if stored.published {
        rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
    }

    let bytes = serde_json::to_vec(&stored)
        .map_err(|e| FrontendError::from(format!("Failed to serialize StoredOwnMarketProfile: {}", e)))?;
    service
        .save_encrypted_data(MARKET_OWN_PROFILE_KEY, &bytes, password.as_deref())
        .map_err(FrontendError::from)?;

    drop(service);
    get_my_market_profile_impl(password, state)
}

#[tauri::command]
pub fn upsert_ad(
    ad: FrontendAdInput,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    upsert_ad_impl(ad, password, &state)
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
    let mut service = state.service.lock().unwrap();
    let mut stored: StoredOwnMarketProfile = service
        .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let ad_idx = stored
        .ads
        .iter()
        .position(|a| a.id == id)
        .ok_or_else(|| FrontendError::from("Ad not found".to_string()))?;

    // If the ad was expired, renewing it will make it active -> enforce limit of 5 active ads
    if stored.ads[ad_idx].expires_at <= now {
        let other_active = stored.ads.iter().filter(|a| a.id != id && a.expires_at > now).count();
        if other_active >= humoco_market::MAX_ADS_PER_CARD {
            return Err(FrontendError::from("market.adLimitReached".to_string()));
        }
    }

    let ad = &mut stored.ads[ad_idx];
    ad.renewed_at = Some(now);
    ad.expires_at = now.saturating_add(duration_days.saturating_mul(86_400));

    if stored.published {
        rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
    }

    let bytes = serde_json::to_vec(&stored)
        .map_err(|e| FrontendError::from(format!("Failed to serialize StoredOwnMarketProfile: {}", e)))?;
    service
        .save_encrypted_data(MARKET_OWN_PROFILE_KEY, &bytes, password.as_deref())
        .map_err(FrontendError::from)?;

    drop(service);
    get_my_market_profile_impl(password, state)
}

#[tauri::command]
pub fn renew_ad(
    id: String,
    duration_days: u64,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    renew_ad_impl(id, duration_days, password, &state)
}

pub fn delete_ad_impl(
    id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let mut service = state.service.lock().unwrap();
    let mut stored: StoredOwnMarketProfile = service
        .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    stored.ads.retain(|a| a.id != id);

    if stored.published {
        rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
    }

    let bytes = serde_json::to_vec(&stored)
        .map_err(|e| FrontendError::from(format!("Failed to serialize StoredOwnMarketProfile: {}", e)))?;
    service
        .save_encrypted_data(MARKET_OWN_PROFILE_KEY, &bytes, password.as_deref())
        .map_err(FrontendError::from)?;

    drop(service);
    get_my_market_profile_impl(password, state)
}

#[tauri::command]
pub fn delete_ad(
    id: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    delete_ad_impl(id, password, &state)
}

pub fn get_market_view_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketSnapshot, FrontendError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let mut service = state.service.lock().unwrap();

    // Auto-renew own card if published and (validity < 30 days OR active ads count changed)
    let mut stored_opt: Option<StoredOwnMarketProfile> = service
        .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    if let Some(ref mut stored) = stored_opt {
        if stored.published {
            if let Some(ref last_card) = stored.last_card {
                let active_ads_count = stored.ads.iter().filter(|a| a.expires_at > now).count() as u8;
                let time_left = last_card.payload.valid_until.saturating_sub(now);
                if time_left < humoco_market::CARD_AUTO_RENEW_THRESHOLD_SECS || last_card.payload.ad_count != active_ads_count {
                    if let Ok(()) = rebuild_and_sign_own_card(&mut service, stored, now) {
                        if let Ok(bytes) = serde_json::to_vec(stored) {
                            let _ = service.save_encrypted_data(MARKET_OWN_PROFILE_KEY, &bytes, password.as_deref());
                        }
                    }
                }
            }
        }
    }

    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password.as_deref()))
        .unwrap_or_default();

    let reg = state.get_or_load_wot_register(&mut service, password.as_deref()).ok();

    let mut store: humoco_market::CardStore = service
        .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let before_count = store.cards.len();
    store.cards.retain(|_, c| c.card.payload.valid_until > now);
    if store.cards.len() != before_count {
        if let Ok(bytes) = serde_json::to_vec(&store) {
            let _ = service.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password.as_deref());
        }
    }

    let cards_vec: Vec<humoco_market::StoredCard> = store.cards.values().cloned().collect();

    let sync_state: crate::state::StoredSyncState = service
        .load_encrypted_data(crate::state::MARKET_SYNC_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let raw_persons = service
        .with_wallet(|wallet| {
            let own_did = wallet.get_user_id().to_string();
            let vouchers: Vec<&human_money_core::Voucher> = wallet
                .voucher_store
                .vouchers
                .values()
                .map(|vi| &vi.voucher)
                .collect();

            let classify = |did: &str| -> humoco_market::TrustTier {
                use human_money_core::models::conflict::TrustStatus;
                if matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_)) {
                    return humoco_market::TrustTier::Hidden;
                }

                if let Some(ref r) = reg {
                    let can_did = crate::state::canonicalize_did(did).unwrap_or_else(|_| did.to_string());
                    if r.own_contacts.get(&can_did).is_some_and(|c| c.level >= 1) {
                        return humoco_market::TrustTier::Trusted;
                    }
                    if let Ok(eval) = r.evaluate_voucher(&can_did, &[], false, Some(now)) {
                        if eval.score > 0.0 {
                            return humoco_market::TrustTier::Trusted;
                        }
                    }
                }

                humoco_market::TrustTier::Unknown
            };

            humoco_market::build_market_view(vouchers, &cards_vec, &own_did, &classify, now)
        })
        .map_err(FrontendError::from)?;

    let raw_board = humoco_market::build_ad_board(&raw_persons);

    let board: Vec<FrontendBoardItem> = raw_board
        .into_iter()
        .filter(|bi| settings.market_show_unknown || bi.trust == humoco_market::TrustTier::Trusted)
        .map(|bi| FrontendBoardItem::from_board_item(bi, now))
        .collect();

    let mut persons: Vec<FrontendMarketPerson> = raw_persons
        .into_iter()
        .filter(|p| settings.market_show_unknown || p.trust == humoco_market::TrustTier::Trusted)
        .map(FrontendMarketPerson::from)
        .collect();

    for person in persons.iter_mut() {
        let can_did = crate::state::canonicalize_did(&person.did).unwrap_or_else(|_| person.did.clone());
        if let Some(read_url) = sync_state
            .sync_sources_by_did
            .get(&person.did)
            .or_else(|| sync_state.sync_sources_by_did.get(&can_did))
        {
            let sync_src = crate::models::FrontendEntrySource::Sync {
                read_url: read_url.clone(),
            };
            if !person.sources.contains(&sync_src) {
                person.sources.push(sync_src);
            }
        }
    }

    Ok(FrontendMarketSnapshot {
        board,
        people: persons,
        now,
    })
}

#[tauri::command]
pub fn get_market_view(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMarketSnapshot, FrontendError> {
    get_market_view_impl(password, &state)
}

pub fn export_forward_card_impl(
    did: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendForwardCardExport, FrontendError> {
    let mut service = state.service.lock().unwrap();

    let store: humoco_market::CardStore = service
        .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let pk = human_money_core::crypto::get_pubkey_from_user_id(&did)
        .map_err(|e| FrontendError::from(format!("Invalid DID: {}", e)))?;

    let stored = store
        .cards
        .get(pk.as_bytes())
        .ok_or_else(|| FrontendError::from("Card not found in store".to_string()))?;

    let env = humoco_market::forward_envelope(stored)
        .ok_or_else(|| FrontendError::from("Card reached its maximum hop count".to_string()))?;

    let envelope_json = serde_json::to_string(&env)
        .map_err(|e| FrontendError::from(format!("Failed to serialize CardEnvelope: {}", e)))?;
    let share_uri = format!(
        "humoco://card?env={}",
        human_money_core::crypto::encode_base64(envelope_json.as_bytes())
    );

    let qr_svg = if share_uri.len() <= humoco_market::MAX_QR_SHARE_BYTES {
        humoco_market::encode_qr_svg(share_uri.as_bytes()).ok()
    } else {
        None
    };

    Ok(FrontendForwardCardExport {
        did: stored.card.payload.did.clone(),
        envelope_json,
        share_uri,
        qr_svg,
    })
}

#[tauri::command]
pub fn export_forward_card(
    did: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendForwardCardExport, FrontendError> {
    export_forward_card_impl(did, password, &state)
}

#[tauri::command]
pub fn recommend_market_card(
    did: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendForwardCardExport, FrontendError> {
    export_forward_card_impl(did, password, &state)
}

pub fn import_contact_card_impl(
    payload: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendImportCardResult, FrontendError> {
    let mut service = state.service.lock().unwrap();

    let json_str = if let Some((_, query_part)) = payload.trim().split_once("?env=") {
        let b64 = query_part.split('&').next().unwrap_or(query_part);
        let bytes = human_money_core::crypto::decode_base64(b64)
            .map_err(|e| FrontendError::from(format!("Failed to decode card share URI base64: {}", e)))?;
        String::from_utf8(bytes)
            .map_err(|e| FrontendError::from(format!("Invalid UTF-8 in card share URI: {}", e)))?
    } else {
        payload
    };

    let env: humoco_market::CardEnvelope = serde_json::from_str(&json_str)
        .map_err(|e| FrontendError::from(format!("Failed to parse CardEnvelope: {}", e)))?;

    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password.as_deref()))
        .unwrap_or_default();
    let max_store_hops = settings.max_store_hops.clamp(1, 3);
    let now = chrono::Utc::now().timestamp().max(0) as u64;

    let mut store: humoco_market::CardStore = service
        .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let outcome = service
        .with_wallet(|wallet| {
            let is_offender = |did: &str| -> bool {
                use human_money_core::models::conflict::TrustStatus;
                matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_))
            };
            humoco_market::accept_envelope(&mut store, &env, now, max_store_hops, &is_offender)
        })
        .map_err(FrontendError::from)?;

    match outcome {
        humoco_market::AcceptOutcome::Accepted { did, replaced } => {
            let bytes = serde_json::to_vec(&store)
                .map_err(|e| FrontendError::from(format!("Failed to serialize CardStore: {}", e)))?;
            service
                .save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password.as_deref())
                .map_err(FrontendError::from)?;
            if let Some(stored) = store.cards.values().find(|sc| sc.card.payload.did == did) {
                let card_syncs: Vec<crate::models::FrontendSyncDescriptor> =
                    stored.card.payload.drops.iter().cloned().map(Into::into).collect();
                state.update_contact_syncs_from_card(
                    &mut service,
                    &did,
                    &card_syncs,
                    password.as_deref(),
                );
            }
            Ok(FrontendImportCardResult {
                status: "accepted".to_string(),
                did: Some(did),
                replaced,
            })
        }
        humoco_market::AcceptOutcome::IgnoredStale => Ok(FrontendImportCardResult {
            status: "ignored_stale".to_string(),
            did: None,
            replaced: false,
        }),
        humoco_market::AcceptOutcome::RejectedExpired => Ok(FrontendImportCardResult {
            status: "rejected_expired".to_string(),
            did: None,
            replaced: false,
        }),
        humoco_market::AcceptOutcome::RejectedHops => Ok(FrontendImportCardResult {
            status: "rejected_hops".to_string(),
            did: None,
            replaced: false,
        }),
        humoco_market::AcceptOutcome::RejectedOffender => Ok(FrontendImportCardResult {
            status: "rejected_offender".to_string(),
            did: None,
            replaced: false,
        }),
        humoco_market::AcceptOutcome::Invalid(msg) => Ok(FrontendImportCardResult {
            status: format!("invalid: {}", msg),
            did: None,
            replaced: false,
        }),
    }
}

#[tauri::command]
pub fn import_contact_card(
    payload: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendImportCardResult, FrontendError> {
    import_contact_card_impl(payload, password, &state)
}

pub fn delete_foreign_contact_card_impl(
    did: String,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();

    let mut store: humoco_market::CardStore = service
        .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let pk = human_money_core::crypto::get_pubkey_from_user_id(&did)
        .map_err(|e| FrontendError::from(format!("Invalid DID: {}", e)))?;

    if store.cards.remove(pk.as_bytes()).is_some() {
        let bytes = serde_json::to_vec(&store)
            .map_err(|e| FrontendError::from(format!("Failed to serialize CardStore: {}", e)))?;
        service
            .save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    Ok(())
}

#[tauri::command]
pub fn delete_foreign_contact_card(
    did: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    delete_foreign_contact_card_impl(did, password, &state)
}

// =========================================================================
//  TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use human_money_core::app_service::AppService;
    use crate::models::{
        CardFieldVisibility, FieldAudience, FrontendAdInput, FrontendSaveMyMarketProfileRequest,
        FrontendUserProfile,
    };

    const ALICE_PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const BOB_PHRASE: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
    const CHARLIE_PHRASE: &str =
        "legal winner thank year wave sausage worth useful legal winner thank yellow";

    fn temp_test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("humoco_market_test_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_test_app(dir: &Path, profile_name: &str, phrase: &str, password: &str) -> AppService {
        let mut service = AppService::new(dir).unwrap();
        service.create_profile(
            profile_name,
            phrase,
            None,
            Some("pc"),
            password,
            human_money_core::MnemonicLanguage::English,
            "device-market-test".into(),
        ).unwrap();
        service
    }

    fn create_test_state(service: AppService) -> AppState {
        AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(None),
            regional_views: Mutex::new(None),
            wot: Mutex::new(None),
        }
    }

    #[test]
    fn test_ad_duration_validation_and_renewals() {
        let base_dir = temp_test_dir("ad_duration_and_renewals");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        // Invalid duration 365 rejected
        let bad_ad = FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Guitar lessons".to_string(),
            text: None,
            pricing: None,
            duration_days: 365,
            allow_inquiries: true,
        };
        assert!(upsert_ad_impl(bad_ad, Some(password.to_string()), &alice_state).is_err());

        // Valid duration 30 accepted
        let ad_30 = FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Guitar lessons".to_string(),
            text: None,
            pricing: None,
            duration_days: 30,
            allow_inquiries: true,
        };
        let p1 = upsert_ad_impl(ad_30, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p1.ads.len(), 1);
        let ad_id = p1.ads[0].id.clone();
        let pub_at = p1.ads[0].published_at;
        assert_eq!(p1.ads[0].renewed_at, None);
        assert_eq!(p1.ads[0].status, "active");

        // Renew ad with 90 days
        let p2 = renew_ad_impl(ad_id.clone(), 90, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p2.ads[0].published_at, pub_at, "published_at must stay unchanged");
        assert!(p2.ads[0].renewed_at.is_some());
        assert!(p2.ads[0].expires_at > p1.ads[0].expires_at);

        // Renew ad with invalid 45 days rejected
        assert!(renew_ad_impl(ad_id.clone(), 45, Some(password.to_string()), &alice_state).is_err());

        // Renew ad with 180 days succeeds
        let p3 = renew_ad_impl(ad_id, 180, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p3.ads[0].published_at, pub_at);
        assert!(p3.ads[0].expires_at > p2.ads[0].expires_at);
    }

    #[test]
    fn test_ad_limit_enforced_and_edit_preserves_timestamps() {
        let base_dir = temp_test_dir("ad_limits_and_edit");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        // Create 5 active ads
        let mut ad_ids = Vec::new();
        for i in 1..=5 {
            let ad_input = FrontendAdInput {
                id: None,
                kind: humoco_market::AdKind::Offer,
                title: format!("Service {}", i),
                text: None,
                pricing: None,
                duration_days: 90,
                allow_inquiries: true,
            };
            let p = upsert_ad_impl(ad_input, Some(password.to_string()), &alice_state).unwrap();
            ad_ids.push(p.ads.last().unwrap().id.clone());
        }

        // 6th active ad fails with market.adLimitReached
        let ad_6 = FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Service 6".to_string(),
            text: None,
            pricing: None,
            duration_days: 90,
            allow_inquiries: true,
        };
        let err = upsert_ad_impl(ad_6, Some(password.to_string()), &alice_state).unwrap_err();
        assert!(err.message.contains("market.adLimitReached"));

        // Editing an existing ad succeeds even when 5 ads are active
        let target_id = ad_ids[0].clone();
        let p_before = get_my_market_profile_impl(Some(password.to_string()), &alice_state).unwrap();
        let ad_before = p_before.ads.iter().find(|a| a.id == target_id).unwrap().clone();

        let edit_input = FrontendAdInput {
            id: Some(target_id.clone()),
            kind: humoco_market::AdKind::Request,
            title: "Service 1 (Updated)".to_string(),
            text: Some("Updated description".to_string()),
            pricing: Some("15 Minuto".to_string()),
            duration_days: 90,
            allow_inquiries: false,
        };
        let p_after = upsert_ad_impl(edit_input, Some(password.to_string()), &alice_state).unwrap();
        let ad_after = p_after.ads.iter().find(|a| a.id == target_id).unwrap();

        assert_eq!(ad_after.title, "Service 1 (Updated)");
        assert_eq!(ad_after.kind, humoco_market::AdKind::Request);
        assert_eq!(ad_after.text.as_deref(), Some("Updated description"));
        assert_eq!(ad_after.pricing.as_deref(), Some("15 Minuto"));
        assert!(!ad_after.allow_inquiries);

        // Crucial requirement: edit MUST preserve published_at, renewed_at, and expires_at!
        assert_eq!(ad_after.published_at, ad_before.published_at);
        assert_eq!(ad_after.renewed_at, ad_before.renewed_at);
        assert_eq!(ad_after.expires_at, ad_before.expires_at);

        // Deleting 1 ad allows creating another
        let p_del = delete_ad_impl(target_id, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p_del.ads.len(), 4);

        let ad_new = FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "New Service".to_string(),
            text: None,
            pricing: None,
            duration_days: 30,
            allow_inquiries: true,
        };
        let p_new = upsert_ad_impl(ad_new, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p_new.ads.len(), 5);
    }

    #[tokio::test]
    async fn test_visibility_rules_and_sd_card_disclosures() {
        let base_dir = temp_test_dir("visibility_rules");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        let save_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                last_name: Some("Smith".to_string()),
                organization: Some("Acme Corp".to_string()),
                email: Some("alice@example.com".to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility {
                name: FieldAudience::Everyone,
                organization: FieldAudience::Contacts,
                email: FieldAudience::Nobody,
                ..Default::default()
            },
            skills: vec![humoco_market::Skill {
                title: "Engineering".to_string(),
                text: None,
                pricing: None,
            }],
            max_hops: 2,
            allow_inquiries: true,
        };

        save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state).unwrap();
        set_card_published_impl(true, false, Some(password.to_string()), &alice_state).await.unwrap();

        let mut stored = {
            let mut s = alice_state.service.lock().unwrap();
            let bytes = s.load_encrypted_data(MARKET_OWN_PROFILE_KEY, Some(password)).unwrap();
            serde_json::from_slice::<StoredOwnMarketProfile>(&bytes).unwrap()
        };

        let now = chrono::Utc::now().timestamp().max(0) as u64;
        {
            let mut s = alice_state.service.lock().unwrap();
            rebuild_and_sign_own_card(&mut s, &mut stored, now).unwrap();
        }

        let card = stored.last_card.unwrap();

        // Distance 1 wire: Name (Everyone, max_hops=2) and Organization (Contacts, dist=1) revealed; Email omitted
        let wire_hop1 = humoco_market::render_wire(&card, 1, humoco_market::WireFilter::Full).unwrap();
        let verified_hop1 = humoco_market::verify_sd_card(&wire_hop1).unwrap();
        let profile_hop1 = humoco_market::extract_public_profile(&verified_hop1);

        assert_eq!(profile_hop1.first_name.as_deref(), Some("Alice"));
        assert_eq!(profile_hop1.last_name.as_deref(), Some("Smith"));
        assert_eq!(profile_hop1.organization.as_deref(), Some("Acme Corp"));
        assert_eq!(profile_hop1.email, None, "Email (Nobody) must never be disclosed");

        // Distance 2 wire: Name revealed (Everyone), but Organization (Contacts only) is redacted!
        let wire_hop2 = humoco_market::render_wire(&card, 2, humoco_market::WireFilter::Full).unwrap();
        let verified_hop2 = humoco_market::verify_sd_card(&wire_hop2).unwrap();
        let profile_hop2 = humoco_market::extract_public_profile(&verified_hop2);

        assert_eq!(profile_hop2.first_name.as_deref(), Some("Alice"));
        assert_eq!(profile_hop2.organization, None, "Organization (Contacts) must be redacted at hop 2");
        assert_eq!(profile_hop2.email, None);
    }

    #[test]
    fn test_service_offer_derivation_and_needs_preservation() {
        let base_dir = temp_test_dir("service_offer_and_needs");
        let password = "TestPassword123!";
        let mut alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);

        // Precondition: Set Core needs on wallet profile
        let mut p = alice_service.with_wallet(|w| w.profile.to_public_profile()).unwrap();
        p.needs = Some("Volunteers needed".to_string());
        alice_service.update_public_profile(p, Some(password)).unwrap();

        let alice_state = create_test_state(alice_service);

        let save_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![
                humoco_market::Skill {
                    title: "Carpentry".to_string(),
                    text: None,
                    pricing: None,
                },
                humoco_market::Skill {
                    title: "Plumbing".to_string(),
                    text: None,
                    pricing: None,
                },
            ],
            max_hops: 2,
            allow_inquiries: true,
        };

        save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state).unwrap();

        let s = alice_state.service.lock().unwrap();
        let core_profile = s.with_wallet(|w| w.profile.to_public_profile()).unwrap();

        // Rule 7: service_offer is derived from skills
        assert_eq!(core_profile.service_offer.as_deref(), Some("Carpentry, Plumbing"));
        // Rule 7: needs is preserved!
        assert_eq!(core_profile.needs.as_deref(), Some("Volunteers needed"));
    }

    #[tokio::test]
    async fn test_full_marketplace_flow_import_forward_and_board() {
        let base_dir = temp_test_dir("full_marketplace_flow");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let charlie_service = setup_test_app(&base_dir.join("charlie"), "Charlie", CHARLIE_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);
        let charlie_state = create_test_state(charlie_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            let raw = s.with_wallet(|w| w.get_user_id().to_string()).unwrap();
            crate::state::canonicalize_did(&raw).unwrap()
        };

        // 1. Alice creates profile, publishes card with 1 ad
        let save_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                last_name: Some("Baker".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![humoco_market::Skill {
                title: "Artisan Bread".to_string(),
                text: None,
                pricing: None,
            }],
            max_hops: 2,
            allow_inquiries: true,
        };
        save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state).unwrap();

        let ad_req = FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Fresh Baguettes".to_string(),
            text: Some("Daily baked".to_string()),
            pricing: Some("3 M".to_string()),
            duration_days: 90,
            allow_inquiries: true,
        };
        upsert_ad_impl(ad_req, Some(password.to_string()), &alice_state).unwrap();
        let alice_profile = set_card_published_impl(true, false, Some(password.to_string()), &alice_state).await.unwrap();

        assert!(alice_profile.published);
        assert!(alice_profile.share_uri.is_some());
        let share_uri = alice_profile.share_uri.unwrap();

        // 2. Bob imports Alice's card via share URI
        let import_res_bob = import_contact_card_impl(share_uri, Some(password.to_string()), &bob_state).unwrap();
        assert_eq!(import_res_bob.status, "accepted");

        // Bob checks market view and board
        let bob_view = get_market_view_impl(Some(password.to_string()), &bob_state).unwrap();
        assert_eq!(bob_view.people.len(), 1);
        assert_eq!(bob_view.people[0].did, alice_did);
        assert_eq!(bob_view.people[0].profile.first_name.as_deref(), Some("Alice"));

        assert_eq!(bob_view.board.len(), 1);
        assert_eq!(bob_view.board[0].owner_did, alice_did);
        assert_eq!(bob_view.board[0].ad.title, "Fresh Baguettes");

        // 3. Bob forwards Alice's card to Charlie (hops = 2)
        let bob_export = export_forward_card_impl(alice_did.clone(), Some(password.to_string()), &bob_state).unwrap();
        let charlie_import = import_contact_card_impl(bob_export.envelope_json, Some(password.to_string()), &charlie_state).unwrap();
        assert_eq!(charlie_import.status, "accepted");

        let charlie_view = get_market_view_impl(Some(password.to_string()), &charlie_state).unwrap();
        assert_eq!(charlie_view.people.len(), 1);
        assert_eq!(charlie_view.people[0].did, alice_did);
        assert_eq!(charlie_view.people[0].sources[0], crate::models::FrontendEntrySource::Card { hops: 2 });
        // Since max_hops = 2, Charlie cannot forward further
        assert!(!charlie_view.people[0].can_forward);
        assert!(export_forward_card_impl(alice_did.clone(), Some(password.to_string()), &charlie_state).is_err());

        // 4. Bob deletes Alice's card from his store
        delete_foreign_contact_card_impl(alice_did.clone(), Some(password.to_string()), &bob_state).unwrap();
        let bob_view_empty = get_market_view_impl(Some(password.to_string()), &bob_state).unwrap();
        assert!(bob_view_empty.people.is_empty());
        assert!(bob_view_empty.board.is_empty());
    }
}
