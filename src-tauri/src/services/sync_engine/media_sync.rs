use std::collections::{HashMap, HashSet};

use human_money_core::app_service::AppService;

use crate::models::StoredOwnMarketProfile;
use crate::services::common::{normalize_sync_url, DropChannelSigner, SyncIdentityContext};
use crate::session::SessionToken;
use crate::state::{own_media_storage_key, AppState, StoredSyncState};
use crate::sync_client::{self, PutSlotOutcome};

/// Encrypts active published ad media attachments into sealed blobs keyed by `blob_id`.
pub(crate) fn seal_active_ad_media_blobs(
    service: &mut AppService,
    active_own_card: Option<&humoco_market::SdCard>,
    own_profile: Option<&StoredOwnMarketProfile>,
    now_secs: u64,
    password: Option<&str>,
) -> Result<Vec<(String, Vec<u8>)>, crate::models::FrontendError> {
    let (Some(own_card), Some(prof)) = (active_own_card, own_profile) else {
        return Ok(Vec::new());
    };
    if !prof.published {
        return Ok(Vec::new());
    }

    let mut blobs = Vec::new();
    for entry in &prof.ads {
        if !entry.is_published_active(now_secs) {
            continue;
        }
        let Some(ref token) = entry.ad.image_token else {
            continue;
        };
        let storage_key = own_media_storage_key(token);
        if let Some(webp_bytes) =
            crate::state::load_encrypted_bytes_opt(service, &storage_key, password)
                .map_err(crate::models::FrontendError::from)?
        {
            if let Ok((tok, blob_id, ciphertext)) =
                humoco_market::seal_media_blob(&own_card.payload.did, &webp_bytes)
            {
                if tok == *token {
                    blobs.push((blob_id, ciphertext));
                } else {
                    log::warn!(
                        "Skipping ad media blob whose derived token {} mismatches ad.image_token {}",
                        tok,
                        token
                    );
                }
            }
        }
    }
    Ok(blobs)
}

/// Uploads active ad media blobs to a single drop target, skipping already uploaded blobs unless forced.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn upload_media_blobs_to_target(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    id_ctx: &SyncIdentityContext,
    drop_desc: &humoco_market::DropDescriptor,
    active_media_blobs: &[(String, Vec<u8>)],
    force_upload: bool,
    now_secs: u64,
    sync_state: &mut StoredSyncState,
    errors: &mut super::SyncErrorTracker,
) {
    let norm_write_url = normalize_sync_url(&drop_desc.write_url);
    if norm_write_url.is_empty() {
        return;
    }

    for (blob_id, ciphertext) in active_media_blobs {
        if !state.is_generation_current(session.generation) {
            errors.push("Active wallet session changed before media upload completed".to_string());
            break;
        }
        if state.is_origin_cooling_down(&drop_desc.write_url, std::time::Instant::now()) {
            errors.push(format!(
                "Media upload to {} skipped: origin cooling down after HTTP 429",
                drop_desc.write_url
            ));
            break;
        }

        let upload_key = format!("{}::{}", norm_write_url, blob_id);
        if !force_upload && sync_state.uploaded_media_blobs.contains(&upload_key) {
            continue;
        }

        let media_ch = format!("media:{}", blob_id);
        let signer = DropChannelSigner {
            is_public: false,
            master_seed: &id_ctx.master_seed,
            my_ed_sk: &id_ctx.my_ed_sk,
            drop_desc,
        };
        match signer.sign_request(
            "PUT",
            &media_ch,
            None,
            now_secs,
            &sync_client::generate_sync_nonce(),
            ciphertext,
        ) {
            Ok(signed) => match sync_client::put_media_blob(
                client,
                &drop_desc.write_url,
                &signed,
                blob_id,
                ciphertext,
            )
            .await
            {
                Ok(PutSlotOutcome::Saved { .. }) => {
                    sync_state.uploaded_media_blobs.insert(upload_key);
                }
                Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                    let msg = format!(
                        "Media upload to {} precondition failed",
                        drop_desc.write_url
                    );
                    log::warn!("{}", msg);
                    errors.push(msg);
                }
                Err(e) => {
                    let msg = format!("Media upload to {} error: {}", drop_desc.write_url, e);
                    log::warn!("{}", msg);
                    errors.record_sync_error(state, &drop_desc.write_url, &e, msg);
                }
            },
            Err(e) => {
                let msg = format!(
                    "Media request signing for {} error: {}",
                    drop_desc.write_url, e
                );
                log::warn!("{}", msg);
                errors.push(msg);
            }
        }
    }
}

/// Deletes orphaned remote media blobs no longer referenced by active published ads.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn gc_orphaned_media_blobs(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    id_ctx: &SyncIdentityContext,
    upload_targets: &[humoco_market::DropDescriptor],
    active_media_blob_ids: &HashSet<String>,
    now_secs: u64,
    sync_state: &mut StoredSyncState,
    errors: &mut super::SyncErrorTracker,
) {
    if sync_state.uploaded_media_blobs.is_empty() || upload_targets.is_empty() {
        return;
    }

    let mut targets_by_norm_write: HashMap<String, humoco_market::DropDescriptor> = HashMap::new();
    for drop_desc in upload_targets {
        let norm_write = normalize_sync_url(&drop_desc.write_url);
        if !norm_write.is_empty() {
            targets_by_norm_write
                .entry(norm_write)
                .or_insert_with(|| drop_desc.clone());
        }
    }

    let mut stale_media_deletes: Vec<(humoco_market::DropDescriptor, String, String)> = Vec::new();
    for key in &sync_state.uploaded_media_blobs {
        let Some((stored_url, blob_id)) = key.split_once("::") else {
            continue;
        };
        let norm_stored_url = normalize_sync_url(stored_url);
        if let Some(drop_desc) = targets_by_norm_write.get(&norm_stored_url) {
            if !active_media_blob_ids.contains(blob_id) {
                stale_media_deletes.push((drop_desc.clone(), blob_id.to_string(), key.clone()));
            }
        }
    }

    for (drop_desc, blob_id, key) in stale_media_deletes {
        if !state.is_generation_current(session.generation) {
            errors.push(
                "Active wallet session changed before orphaned media GC completed".to_string(),
            );
            break;
        }
        if state.is_origin_cooling_down(&drop_desc.write_url, std::time::Instant::now()) {
            errors.push(format!(
                "Orphaned media DELETE on {} deferred: origin cooling down after HTTP 429",
                drop_desc.write_url
            ));
            continue;
        }

        let media_ch = format!("media:{}", blob_id);
        let signer = DropChannelSigner {
            is_public: false,
            master_seed: &id_ctx.master_seed,
            my_ed_sk: &id_ctx.my_ed_sk,
            drop_desc: &drop_desc,
        };
        match signer.sign_request(
            "DELETE",
            &media_ch,
            None,
            now_secs,
            &sync_client::generate_sync_nonce(),
            b"",
        ) {
            Ok(signed) => match sync_client::delete_media_blob(
                client,
                &drop_desc.write_url,
                &signed,
                Some(&blob_id),
            )
            .await
            {
                Ok(_) => {
                    sync_state.uploaded_media_blobs.remove(&key);
                }
                Err(e) => {
                    errors.record_sync_error(
                        state,
                        &drop_desc.write_url,
                        &e,
                        format!(
                            "Orphaned media DELETE on {} failed: {}",
                            drop_desc.write_url, e
                        ),
                    );
                }
            },
            Err(e) => {
                errors.push(format!(
                    "Orphaned media DELETE signing on {} failed: {}",
                    drop_desc.write_url, e
                ));
            }
        }
    }
}
