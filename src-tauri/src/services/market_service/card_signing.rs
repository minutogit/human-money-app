use super::storage::*;
use crate::models::{FieldAudience, FrontendError, FrontendSyncDescriptor, StoredOwnMarketProfile};
use crate::settings::StoredPublicSync;
use crate::state::AppState;
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
    use crate::services::sync_endpoints::{
        canonical_drop_pub_hex, is_endpoint_revoked, normalize_sync_url,
    };

    let mut seen_urls = std::collections::HashSet::new();
    let mut drops: Vec<humoco_market::DropDescriptor> = Vec::new();

    for d in selected_syncs {
        if is_endpoint_revoked(&d.status) {
            continue;
        }
        let write_trimmed = d.write_url.trim();
        let read_trimmed = d.read_url.trim();
        if sync_client::validate_sync_url(write_trimmed).is_err()
            || sync_client::validate_sync_read_url(read_trimmed).is_err()
        {
            continue;
        }
        let Ok(clean_drop_pub) = canonical_drop_pub_hex(&d.drop_pub) else {
            continue;
        };
        let slot_id = match d
            .slot_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(sid) => {
                if humoco_market::parse_slot_id(sid).is_err() {
                    continue;
                }
                Some(sid.to_string())
            }
            None => None,
        };
        let norm = normalize_sync_url(write_trimmed);
        if norm.is_empty() || !seen_urls.insert(norm) {
            continue;
        }
        drops.push(humoco_market::DropDescriptor {
            write_url: write_trimmed.to_string(),
            read_url: read_trimmed.to_string(),
            slot_id,
            drop_pub: clean_drop_pub,
        });
    }

    for pd in public_syncs {
        if !pd.enabled || is_endpoint_revoked(&pd.status) {
            continue;
        }
        let write_trimmed = pd.write_url.trim();
        let read_trimmed = pd.read_url.trim();
        if sync_client::validate_sync_url(write_trimmed).is_err()
            || sync_client::validate_sync_read_url(read_trimmed).is_err()
        {
            continue;
        }
        let Ok(clean_drop_pub) = canonical_drop_pub_hex(&pd.drop_pub) else {
            continue;
        };
        let Ok(clean_access_pk) = canonical_drop_pub_hex(&pd.access_pubkey) else {
            continue;
        };
        let Ok(slot_id) = humoco_market::format_slot_id(&clean_access_pk, my_prefix) else {
            continue;
        };
        let norm = normalize_sync_url(write_trimmed);
        if norm.is_empty() || !seen_urls.insert(norm) {
            continue;
        }
        drops.push(humoco_market::DropDescriptor {
            write_url: write_trimmed.to_string(),
            read_url: read_trimmed.to_string(),
            slot_id: Some(slot_id),
            drop_pub: clean_drop_pub,
        });
    }

    drops.truncate(humoco_market::MAX_CARD_DROPS);
    drops
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
    let card_did = prev.payload.recipient_id().to_string();

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
        did: card_did,
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
pub(crate) fn bump_own_card_version_for_eviction(
    state: &AppState,
    service: &mut AppService,
    password: Option<&str>,
) {
    let mut stored = match load_own_market_profile_for_write(service, password) {
        Ok(s) => s,
        Err(_) => return,
    };
    if stored.last_card.is_none() {
        return;
    }
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let res = if stored.published {
        let Ok(drops) = load_drops_from_settings(state, service, None, password) else {
            return;
        };
        rebuild_and_sign_own_card_with_drops(service, &mut stored, now, Some(drops))
    } else {
        sign_unpublish_tombstone_card(service, &mut stored, now)
    };
    if res.is_ok() {
        let _ = save_own_market_profile_state(state, service, &stored, password);
    }
}

pub(crate) fn prepare_own_card_max_hops_for_write(
    state: &AppState,
    service: &mut AppService,
    max_hops: u8,
    bump_for_eviction: bool,
    password: Option<&str>,
) -> Result<StoredOwnMarketProfile, FrontendError> {
    service
        .require_access_mode(human_money_core::AccessMode::DailyPay, password)
        .map_err(FrontendError::from)?;
    let clamped = max_hops.clamp(1, 3);
    let mut stored = load_own_market_profile_for_write(service, password)?;

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    update_shrink_window(&mut stored, clamped, now);
    stored.max_hops = clamped;
    if stored.published {
        let drops = load_drops_from_settings(state, service, None, password)?;
        rebuild_and_sign_own_card_with_drops(service, &mut stored, now, Some(drops))?;
    } else if bump_for_eviction && stored.last_card.is_some() {
        sign_unpublish_tombstone_card(service, &mut stored, now)?;
    }

    Ok(stored)
}

#[allow(dead_code)]
pub(crate) fn set_own_card_max_hops_inner(
    state: &AppState,
    service: &mut AppService,
    max_hops: u8,
    password: Option<&str>,
) -> Result<u8, FrontendError> {
    let stored = prepare_own_card_max_hops_for_write(state, service, max_hops, false, password)?;
    let max_hops_out = stored.max_hops;
    save_own_market_profile_state(state, service, &stored, password)?;
    Ok(max_hops_out)
}

pub fn sync_own_card_drops_on_settings_change(
    state: &AppState,
    service: &mut AppService,
    syncs: &[FrontendSyncDescriptor],
    password: Option<&str>,
) {
    if let Ok(mut existing) = load_own_market_profile_for_write(service, password) {
        if existing.published {
            let now_unix = chrono::Utc::now().timestamp().max(0) as u64;
            let Ok(drops) = load_drops_from_settings(state, service, Some(syncs), password) else {
                return;
            };
            let _ =
                rebuild_and_sign_own_card_with_drops(service, &mut existing, now_unix, Some(drops));
            let _ = save_own_market_profile_state(state, service, &existing, password);
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
    rebuild_and_sign_own_card_with_drops_and_profile(service, stored, now, override_drops, None)
}

pub(crate) fn rebuild_and_sign_own_card_with_drops_and_profile(
    service: &mut AppService,
    stored: &mut StoredOwnMarketProfile,
    now: u64,
    override_drops: Option<Vec<humoco_market::DropDescriptor>>,
    draft_profile: Option<&human_money_core::models::profile::PublicProfile>,
) -> Result<(), FrontendError> {
    let (my_identity, my_signing_key) = service
        .with_wallet_and_identity(|_w, id| (id.clone(), id.signing_key.clone()))
        .map_err(FrontendError::from)?;
    let my_account_id = humoco_market::canonicalize_owner_account_id(&my_identity.user_id)
        .map_err(FrontendError::from)?;

    let profile = match draft_profile {
        Some(p) => p.clone(),
        None => service
            .with_wallet(|w| w.profile.to_public_profile())
            .map_err(FrontendError::from)?,
    };

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

    // Active published Ads (local drafts remain excluded from signed card)
    let mut active_ad_count = 0u8;
    for entry in &stored.ads {
        if entry.is_published_active(now) {
            items.push((humoco_market::CardItem::Ad(entry.ad.clone()), max_hops));
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
