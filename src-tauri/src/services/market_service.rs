use crate::models::{
    FieldAudience, FrontendAdInput, FrontendBoardItem, FrontendError, FrontendForwardCardExport,
    FrontendImportCardResult, FrontendMarketPerson, FrontendMarketSnapshot,
    FrontendMyMarketProfile, FrontendSaveMyMarketProfileRequest, FrontendSyncDescriptor,
    FrontendTrustTier, StoredAppNotices, StoredOwnMarketProfile,
};
use crate::services::common::DropChannelSigner;
use crate::settings::{AppSettings, StoredPublicSync, SETTINGS_KEY};
use crate::state::{AppState, CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY, MARKET_OWN_PROFILE_KEY};
use crate::sync_client;
use human_money_core::app_service::AppService;

pub struct ShareArtifacts {
    pub file_wire: String,
    pub share_uri: String,
    pub qr_svg: Option<String>,
    pub qr_is_partial: bool,
}

pub fn build_share_artifacts(
    card: &humoco_market::SdCard,
) -> Result<ShareArtifacts, FrontendError> {
    let raw_file_wire = humoco_market::render_wire(card, 1, humoco_market::WireFilter::Full)
        .ok_or_else(|| FrontendError::from("Failed to render full wire".to_string()))?;

    let qr_wire = humoco_market::render_wire(card, 1, humoco_market::WireFilter::FitBytes(2100))
        .ok_or_else(|| FrontendError::from("Failed to render fit wire for QR".to_string()))?;

    let qr_is_partial = qr_wire.len() < raw_file_wire.len();

    let file_env = humoco_market::own_envelope(raw_file_wire);
    let file_wire = serde_json::to_string_pretty(&file_env).map_err(|e| {
        FrontendError::from(format!("Failed to serialize file CardEnvelope: {}", e))
    })?;

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

pub(crate) fn build_card_drops(
    selected_syncs: &[FrontendSyncDescriptor],
    public_syncs: &[StoredPublicSync],
    my_prefix: Option<&str>,
) -> Vec<humoco_market::DropDescriptor> {
    let mut drops: Vec<humoco_market::DropDescriptor> =
        selected_syncs.iter().cloned().map(Into::into).collect();

    for pd in public_syncs {
        if pd.enabled
            && !pd.write_url.is_empty()
            && !pd.read_url.is_empty()
            && !pd.drop_pub.is_empty()
            && !drops.iter().any(|d| d.write_url == pd.write_url)
        {
            let slot_id = humoco_market::format_slot_id(&pd.access_pubkey, my_prefix)
                .unwrap_or_else(|_| pd.access_pubkey.clone());
            drops.push(humoco_market::DropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(slot_id),
                drop_pub: pd.drop_pub.clone(),
            });
        }
    }

    drops.truncate(humoco_market::MAX_CARD_DROPS);
    drops
}

pub(crate) fn load_drops_from_settings(
    service: &mut AppService,
    selected_override: Option<&[FrontendSyncDescriptor]>,
    password: Option<&str>,
) -> Option<Vec<humoco_market::DropDescriptor>> {
    let settings: AppSettings = service
        .load_encrypted_data(SETTINGS_KEY, password)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())?;
    let my_prefix = service
        .with_wallet_and_identity(|_w, id| {
            human_money_core::crypto::get_prefix_from_user_id(&id.user_id).map(|s| s.to_string())
        })
        .ok()
        .flatten();
    let selected = selected_override.unwrap_or(&settings.market_selected_syncs);
    Some(build_card_drops(
        selected,
        &settings.public_syncs,
        my_prefix.as_deref(),
    ))
}

fn load_own_market_profile_state(
    service: &mut AppService,
    password: Option<&str>,
) -> StoredOwnMarketProfile {
    service
        .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_own_market_profile_state(
    service: &mut AppService,
    stored: &StoredOwnMarketProfile,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec(stored).map_err(|e| {
        FrontendError::from(format!("Failed to serialize StoredOwnMarketProfile: {}", e))
    })?;
    service
        .save_encrypted_data(MARKET_OWN_PROFILE_KEY, &bytes, password)
        .map_err(FrontendError::from)
}

fn cleanup_unused_own_media(
    service: &mut AppService,
    ads: &[humoco_market::Ad],
    old_token: &str,
    password: Option<&str>,
) {
    let is_used = ads
        .iter()
        .any(|a| a.image_token.as_deref() == Some(old_token));
    if !is_used {
        let _ = service.save_encrypted_data(
            &crate::state::own_media_storage_key(old_token),
            &[],
            password,
        );
    }
}

fn encode_media_data_uri(media_bytes: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let prefix = humoco_market::detect_media_data_uri_prefix(media_bytes);
    format!("{}{}", prefix, STANDARD.encode(media_bytes))
}

fn try_load_cached_media(
    service: &mut AppService,
    storage_key: &str,
    password: Option<&str>,
) -> Option<String> {
    let media_bytes = service.load_encrypted_data(storage_key, password).ok()?;
    humoco_market::validate_ad_media(&media_bytes).ok()?;
    Some(encode_media_data_uri(&media_bytes))
}

pub(crate) fn get_own_card_max_hops_inner(service: &mut AppService, password: Option<&str>) -> u8 {
    load_own_market_profile_state(service, password)
        .max_hops
        .clamp(1, 3)
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

pub(crate) fn sign_unpublish_tombstone_card(
    service: &mut AppService,
    stored: &mut StoredOwnMarketProfile,
    now: u64,
) -> Result<(), FrontendError> {
    let Some(ref prev) = stored.last_card else {
        return Ok(());
    };
    let my_signing_key = service
        .with_wallet_and_identity(|_w, id| id.signing_key.clone())
        .map_err(FrontendError::from)?;
    let root_did = human_money_core::crypto::create_user_id(&my_signing_key.verifying_key(), None)
        .map_err(|e| FrontendError::from(format!("Failed to create user ID: {}", e)))?;

    let prev_hops = prev.payload.max_hops.clamp(1, 3);
    let active_hops = if now < stored.shrink_until {
        prev_hops.max(stored.shrink_hops).clamp(1, 3)
    } else {
        prev_hops
    };
    stored.shrink_hops = active_hops;
    stored.shrink_until = now.saturating_add(30 * 86_400);

    let v = humoco_market::next_card_version(Some(prev.payload.v), now);
    let valid_until = now.saturating_add(30 * 86_400);
    let payload_input = humoco_market::CardPayloadInput {
        did: root_did,
        v,
        valid_until,
        max_hops: active_hops,
        ad_count: 0,
        allow_inquiries: false,
        drops: Vec::new(),
    };
    let tombstone = humoco_market::sign_sd_card(payload_input, Vec::new(), &my_signing_key)
        .map_err(|e| FrontendError::from(format!("Failed to sign tombstone SD card: {}", e)))?;
    stored.last_card = Some(tombstone);
    Ok(())
}

/// Re-signs `last_card` with a strictly higher version `v` so evicted contacts
/// accept and apply the `<jws>~` empty-shell tombstone over cached disclosures.
pub(crate) fn bump_own_card_version_for_eviction(service: &mut AppService, password: Option<&str>) {
    let mut stored: StoredOwnMarketProfile = match service
        .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        Some(s) => s,
        None => return,
    };
    if stored.last_card.is_none() {
        return;
    }
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let res = if stored.published {
        let drops_opt = load_drops_from_settings(service, None, password);
        rebuild_and_sign_own_card_with_drops(service, &mut stored, now, drops_opt)
    } else {
        sign_unpublish_tombstone_card(service, &mut stored, now)
    };
    if res.is_ok() {
        let _ = save_own_market_profile_state(service, &stored, password);
    }
}

pub(crate) fn set_own_card_max_hops_inner(
    service: &mut AppService,
    max_hops: u8,
    password: Option<&str>,
) -> Result<u8, FrontendError> {
    let clamped = max_hops.clamp(1, 3);
    let mut stored = load_own_market_profile_state(service, password);

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    update_shrink_window(&mut stored, clamped, now);
    stored.max_hops = clamped;
    if stored.published {
        let drops_opt = load_drops_from_settings(service, None, password);
        rebuild_and_sign_own_card_with_drops(service, &mut stored, now, drops_opt)?;
    }

    save_own_market_profile_state(service, &stored, password)?;

    Ok(clamped)
}

pub fn sync_own_card_drops_on_settings_change(
    service: &mut AppService,
    syncs: &[FrontendSyncDescriptor],
    password: Option<&str>,
) {
    let existing_opt: Option<StoredOwnMarketProfile> = service
        .load_encrypted_data(MARKET_OWN_PROFILE_KEY, password)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    if let Some(mut existing) = existing_opt {
        if existing.published {
            let now_unix = chrono::Utc::now().timestamp().max(0) as u64;
            let drops =
                load_drops_from_settings(service, Some(syncs), password).unwrap_or_else(|| {
                    syncs
                        .iter()
                        .take(humoco_market::MAX_CARD_DROPS)
                        .cloned()
                        .map(Into::into)
                        .collect()
                });
            let _ =
                rebuild_and_sign_own_card_with_drops(service, &mut existing, now_unix, Some(drops));
            let _ = save_own_market_profile_state(service, &existing, password);
        }
    }
}

pub fn rebuild_and_sign_own_card(
    service: &mut AppService,
    stored: &mut StoredOwnMarketProfile,
    now: u64,
) -> Result<(), FrontendError> {
    rebuild_and_sign_own_card_with_drops(service, stored, now, None)
}

pub fn rebuild_and_sign_own_card_with_drops(
    service: &mut AppService,
    stored: &mut StoredOwnMarketProfile,
    now: u64,
    override_drops: Option<Vec<humoco_market::DropDescriptor>>,
) -> Result<(), FrontendError> {
    let (my_identity, my_signing_key) = service
        .with_wallet_and_identity(|_w, id| (id.clone(), id.signing_key.clone()))
        .map_err(FrontendError::from)?;
    let my_account_id = humoco_market::canonicalize_owner_account_id(&my_identity.user_id)
        .map_err(FrontendError::from)?;

    let profile = service
        .with_wallet(|w| w.profile.to_public_profile())
        .map_err(FrontendError::from)?;

    let max_hops = stored.max_hops.clamp(1, 3);
    let envelope_max_hops = if now < stored.shrink_until {
        max_hops.max(stored.shrink_hops).clamp(1, 3)
    } else {
        max_hops
    };
    let audience_dist = |aud: FieldAudience| -> Option<u8> {
        match aud {
            FieldAudience::Everyone => Some(max_hops),
            FieldAudience::Contacts => Some(1),
            FieldAudience::Nobody => None,
        }
    };

    let mut items: Vec<(humoco_market::CardItem, u8)> = Vec::new();

    // Name
    let first_name = profile
        .first_name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let last_name = profile
        .last_name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if first_name.is_some() || last_name.is_some() {
        if let Some(dist) = audience_dist(stored.visibility.name) {
            items.push((
                humoco_market::CardItem::Name {
                    first_name,
                    last_name,
                },
                dist,
            ));
        }
    }

    // Organization
    if let Some(ref org) = profile.organization {
        if !org.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.organization) {
                items.push((
                    humoco_market::CardItem::Organization(org.trim().to_string()),
                    dist,
                ));
            }
        }
    }

    // Community
    if let Some(ref comm) = profile.community {
        if !comm.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.community) {
                items.push((
                    humoco_market::CardItem::Community(comm.trim().to_string()),
                    dist,
                ));
            }
        }
    }

    // Address -> City & Street
    if let Some(ref addr) = profile.address {
        let zip = if addr.zip_code.trim().is_empty() {
            None
        } else {
            Some(addr.zip_code.trim().to_string())
        };
        let city = if addr.city.trim().is_empty() {
            None
        } else {
            Some(addr.city.trim().to_string())
        };
        let country = if addr.country.trim().is_empty() {
            None
        } else {
            Some(addr.country.trim().to_string())
        };
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
                let house_num = if addr.house_number.trim().is_empty() {
                    None
                } else {
                    Some(addr.house_number.trim().to_string())
                };
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
                items.push((
                    humoco_market::CardItem::Coordinates(coords.trim().to_string()),
                    dist,
                ));
            }
        }
    }

    // Email
    if let Some(ref email) = profile.email {
        if !email.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.email) {
                items.push((
                    humoco_market::CardItem::Email(email.trim().to_string()),
                    dist,
                ));
            }
        }
    }

    // Phone
    if let Some(ref phone) = profile.phone {
        if !phone.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.phone) {
                items.push((
                    humoco_market::CardItem::Phone(phone.trim().to_string()),
                    dist,
                ));
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
                items.push((
                    humoco_market::CardItem::Picture(pic.trim().to_string()),
                    dist,
                ));
            }
        }
    }

    // Gender
    if let Some(ref gender) = profile.gender {
        if !gender.trim().is_empty() {
            if let Some(dist) = audience_dist(stored.visibility.gender) {
                items.push((
                    humoco_market::CardItem::Gender(gender.trim().to_string()),
                    dist,
                ));
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
    let validity_years = stored.validity_years.clamp(1, 3) as u64;
    let valid_until = now.saturating_add(validity_years * 365 * 86_400);

    let drops = override_drops.unwrap_or_else(|| {
        stored
            .last_card
            .as_ref()
            .map(|c| c.payload.drops.clone())
            .unwrap_or_default()
    });

    let payload_input = humoco_market::CardPayloadInput {
        did: my_account_id,
        v,
        valid_until,
        max_hops: envelope_max_hops,
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
    let (my_ed_sk, master_seed, my_prefix, drops) = {
        let mut service = match state.service.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let (my_ed_sk, my_prefix) = match service.with_wallet_and_identity(|_w, id| {
            let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                .map(|s| s.to_string());
            (id.signing_key.clone(), pfx)
        }) {
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
                drops.push(FrontendSyncDescriptor {
                    write_url: pd.write_url,
                    read_url: pd.read_url,
                    slot_id: Some(pd.access_pubkey),
                    drop_pub: pd.drop_pub,
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                });
            }
        }
        (my_ed_sk, master_seed, my_prefix, drops)
    };
    let client = sync_client::create_sync_http_client();
    let now = sync_client::current_unix_timestamp();
    let delta_prefix = humoco_market::derive_delta_prefix(my_prefix.as_deref());
    for drop_desc in drops {
        let dd: humoco_market::DropDescriptor = drop_desc.into();
        let signer = DropChannelSigner {
            is_public: false,
            master_seed: &master_seed,
            my_ed_sk: &my_ed_sk,
            drop_desc: &dd,
        };
        for prefix in [my_prefix.as_deref(), Some(delta_prefix.as_str())] {
            let nonce = sync_client::generate_sync_nonce();
            if let Ok(signed) = signer.sign_request("DELETE", "card", prefix, now, &nonce, b"") {
                let _ = sync_client::delete_slot(&client, &dd.write_url, &signed, "card").await;
            }
        }
    }
}

pub fn get_my_market_profile_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let stored = load_own_market_profile_state(&mut service, password.as_deref());

    let core_profile = service
        .with_wallet(|w| w.profile.to_public_profile())
        .map_err(FrontendError::from)?;

    let now = chrono::Utc::now().timestamp().max(0) as u64;

    let media_used_count = stored
        .ads
        .iter()
        .filter(|a| a.expires_at > now && a.image_token.is_some())
        .count();

    let ads = stored
        .ads
        .iter()
        .map(|a| crate::models::FrontendMyAd::from_ad(a, now))
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

fn coarsen_public_coordinates(raw: &str) -> String {
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
    if let Some(ref pic) = request.profile.picture_url {
        let pic_trimmed = pic.trim();
        if !pic_trimmed.is_empty() && humoco_market::is_supported_media_data_uri(pic_trimmed) {
            humoco_market::validate_avatar_data_uri(pic_trimmed)
                .map_err(|e| FrontendError::from(e.to_string()))?;
        }
    }

    let mut service = state.service.lock().unwrap();

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
    core_profile.picture_url = request.profile.picture_url.clone();

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

    let mut stored = load_own_market_profile_state(&mut service, password.as_deref());

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let new_max_hops = request.max_hops.clamp(1, 3);
    update_shrink_window(&mut stored, new_max_hops, now);
    stored.visibility = sanitized_visibility;
    stored.skills = request.skills;
    stored.max_hops = new_max_hops;
    stored.validity_years = request.validity_years.clamp(1, 3);
    stored.allow_inquiries = request.allow_inquiries;
    for ad in &mut stored.ads {
        ad.allow_inquiries = request.allow_inquiries;
    }

    if let Some(pub_flag) = request.published {
        if pub_flag {
            stored.published = true;
        } else {
            stored.published = false;
            sign_unpublish_tombstone_card(&mut service, &mut stored, now)?;
        }
    }

    if stored.published {
        let drops_opt = load_drops_from_settings(&mut service, None, password.as_deref());
        rebuild_and_sign_own_card_with_drops(&mut service, &mut stored, now, drops_opt)?;
    }

    save_own_market_profile_state(&mut service, &stored, password.as_deref())?;

    drop(service);
    get_my_market_profile_impl(password, state)
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
        let mut stored = load_own_market_profile_state(&mut service, password.as_deref());

        if published {
            stored.published = true;
            let drops_opt = load_drops_from_settings(&mut service, None, password.as_deref());
            rebuild_and_sign_own_card_with_drops(&mut service, &mut stored, now, drops_opt)?;
        } else {
            stored.published = false;
            if delete_remote {
                stored.last_card = None;
                stored.shrink_hops = 0;
                stored.shrink_until = 0;
            } else {
                sign_unpublish_tombstone_card(&mut service, &mut stored, now)?;
            }
        }

        save_own_market_profile_state(&mut service, &stored, password.as_deref())?;
    }

    if !published && delete_remote {
        delete_remote_card_slots(state, password.as_deref()).await;
    }

    get_my_market_profile_impl(password, state)
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
    let mut stored = load_own_market_profile_state(&mut service, password.as_deref());

    let existing_idx = ad_input
        .id
        .as_ref()
        .and_then(|id| stored.ads.iter().position(|a| &a.id == id));

    let new_image_token: Option<String> = match ad_input.image_webp_data_uri {
        Some(ref uri) if uri.trim().is_empty() => None,
        Some(ref uri) => {
            let uri_trimmed = uri.trim();
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
            let pk = human_money_core::crypto::get_pubkey_from_user_id(&raw_id)
                .map_err(|e| FrontendError::from(format!("Failed to parse wallet DID: {}", e)))?;
            let root_did = human_money_core::crypto::create_user_id(&pk, None)
                .map_err(|e| FrontendError::from(format!("Failed to recreate root DID: {}", e)))?;

            let (token, _blob_id, _ciphertext) =
                humoco_market::seal_media_blob(&root_did, &media_bytes)
                    .map_err(|e| FrontendError::from(e.to_string()))?;

            service
                .save_encrypted_data(
                    &crate::state::own_media_storage_key(&token),
                    &media_bytes,
                    password.as_deref(),
                )
                .map_err(FrontendError::from)?;

            Some(token)
        }
        None => {
            if let Some(idx) = existing_idx {
                stored.ads[idx].image_token.clone()
            } else {
                None
            }
        }
    };

    let mut old_token_to_clean: Option<String> = None;
    if let Some(idx) = existing_idx {
        // Edit existing ad: MUST keep published_at, renewed_at, and expires_at unchanged!
        let existing = &mut stored.ads[idx];
        let old_token = existing.image_token.clone();
        existing.kind = ad_input.kind;
        existing.title = title_trimmed.to_string();
        existing.text = ad_input.text;
        existing.pricing = ad_input.pricing;
        existing.allow_inquiries = ad_input.allow_inquiries;
        existing.is_remote = ad_input.is_remote;
        existing.image_token = new_image_token.clone();

        if let Some(ref old_t) = old_token {
            if Some(old_t) != new_image_token.as_ref() {
                old_token_to_clean = Some(old_t.clone());
            }
        }
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
            is_remote: ad_input.is_remote,
            image_token: new_image_token,
        });
    }

    if stored.published {
        rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
    }

    save_own_market_profile_state(&mut service, &stored, password.as_deref())?;

    if let Some(ref old_t) = old_token_to_clean {
        cleanup_unused_own_media(&mut service, &stored.ads, old_t, password.as_deref());
    }

    drop(service);
    get_my_market_profile_impl(password, state)
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
    let mut stored = load_own_market_profile_state(&mut service, password.as_deref());

    let ad_idx = stored
        .ads
        .iter()
        .position(|a| a.id == id)
        .ok_or_else(|| FrontendError::from("Ad not found".to_string()))?;

    // If the ad was expired, renewing it will make it active -> enforce limit of 5 active ads
    if stored.ads[ad_idx].expires_at <= now {
        let other_active = stored
            .ads
            .iter()
            .filter(|a| a.id != id && a.expires_at > now)
            .count();
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

    save_own_market_profile_state(&mut service, &stored, password.as_deref())?;

    drop(service);
    get_my_market_profile_impl(password, state)
}

pub fn delete_ad_impl(
    id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMyMarketProfile, FrontendError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let mut service = state.service.lock().unwrap();
    let mut stored = load_own_market_profile_state(&mut service, password.as_deref());

    let old_token_opt = stored
        .ads
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.image_token.clone());
    stored.ads.retain(|a| a.id != id);

    if let Some(ref old_t) = old_token_opt {
        cleanup_unused_own_media(&mut service, &stored.ads, old_t, password.as_deref());
    }

    if stored.published {
        rebuild_and_sign_own_card(&mut service, &mut stored, now)?;
    }

    save_own_market_profile_state(&mut service, &stored, password.as_deref())?;

    drop(service);
    get_my_market_profile_impl(password, state)
}

pub async fn get_ad_image_impl(
    owner_did: String,
    image_token: String,
    password: Option<String>,
    state: &AppState,
) -> Result<Option<String>, FrontendError> {
    if !humoco_market::is_valid_image_token(&image_token) {
        return Ok(None);
    }

    let blob_id = match humoco_market::derive_media_blob_id(&image_token) {
        Ok(id) => id,
        Err(_) => return Ok(None),
    };

    let read_urls = {
        let mut service = match state.service.lock() {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };

        let own_key = crate::state::own_media_storage_key(&image_token);
        if let Some(uri) = try_load_cached_media(&mut service, &own_key, password.as_deref()) {
            return Ok(Some(uri));
        }

        let cache_key = crate::state::cached_media_storage_key(&blob_id);
        if let Some(uri) = try_load_cached_media(&mut service, &cache_key, password.as_deref()) {
            return Ok(Some(uri));
        }

        let mut candidate_urls = Vec::new();

        if let Ok(bytes) = service.load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref()) {
            if let Ok(card_store) = serde_json::from_slice::<humoco_market::CardStore>(&bytes) {
                if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(&owner_did) {
                    if let Some(stored_card) = card_store.cards.get(&pk.to_bytes()) {
                        for d in &stored_card.card.payload.drops {
                            if !d.read_url.trim().is_empty() {
                                candidate_urls.push(d.read_url.clone());
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

        for s in &settings.market_selected_syncs {
            if !s.read_url.trim().is_empty() {
                candidate_urls.push(s.read_url.clone());
            }
        }
        for p in &settings.public_syncs {
            if p.enabled && !p.read_url.trim().is_empty() {
                candidate_urls.push(p.read_url.clone());
            }
        }

        let mut seen = std::collections::HashSet::new();
        candidate_urls.retain(|u| seen.insert(u.clone()));
        candidate_urls
    };

    let now = chrono::Utc::now().timestamp().max(0) as u64;

    {
        if let Ok(neg_cache) = state.media_negative_cache.lock() {
            if let Some(&ts) = neg_cache.get(&blob_id) {
                if now.saturating_sub(ts) < 86_400 {
                    return Ok(None);
                }
            }
        }
    }

    if read_urls.is_empty() {
        return Ok(None);
    }

    let client = sync_client::create_sync_http_client();
    for read_url in &read_urls {
        if let Ok(Some(ciphertext)) = sync_client::get_media_blob(&client, read_url, &blob_id).await
        {
            if let Ok(media_bytes) = humoco_market::open_media_blob(&image_token, &ciphertext) {
                let cache_key = crate::state::cached_media_storage_key(&blob_id);
                if let Ok(mut service) = state.service.lock() {
                    let _ =
                        service.save_encrypted_data(&cache_key, &media_bytes, password.as_deref());
                }
                return Ok(Some(encode_media_data_uri(&media_bytes)));
            }
        }
    }

    if let Ok(mut neg_cache) = state.media_negative_cache.lock() {
        neg_cache.insert(blob_id, now);
    }

    Ok(None)
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
            let active_user_id = service
                .with_wallet(|w| w.get_user_id().to_string())
                .ok()
                .unwrap_or_default();
            if let Some(ref last_card) = stored.last_card {
                let active_ads_count =
                    stored.ads.iter().filter(|a| a.expires_at > now).count() as u8;
                let time_left = last_card.payload.valid_until.saturating_sub(now);
                let recipient_changed = !active_user_id.trim().is_empty()
                    && last_card.payload.recipient_id() != active_user_id.trim();
                if time_left < humoco_market::CARD_AUTO_RENEW_THRESHOLD_SECS
                    || last_card.payload.ad_count != active_ads_count
                    || recipient_changed
                {
                    if let Ok(()) = rebuild_and_sign_own_card(&mut service, stored, now) {
                        let _ = save_own_market_profile_state(
                            &mut service,
                            stored,
                            password.as_deref(),
                        );
                    }
                }
            }
        }
    }

    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password.as_deref()))
        .unwrap_or_default();

    let reg = state
        .get_or_load_wot_register(&mut service, password.as_deref())
        .ok();

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

    let known_contact_dids: std::collections::HashSet<String> = {
        let cached = state.contacts.lock().unwrap().clone();
        let ab_opt = cached.or_else(|| {
            service
                .load_encrypted_data(CONTACTS_DATA_NAME, password.as_deref())
                .ok()
                .and_then(|bytes| {
                    serde_json::from_slice::<crate::models::FrontendAddressBook>(&bytes).ok()
                })
        });
        ab_opt
            .map(|ab| {
                ab.contacts
                    .keys()
                    .map(|k| crate::state::canonicalize_did(k).unwrap_or_else(|_| k.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };

    let (raw_persons, own_did, wallet_owner_name) = service
        .with_wallet(|wallet| {
            let own_did = wallet.get_user_id().to_string();
            let prof = wallet.profile.to_public_profile();
            let name = [prof.first_name.as_deref(), prof.last_name.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            let owner_name = if !name.is_empty() {
                name
            } else if let Some(ref org) = prof.organization {
                org.clone()
            } else if let Some(ref com) = prof.community {
                com.clone()
            } else {
                "Ich".to_string()
            };

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
                    let can_did =
                        crate::state::canonicalize_did(did).unwrap_or_else(|_| did.to_string());
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

            let persons =
                humoco_market::build_market_view(vouchers, &cards_vec, &own_did, &classify, now);
            (persons, own_did, owner_name)
        })
        .map_err(FrontendError::from)?;

    let canonical_own_did = crate::state::canonicalize_did(&own_did).unwrap_or_else(|_| own_did.clone());

    let raw_board = humoco_market::build_ad_board(&raw_persons);

    let mut board: Vec<FrontendBoardItem> = raw_board
        .into_iter()
        .filter(|bi| settings.market_show_unknown || bi.trust == humoco_market::TrustTier::Trusted)
        .map(|bi| FrontendBoardItem::from_board_item(bi, now))
        .collect();

    if let Some(ref stored) = stored_opt {
        if stored.published {
            for ad in &stored.ads {
                if ad.expires_at > now {
                    let days_left = ad.expires_at.saturating_sub(now).div_ceil(86_400);
                    board.push(FrontendBoardItem {
                        owner_did: canonical_own_did.clone(),
                        recipient_did: own_did.clone(),
                        owner_name: wallet_owner_name.clone(),
                        trust: FrontendTrustTier::Trusted,
                        ad: ad.clone(),
                        days_left,
                        is_own: true,
                    });
                }
            }
        }
    }

    // Sort combined board vector descending by renewed_at / published_at
    board.sort_by(|a, b| {
        let time_a = a.ad.renewed_at.unwrap_or(a.ad.published_at);
        let time_b = b.ad.renewed_at.unwrap_or(b.ad.published_at);
        time_b.cmp(&time_a).then_with(|| a.ad.id.cmp(&b.ad.id))
    });

    let mut persons: Vec<FrontendMarketPerson> = raw_persons
        .into_iter()
        .filter(|p| {
            if p.trust == humoco_market::TrustTier::Hidden {
                return false;
            }
            if settings.market_show_unknown || p.trust == humoco_market::TrustTier::Trusted {
                return true;
            }
            let can_did = crate::state::canonicalize_did(&p.did).unwrap_or_else(|_| p.did.clone());
            known_contact_dids.contains(&can_did)
        })
        .map(FrontendMarketPerson::from)
        .collect();

    for person in persons.iter_mut() {
        let can_did =
            crate::state::canonicalize_did(&person.did).unwrap_or_else(|_| person.did.clone());
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
        stored_card_count: store.cards.len(),
        now,
    })
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

pub fn import_contact_card_impl(
    payload: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendImportCardResult, FrontendError> {
    let mut service = state.service.lock().unwrap();
    crate::services::common::ensure_access_mode(
        &mut service,
        human_money_core::AccessMode::DailyPay,
        password.as_deref(),
    )?;

    let trimmed_payload = payload.trim();
    let lower_payload = trimmed_payload.to_ascii_lowercase();
    let json_str = if let Some(pos) = lower_payload.find("?env=") {
        let query_part = &trimmed_payload[pos + 5..];
        let raw_b64 = query_part
            .split(|c: char| c == '&' || c.is_whitespace() || matches!(c, '"' | '\'' | '>' | '<'))
            .next()
            .unwrap_or(query_part);
        let b64 = raw_b64.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
        let bytes = human_money_core::crypto::decode_base64(b64).map_err(|e| {
            FrontendError::from(format!("Failed to decode card share URI base64: {}", e))
        })?;
        String::from_utf8(bytes)
            .map_err(|e| FrontendError::from(format!("Invalid UTF-8 in card share URI: {}", e)))?
    } else {
        payload
    };

    let json_trimmed = json_str.trim();
    if !json_trimmed.starts_with('{') {
        return Err(FrontendError::from(
            "Payload is not a valid CardEnvelope JSON".to_string(),
        ));
    }

    let env: humoco_market::CardEnvelope = serde_json::from_str(json_trimmed)
        .map_err(|e| FrontendError::from(format!("Failed to parse CardEnvelope: {}", e)))?;

    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password.as_deref()))
        .unwrap_or_default();
    let max_stored_cards = settings.max_stored_cards.clamp(100, 10_000);
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
            let own_coords = wallet
                .profile
                .coordinates
                .as_deref()
                .and_then(humoco_market::parse_coordinates_pair);
            humoco_market::accept_envelope(
                &mut store,
                &env,
                now,
                max_stored_cards,
                own_coords,
                &is_offender,
            )
        })
        .map_err(FrontendError::from)?;

    match outcome {
        humoco_market::AcceptOutcome::Accepted { did, replaced } => {
            let bytes = serde_json::to_vec(&store).map_err(|e| {
                FrontendError::from(format!("Failed to serialize CardStore: {}", e))
            })?;
            service
                .save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password.as_deref())
                .map_err(FrontendError::from)?;
            if let Some(stored) = store.cards.values().find(|sc| sc.card.payload.did == did) {
                let card_syncs: Vec<FrontendSyncDescriptor> = stored
                    .card
                    .payload
                    .drops
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect();

                let can_card_did =
                    crate::state::canonicalize_did(&did).unwrap_or_else(|_| did.clone());
                let own_ids: Option<(String, String)> = service
                    .with_wallet(|w| {
                        let uid = w.get_user_id().trim().to_string();
                        let can =
                            crate::state::canonicalize_did(&uid).unwrap_or_else(|_| uid.clone());
                        (uid, can)
                    })
                    .ok();
                let is_exact_own_profile = own_ids
                    .as_ref()
                    .is_some_and(|(own_uid, _)| own_uid == did.trim());

                if !is_exact_own_profile {
                    let is_own_root = own_ids
                        .as_ref()
                        .is_some_and(|(_, own_can)| *own_can == can_card_did);
                    let mut address_book = match service
                        .load_encrypted_data(CONTACTS_DATA_NAME, password.as_deref())
                    {
                        Ok(data) => {
                            serde_json::from_slice::<crate::models::FrontendAddressBook>(&data)
                                .unwrap_or_default()
                        }
                        Err(_) => crate::models::FrontendAddressBook::default(),
                    };

                    let exists = if is_own_root {
                        address_book.contacts.keys().any(|k| k.trim() == did.trim())
                    } else {
                        address_book.contacts.iter().any(|(k, _)| {
                            k == &did
                                || crate::state::canonicalize_did(k).ok().as_deref()
                                    == Some(&can_card_did)
                        })
                    };

                    if exists {
                        let card_profile = humoco_market::extract_public_profile(&stored.card);
                        if is_own_root {
                            if let Some(existing_contact) = address_book
                                .contacts
                                .values_mut()
                                .find(|c| c.did.trim() == did.trim())
                            {
                                let merged = crate::state::merge_contact_syncs(
                                    &existing_contact.syncs,
                                    &card_syncs,
                                );
                                let mut changed = false;
                                if existing_contact.syncs != merged {
                                    existing_contact.syncs = merged;
                                    changed = true;
                                }
                                if crate::state::apply_card_profile_to_contact(
                                    existing_contact,
                                    &card_profile,
                                ) {
                                    changed = true;
                                }
                                if changed {
                                    if let Ok(bytes) = serde_json::to_vec(&address_book) {
                                        let _ = service.save_encrypted_data(
                                            CONTACTS_DATA_NAME,
                                            &bytes,
                                            password.as_deref(),
                                        );
                                    }
                                    *state.contacts.lock().unwrap() = Some(address_book);
                                }
                            }
                        } else {
                            state.update_contact_from_card(
                                &mut service,
                                &did,
                                &card_syncs,
                                Some(&card_profile),
                                password.as_deref(),
                            );
                        }
                    }
                }
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

pub fn get_app_notices_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<StoredAppNotices, FrontendError> {
    let mut service = state.service.lock().unwrap();
    Ok(state.load_app_notices(&mut service, password.as_deref()))
}

pub fn mark_app_notices_read_impl(
    notice_id: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<StoredAppNotices, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let mut store = state.load_app_notices(&mut service, password.as_deref());
    let mut changed = false;
    for notice in &mut store.notices {
        let matches = notice_id
            .as_deref()
            .map(|id| notice.id == id)
            .unwrap_or(true);
        if matches && !notice.read {
            notice.read = true;
            changed = true;
        }
    }
    if changed {
        state.save_app_notices(&mut service, &store, password.as_deref())?;
    }
    Ok(store)
}

pub fn clear_app_notices_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<StoredAppNotices, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let mut store = state.load_app_notices(&mut service, password.as_deref());
    if !store.notices.is_empty() {
        store.notices.clear();
        state.save_app_notices(&mut service, &store, password.as_deref())?;
    }
    Ok(store)
}

// =========================================================================
//  TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        CardFieldVisibility, FieldAudience, FrontendAdInput, FrontendSaveMyMarketProfileRequest,
        FrontendUserProfile,
    };
    use human_money_core::app_service::AppService;
    use std::path::{Path, PathBuf};

    const ALICE_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const BOB_PHRASE: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
    const CHARLIE_PHRASE: &str =
        "legal winner thank year wave sausage worth useful legal winner thank yellow";

    fn temp_test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "humoco_market_test_{}_{}",
            name,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_test_app(dir: &Path, profile_name: &str, phrase: &str, password: &str) -> AppService {
        let mut service = AppService::new(dir).unwrap();
        service
            .create_profile(
                profile_name,
                phrase,
                None,
                Some("pc"),
                password,
                human_money_core::MnemonicLanguage::English,
                "device-market-test".into(),
                true,
            )
            .unwrap();
        service
    }

    fn create_test_state(service: AppService) -> AppState {
        AppState::new(service)
    }

    #[test]
    fn test_ad_duration_validation_and_renewals() {
        let base_dir = temp_test_dir("ad_duration_and_renewals");
        let password = "TestPassword123!";
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
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
            is_remote: false,
            image_webp_data_uri: None,
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
            is_remote: false,
            image_webp_data_uri: None,
        };
        let p1 = upsert_ad_impl(ad_30, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p1.ads.len(), 1);
        let ad_id = p1.ads[0].id.clone();
        let pub_at = p1.ads[0].published_at;
        assert_eq!(p1.ads[0].renewed_at, None);
        assert_eq!(p1.ads[0].status, "active");

        // Renew ad with 90 days
        let p2 =
            renew_ad_impl(ad_id.clone(), 90, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(
            p2.ads[0].published_at, pub_at,
            "published_at must stay unchanged"
        );
        assert!(p2.ads[0].renewed_at.is_some());
        assert!(p2.ads[0].expires_at > p1.ads[0].expires_at);

        // Renew ad with invalid 45 days rejected
        assert!(
            renew_ad_impl(ad_id.clone(), 45, Some(password.to_string()), &alice_state).is_err()
        );

        // Renew ad with 180 days succeeds
        let p3 = renew_ad_impl(ad_id, 180, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p3.ads[0].published_at, pub_at);
        assert!(p3.ads[0].expires_at > p2.ads[0].expires_at);
    }

    #[test]
    fn test_ad_limit_enforced_and_edit_preserves_timestamps() {
        let base_dir = temp_test_dir("ad_limits_and_edit");
        let password = "TestPassword123!";
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
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
                is_remote: false,
                image_webp_data_uri: None,
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
            is_remote: false,
            image_webp_data_uri: None,
        };
        let err = upsert_ad_impl(ad_6, Some(password.to_string()), &alice_state).unwrap_err();
        assert!(err.message.contains("market.adLimitReached"));

        // Editing an existing ad succeeds even when 5 ads are active
        let target_id = ad_ids[0].clone();
        let p_before =
            get_my_market_profile_impl(Some(password.to_string()), &alice_state).unwrap();
        let ad_before = p_before
            .ads
            .iter()
            .find(|a| a.id == target_id)
            .unwrap()
            .clone();

        let edit_input = FrontendAdInput {
            id: Some(target_id.clone()),
            kind: humoco_market::AdKind::Request,
            title: "Service 1 (Updated)".to_string(),
            text: Some("Updated description".to_string()),
            pricing: Some("15 Minuto".to_string()),
            duration_days: 90,
            allow_inquiries: false,
            is_remote: false,
            image_webp_data_uri: None,
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
            is_remote: false,
            image_webp_data_uri: None,
        };
        let p_new = upsert_ad_impl(ad_new, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p_new.ads.len(), 5);
    }

    #[tokio::test]
    async fn test_visibility_rules_and_sd_card_disclosures() {
        let base_dir = temp_test_dir("visibility_rules");
        let password = "TestPassword123!";
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
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
                is_remote: false,
            }],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        };

        save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state).unwrap();
        set_card_published_impl(true, false, Some(password.to_string()), &alice_state)
            .await
            .unwrap();

        let mut stored = {
            let mut s = alice_state.service.lock().unwrap();
            let bytes = s
                .load_encrypted_data(MARKET_OWN_PROFILE_KEY, Some(password))
                .unwrap();
            serde_json::from_slice::<StoredOwnMarketProfile>(&bytes).unwrap()
        };

        let now = chrono::Utc::now().timestamp().max(0) as u64;
        {
            let mut s = alice_state.service.lock().unwrap();
            rebuild_and_sign_own_card(&mut s, &mut stored, now).unwrap();
        }

        let card = stored.last_card.unwrap();

        // Distance 1 wire: Name (Everyone, max_hops=2) and Organization (Contacts, dist=1) revealed; Email omitted
        let wire_hop1 =
            humoco_market::render_wire(&card, 1, humoco_market::WireFilter::Full).unwrap();
        let verified_hop1 = humoco_market::verify_sd_card(&wire_hop1).unwrap();
        let profile_hop1 = humoco_market::extract_public_profile(&verified_hop1);

        assert_eq!(profile_hop1.first_name.as_deref(), Some("Alice"));
        assert_eq!(profile_hop1.last_name.as_deref(), Some("Smith"));
        assert_eq!(profile_hop1.organization.as_deref(), Some("Acme Corp"));
        assert_eq!(
            profile_hop1.email, None,
            "Email (Nobody) must never be disclosed"
        );

        // Distance 2 wire: Name revealed (Everyone), but Organization (Contacts only) is redacted!
        let wire_hop2 =
            humoco_market::render_wire(&card, 2, humoco_market::WireFilter::Full).unwrap();
        let verified_hop2 = humoco_market::verify_sd_card(&wire_hop2).unwrap();
        let profile_hop2 = humoco_market::extract_public_profile(&verified_hop2);

        assert_eq!(profile_hop2.first_name.as_deref(), Some("Alice"));
        assert_eq!(
            profile_hop2.organization, None,
            "Organization (Contacts) must be redacted at hop 2"
        );
        assert_eq!(profile_hop2.email, None);
    }

    #[test]
    fn test_service_offer_derivation_and_needs_preservation() {
        let base_dir = temp_test_dir("service_offer_and_needs");
        let password = "TestPassword123!";
        let mut alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);

        // Precondition: Set Core needs on wallet profile
        let mut p = alice_service
            .with_wallet(|w| w.profile.to_public_profile())
            .unwrap();
        p.needs = Some("Volunteers needed".to_string());
        alice_service
            .update_public_profile(p, Some(password))
            .unwrap();

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
                    is_remote: false,
                },
                humoco_market::Skill {
                    title: "Plumbing".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                },
            ],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        };

        save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state).unwrap();

        let s = alice_state.service.lock().unwrap();
        let core_profile = s.with_wallet(|w| w.profile.to_public_profile()).unwrap();

        // Rule 7: service_offer is derived from skills
        assert_eq!(
            core_profile.service_offer.as_deref(),
            Some("Carpentry, Plumbing")
        );
        // Rule 7: needs is preserved!
        assert_eq!(core_profile.needs.as_deref(), Some("Volunteers needed"));
    }

    #[tokio::test]
    async fn test_full_marketplace_flow_import_forward_and_board() {
        let base_dir = temp_test_dir("full_marketplace_flow");
        let password = "TestPassword123!";

        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let charlie_service = setup_test_app(
            &base_dir.join("charlie"),
            "Charlie",
            CHARLIE_PHRASE,
            password,
        );

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
                is_remote: false,
            }],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
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
            is_remote: false,
            image_webp_data_uri: None,
        };
        upsert_ad_impl(ad_req, Some(password.to_string()), &alice_state).unwrap();
        let alice_profile =
            set_card_published_impl(true, false, Some(password.to_string()), &alice_state)
                .await
                .unwrap();

        assert!(alice_profile.published);
        assert!(alice_profile.share_uri.is_some());
        let _share_uri = alice_profile.share_uri.unwrap();

        // Alice's own active ad must appear on her own board with is_own = true
        let alice_view = get_market_view_impl(Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(alice_view.board.len(), 1);
        assert!(alice_view.board[0].is_own);
        assert_eq!(alice_view.board[0].ad.title, "Fresh Baguettes");

        // 2. Bob imports Alice's full card via file envelope (since QR share_uri is 100% ad-free & image-free)
        let file_env_json = alice_profile.file_wire.expect("file_wire must be present");
        let import_res_bob =
            import_contact_card_impl(file_env_json, Some(password.to_string()), &bob_state)
                .unwrap();
        assert_eq!(import_res_bob.status, "accepted");

        // Bob checks market view and board
        let bob_view = get_market_view_impl(Some(password.to_string()), &bob_state).unwrap();
        assert_eq!(bob_view.people.len(), 1);
        assert_eq!(bob_view.people[0].did, alice_did);
        assert_eq!(
            bob_view.people[0].profile.first_name.as_deref(),
            Some("Alice")
        );

        assert_eq!(bob_view.board.len(), 1);
        assert_eq!(bob_view.board[0].owner_did, alice_did);
        assert_eq!(bob_view.board[0].ad.title, "Fresh Baguettes");
        assert!(!bob_view.board[0].is_own);

        // 3. Bob forwards Alice's card to Charlie (hops = 2)
        let bob_export =
            export_forward_card_impl(alice_did.clone(), Some(password.to_string()), &bob_state)
                .unwrap();
        let charlie_import = import_contact_card_impl(
            bob_export.envelope_json,
            Some(password.to_string()),
            &charlie_state,
        )
        .unwrap();
        assert_eq!(charlie_import.status, "accepted");

        let charlie_view =
            get_market_view_impl(Some(password.to_string()), &charlie_state).unwrap();
        assert_eq!(charlie_view.people.len(), 1);
        assert_eq!(charlie_view.people[0].did, alice_did);
        assert_eq!(
            charlie_view.people[0].sources[0],
            crate::models::FrontendEntrySource::Card { hops: 2 }
        );
        // Since max_hops = 2, Charlie cannot forward further
        assert!(!charlie_view.people[0].can_forward);
        assert!(export_forward_card_impl(
            alice_did.clone(),
            Some(password.to_string()),
            &charlie_state
        )
        .is_err());

        // 4. Bob deletes Alice's card from his store
        delete_foreign_contact_card_impl(alice_did.clone(), Some(password.to_string()), &bob_state)
            .unwrap();
        let bob_view_empty = get_market_view_impl(Some(password.to_string()), &bob_state).unwrap();
        assert!(bob_view_empty.people.is_empty());
        assert!(bob_view_empty.board.is_empty());
    }

    fn make_test_vp8_webp(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(30);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(22u32).to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8 ");
        bytes.extend_from_slice(&(10u32).to_le_bytes());
        bytes.push(0x00);
        bytes.extend_from_slice(&[0x00, 0x00]);
        bytes.extend_from_slice(&[0x9d, 0x01, 0x2a]);
        bytes.extend_from_slice(&(width & 0x3fff).to_le_bytes());
        bytes.extend_from_slice(&(height & 0x3fff).to_le_bytes());
        bytes
    }

    fn make_test_jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0xFF, 0xD8]);
        bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        bytes.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08]);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
        bytes.extend_from_slice(&[0xFF, 0xD9]);
        bytes
    }

    #[tokio::test]
    async fn test_market_media_workflow() {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;

        let base_dir = temp_test_dir("market_media_workflow");
        let password = "TestPassword123!";
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        // 1. Initial profile check
        let initial_profile =
            get_my_market_profile_impl(Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(
            initial_profile.media_max_bytes,
            humoco_market::MAX_AD_MEDIA_BYTES
        );
        assert_eq!(
            initial_profile.media_max_count,
            humoco_market::MAX_ADS_PER_CARD
        );
        assert_eq!(initial_profile.media_used_count, 0);

        // 2. Avatar validation in save_my_market_profile_impl
        let bad_avatar_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                picture_url: Some(format!(
                    "data:image/webp;base64,{}",
                    STANDARD.encode(make_test_vp8_webp(120, 120))
                )),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        };
        assert!(save_my_market_profile_impl(
            bad_avatar_req,
            Some(password.to_string()),
            &alice_state
        )
        .is_err());

        let good_avatar_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                picture_url: Some(format!(
                    "data:image/webp;base64,{}",
                    STANDARD.encode(make_test_vp8_webp(96, 96))
                )),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        };
        let saved_profile =
            save_my_market_profile_impl(good_avatar_req, Some(password.to_string()), &alice_state)
                .unwrap();
        assert!(saved_profile.profile.picture_url.is_some());

        // Also test JPEG avatar (WebKitGTK fallback)
        let good_jpeg_avatar_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                picture_url: Some(format!(
                    "data:image/jpeg;base64,{}",
                    STANDARD.encode(make_test_jpeg(96, 96))
                )),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        };
        let saved_jpeg_profile = save_my_market_profile_impl(
            good_jpeg_avatar_req,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert!(saved_jpeg_profile
            .profile
            .picture_url
            .as_ref()
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));

        // 3. Create ad with WebP image
        let webp_ad = make_test_vp8_webp(400, 300);
        let ad_data_uri = format!("data:image/webp;base64,{}", STANDARD.encode(&webp_ad));

        let ad_with_img = FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Handmade Pottery".to_string(),
            text: Some("High quality clay".to_string()),
            pricing: Some("15 M".to_string()),
            duration_days: 90,
            allow_inquiries: true,
            is_remote: false,
            image_webp_data_uri: Some(ad_data_uri.clone()),
        };
        let p_after_ad =
            upsert_ad_impl(ad_with_img, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p_after_ad.media_used_count, 1);
        let ad_id = p_after_ad.ads[0].id.clone();
        let token = p_after_ad.ads[0]
            .image_token
            .clone()
            .expect("Must have image_token");
        assert!(humoco_market::is_valid_image_token(&token));

        // Verify own image retrieval via get_ad_image_impl
        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let retrieved_img = get_ad_image_impl(
            alice_did.clone(),
            token.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(retrieved_img, Some(ad_data_uri.clone()));

        // 4. Edit ad without changing image (image_webp_data_uri: None) -> image_token preserved
        let edit_no_img_change = FrontendAdInput {
            id: Some(ad_id.clone()),
            kind: humoco_market::AdKind::Offer,
            title: "Handmade Pottery (Refined)".to_string(),
            text: Some("High quality clay".to_string()),
            pricing: Some("15 M".to_string()),
            duration_days: 90,
            allow_inquiries: true,
            is_remote: false,
            image_webp_data_uri: None,
        };
        let p_edit =
            upsert_ad_impl(edit_no_img_change, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p_edit.ads[0].image_token, Some(token.clone()));
        assert_eq!(p_edit.media_used_count, 1);

        // 5. Edit ad removing image (image_webp_data_uri: Some("")) -> image_token becomes None
        let edit_remove_img = FrontendAdInput {
            id: Some(ad_id.clone()),
            kind: humoco_market::AdKind::Offer,
            title: "Handmade Pottery (No Photo)".to_string(),
            text: Some("High quality clay".to_string()),
            pricing: Some("15 M".to_string()),
            duration_days: 90,
            allow_inquiries: true,
            is_remote: false,
            image_webp_data_uri: Some("".to_string()),
        };
        let p_removed =
            upsert_ad_impl(edit_remove_img, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p_removed.ads[0].image_token, None);
        assert_eq!(p_removed.media_used_count, 0);

        // Also test ad with JPEG image (WebKitGTK fallback)
        let jpeg_ad = make_test_jpeg(400, 300);
        let jpeg_ad_data_uri = format!("data:image/jpeg;base64,{}", STANDARD.encode(&jpeg_ad));
        let ad_with_jpeg = FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Vintage Camera".to_string(),
            text: Some("Fully functional".to_string()),
            pricing: Some("50 M".to_string()),
            duration_days: 30,
            allow_inquiries: true,
            is_remote: true,
            image_webp_data_uri: Some(jpeg_ad_data_uri.clone()),
        };
        let p_after_jpeg_ad =
            upsert_ad_impl(ad_with_jpeg, Some(password.to_string()), &alice_state).unwrap();
        let jpeg_token = p_after_jpeg_ad.ads[1]
            .image_token
            .clone()
            .expect("Must have jpeg image_token");
        let retrieved_jpeg_img = get_ad_image_impl(
            alice_did.clone(),
            jpeg_token,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(retrieved_jpeg_img, Some(jpeg_ad_data_uri));

        // 6. Test negative caching and unknown image tokens in get_ad_image_impl
        // Invalid token
        let invalid_token_res = get_ad_image_impl(
            alice_did.clone(),
            "invalid_token".to_string(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(invalid_token_res, None);

        // Valid syntax token but unknown and drops configured -> tries network, fails, inserts into negative cache
        {
            let mut s = alice_state.settings.lock().unwrap();
            *s = Some(crate::settings::AppSettings {
                market_selected_syncs: vec![FrontendSyncDescriptor {
                    write_url: "http://127.0.0.1:9999".to_string(),
                    read_url: "http://127.0.0.1:9999".to_string(),
                    slot_id: None,
                    drop_pub: "0".repeat(64),
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                }],
                ..Default::default()
            });
        }

        let unknown_token = "0123456789abcdef0123456789abcdef".to_string();
        let unknown_res = get_ad_image_impl(
            alice_did.clone(),
            unknown_token.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(unknown_res, None);

        // Negative cache must contain blob_id
        let blob_id = humoco_market::derive_media_blob_id(&unknown_token).unwrap();
        assert!(alice_state
            .media_negative_cache
            .lock()
            .unwrap()
            .contains_key(&blob_id));

        // Subsequent call hits negative cache immediately
        let hit_neg_cache = get_ad_image_impl(
            alice_did.clone(),
            unknown_token.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(hit_neg_cache, None);
    }

    #[tokio::test]
    async fn test_rebuild_and_sign_own_card_with_empty_or_none_name() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_empty_name_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let mut alice_service = crate::test_support::setup_test_app(
            &temp_dir,
            "Initial Name",
            crate::test_support::ALICE_PHRASE,
            password,
        );

        let mut public_profile = alice_service
            .with_wallet(|w| w.profile.to_public_profile())
            .unwrap();
        public_profile.first_name = Some("".to_string());
        public_profile.last_name = Some("   ".to_string());
        alice_service
            .update_public_profile(public_profile, Some(password))
            .unwrap();

        let alice_state = crate::test_support::create_test_state(alice_service);

        let res =
            set_card_published_impl(true, false, Some(password.to_string()), &alice_state).await;
        assert!(
            res.is_ok(),
            "Expected success publishing card with empty name, got: {:?}",
            res.err()
        );
        let prof = res.unwrap();
        assert!(prof.published);
    }

    #[test]
    fn test_save_profile_p8_privacy_guardrails_and_inquiry_sync() {
        let base_dir = temp_test_dir("p8_guardrails");
        let password = "TestPassword123!";
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        // Create an ad first with allow_inquiries = true
        let ad_input = FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Neighborhood Bike Help".to_string(),
            text: None,
            pricing: None,
            duration_days: 90,
            allow_inquiries: true,
            is_remote: false,
            image_webp_data_uri: None,
        };
        let p_with_ad = upsert_ad_impl(ad_input, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(p_with_ad.ads.len(), 1);
        assert!(p_with_ad.ads[0].allow_inquiries);

        // Save profile attempting street=Everyone, public fine coords, and allow_inquiries=false
        let save_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                coordinates: Some("48.1351, 11.5820".to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility {
                street: FieldAudience::Everyone,
                coordinates: FieldAudience::Everyone,
                ..Default::default()
            },
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: false,
            published: None,
        };

        let saved = save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state)
            .unwrap();
        assert_eq!(
            saved.visibility.street,
            FieldAudience::Contacts,
            "street must never be public"
        );
        assert_eq!(
            saved.profile.coordinates.as_deref(),
            Some("48.14, 11.58"),
            "public coordinates must be coarsened to 2 decimal places"
        );
        assert!(!saved.allow_inquiries);
        assert!(
            !saved.ads[0].allow_inquiries,
            "profile allow_inquiries must propagate to existing ads"
        );

        // When coordinates visibility is contacts-only, finer coordinates are preserved
        let save_contacts_coords = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                coordinates: Some("48.1351, 11.5820".to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility {
                street: FieldAudience::Contacts,
                coordinates: FieldAudience::Contacts,
                ..Default::default()
            },
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        };

        let saved_contacts = save_my_market_profile_impl(
            save_contacts_coords,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert_eq!(
            saved_contacts.profile.coordinates.as_deref(),
            Some("48.1351, 11.5820")
        );
        assert!(saved_contacts.ads[0].allow_inquiries);
    }

    #[test]
    fn test_import_contact_card_uri_with_surrounding_chat_text() {
        let base_dir = temp_test_dir("card_uri_chat_text");
        let password = "TestPassword123!";
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        let env_json = serde_json::to_string(&humoco_market::CardEnvelope {
            card_wire: "not-a-real-jws".to_string(),
            hops: 1,
        })
        .unwrap();
        let b64 = human_money_core::crypto::encode_base64(env_json.as_bytes());
        let chat_msg = format!(
            "Hallo Alice,\nhier ist meine Visitenkarte:\n\"humoco://card?env={}\"\nViele Grüße, Bob!",
            b64
        );

        let res = import_contact_card_impl(chat_msg, Some(password.to_string()), &alice_state)
            .expect("URI extraction and CardEnvelope JSON parsing should succeed");
        assert!(
            res.status.starts_with("invalid:"),
            "expected JWS validation error status after successful URI/JSON parse, got: {}",
            res.status
        );
    }
}
