use super::storage::*;
use crate::models::FrontendError;
use crate::state::AppState;
use crate::sync_client;

pub(super) enum AdImageLookup {
    Cached(String),
    RemoteUrls(Vec<String>),
}

pub(super) const MEDIA_NEG_CACHE_PERMANENT_TTL_SECS: u64 = 86_400;
pub(super) const MEDIA_NEG_CACHE_TRANSIENT_TTL_SECS: u64 = 60;

pub async fn get_ad_image_impl(
    owner_did: String,
    image_token: String,
    password: Option<String>,
    state: &AppState,
) -> Result<Option<String>, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::Unlocked,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    if !humoco_market::is_valid_image_token(&image_token) {
        return Ok(None);
    }

    let blob_id = match humoco_market::derive_media_blob_id(&image_token) {
        Ok(id) => id,
        Err(_) => return Ok(None),
    };

    let own_hit = state.with_session(&session, |service| {
        if !(service.is_session_active() || pw.is_some()) {
            return Ok(None);
        }
        let own_key = crate::state::own_media_storage_key(&image_token);
        try_load_cached_media_bytes(service, &own_key, false, pw)
            .map(|opt| opt.map(|bytes| encode_media_data_uri(&bytes)))
    })?;
    if let Some(Some(uri)) = own_hit {
        return Ok(Some(uri));
    }

    let validated_owner = human_money_core::ValidatedUserId::parse(&owner_did)
        .map_err(|e| FrontendError::from(format!("Invalid owner DID: {}", e)))?;
    let canonical_owner = validated_owner.into_string();

    let lookup = match state.with_session(&session, |service| {
        let can_decrypt_storage = service.is_session_active() || pw.is_some();
        if can_decrypt_storage {
            let cache_key = crate::state::cached_media_storage_key(&blob_id);
            if let Some(bytes) = try_load_cached_media_bytes(service, &cache_key, true, pw)? {
                if humoco_market::compute_image_token(&canonical_owner, &bytes)
                    .ok()
                    .as_deref()
                    == Some(image_token.as_str())
                {
                    return Ok(AdImageLookup::Cached(encode_media_data_uri(&bytes)));
                }
            }
        }

        let mut candidate_urls = Vec::new();

        let card_store = state
            .load_card_store_or_cached(service, pw)
            .map_err(FrontendError::from)?;
        if let Some(stored_card) = card_store.cards.get(canonical_owner.as_str()) {
            for d in &stored_card.card.payload.drops {
                if !d.read_url.trim().is_empty() {
                    candidate_urls.push(d.read_url.clone());
                }
            }
        } else if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(&canonical_owner) {
            for stored_card in card_store.cards.values() {
                if human_money_core::crypto::get_pubkey_from_user_id(&stored_card.card.payload.did)
                    .ok()
                    .as_ref()
                    == Some(&pk)
                {
                    for d in &stored_card.card.payload.drops {
                        if !d.read_url.trim().is_empty() {
                            candidate_urls.push(d.read_url.clone());
                        }
                    }
                }
            }
        }

        let security_policy = state
            .get_or_load_security_policy(service, pw)
            .map_err(FrontendError::from)?;

        for s in &security_policy.market_selected_syncs {
            if !s.read_url.trim().is_empty() {
                candidate_urls.push(s.read_url.clone());
            }
        }
        for p in &security_policy.public_syncs {
            if p.enabled && !p.read_url.trim().is_empty() {
                candidate_urls.push(p.read_url.clone());
            }
        }

        let mut seen = std::collections::HashSet::new();
        candidate_urls.retain(|u| seen.insert(u.clone()));
        Ok(AdImageLookup::RemoteUrls(candidate_urls))
    })? {
        Some(res) => res,
        None => return Ok(None),
    };

    let read_urls = match lookup {
        AdImageLookup::Cached(uri) => return Ok(Some(uri)),
        AdImageLookup::RemoteUrls(urls) => urls,
    };

    if read_urls.is_empty() {
        return Ok(None);
    }

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let client = sync_client::create_sync_http_client();
    for read_url in &read_urls {
        if state.is_media_negatively_cached(&session, &blob_id, read_url, now) {
            continue;
        }
        let ttl_secs = match sync_client::get_media_blob(&client, read_url, &blob_id).await {
            Ok(Some(ciphertext)) => {
                if let Ok(media_bytes) =
                    humoco_market::open_media_blob(&canonical_owner, &image_token, &ciphertext)
                {
                    let cache_key = crate::state::cached_media_storage_key(&blob_id);
                    let session_valid = state
                        .with_session(&session, |service| {
                            if service.is_session_active() || pw.is_some() {
                                let _ = crate::state::save_local_sidecar_bytes(
                                    service,
                                    &cache_key,
                                    &media_bytes,
                                    pw,
                                );
                            }
                            Ok(())
                        })?
                        .is_some();
                    if !session_valid {
                        return Ok(None);
                    }
                    return Ok(Some(encode_media_data_uri(&media_bytes)));
                }
                MEDIA_NEG_CACHE_PERMANENT_TTL_SECS
            }
            Ok(None) => MEDIA_NEG_CACHE_PERMANENT_TTL_SECS,
            Err(_) => MEDIA_NEG_CACHE_TRANSIENT_TTL_SECS,
        };
        state.record_media_negative_cache(
            &session,
            blob_id.clone(),
            read_url.clone(),
            now,
            ttl_secs,
        );
    }

    Ok(None)
}
