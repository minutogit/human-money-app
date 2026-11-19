use std::collections::HashSet;

use human_money_core::app_service::AppService;

use crate::models::StoredOwnMarketProfile;
use crate::services::common::{DropChannelSigner, SyncIdentityContext};
use crate::state::{own_media_storage_key, StoredSyncState};
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
    for ad in &prof.ads {
        if ad.expires_at <= now_secs {
            continue;
        }
        let Some(ref token) = ad.image_token else {
            continue;
        };
        let storage_key = own_media_storage_key(token);
        if let Some(webp_bytes) = crate::state::load_encrypted_bytes_opt(service, &storage_key, password)
            .map_err(crate::models::FrontendError::from)?
        {
            if let Ok((_tok, blob_id, ciphertext)) =
                humoco_market::seal_media_blob(&own_card.payload.did, &webp_bytes)
            {
                blobs.push((blob_id, ciphertext));
            }
        }
    }
    Ok(blobs)
}

/// Uploads active ad media blobs to a single drop target, skipping already uploaded blobs unless forced.
pub(crate) async fn upload_media_blobs_to_target(
    client: &reqwest::Client,
    id_ctx: &SyncIdentityContext,
    drop_desc: &humoco_market::DropDescriptor,
    active_media_blobs: &[(String, Vec<u8>)],
    force_upload: bool,
    now_secs: u64,
    sync_state: &mut StoredSyncState,
) {
    for (blob_id, ciphertext) in active_media_blobs {
        let upload_key = format!("{}::{}", drop_desc.write_url, blob_id);
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
                    log::warn!(
                        "Media upload to {} precondition failed",
                        drop_desc.write_url
                    );
                }
                Err(e) => {
                    log::warn!(
                        "Media upload to {} non-fatal error: {}",
                        drop_desc.write_url,
                        e
                    );
                }
            },
            Err(e) => {
                log::warn!(
                    "Media request signing for {} non-fatal error: {}",
                    drop_desc.write_url,
                    e
                );
            }
        }
    }
}

/// Deletes orphaned remote media blobs no longer referenced by active published ads.
pub(crate) async fn gc_orphaned_media_blobs(
    client: &reqwest::Client,
    id_ctx: &SyncIdentityContext,
    upload_targets: &[humoco_market::DropDescriptor],
    active_media_blob_ids: &HashSet<String>,
    now_secs: u64,
    sync_state: &mut StoredSyncState,
) {
    if sync_state.uploaded_media_blobs.is_empty() || upload_targets.is_empty() {
        return;
    }

    let mut stale_media_deletes: Vec<(humoco_market::DropDescriptor, String, String)> = Vec::new();
    for drop_desc in upload_targets {
        if drop_desc.write_url.is_empty() {
            continue;
        }
        let prefix = format!("{}::", drop_desc.write_url);
        for key in &sync_state.uploaded_media_blobs {
            if let Some(blob_id) = key.strip_prefix(&prefix) {
                if !active_media_blob_ids.contains(blob_id) {
                    stale_media_deletes.push((drop_desc.clone(), blob_id.to_string(), key.clone()));
                }
            }
        }
    }

    for (drop_desc, blob_id, key) in stale_media_deletes {
        let media_ch = format!("media:{}", blob_id);
        let signer = DropChannelSigner {
            is_public: false,
            master_seed: &id_ctx.master_seed,
            my_ed_sk: &id_ctx.my_ed_sk,
            drop_desc: &drop_desc,
        };
        if let Ok(signed) = signer.sign_request(
            "DELETE",
            &media_ch,
            None,
            now_secs,
            &sync_client::generate_sync_nonce(),
            b"",
        ) {
            if sync_client::delete_media_blob(client, &drop_desc.write_url, &signed, Some(&blob_id))
                .await
                .is_ok()
            {
                sync_state.uploaded_media_blobs.remove(&key);
            }
        }
    }
}
