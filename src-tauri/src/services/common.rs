use crate::models::FrontendError;
use crate::services::fs_helpers::collect_custom_standards;
use crate::settings::AppSettings;
use crate::AppState;
use human_money_core::app_service::AppService;
use human_money_core::crypto::{get_prefix_from_user_id, get_pubkey_from_user_id};
use human_money_core::humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS;
use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
use human_money_core::FileVoucherArchive;
use std::collections::HashMap;
use tauri::Manager;

/// Extracted wallet identity components frequently needed by sync/post/backup operations.
pub(crate) struct SyncIdentityContext {
    pub my_did: String,
    pub my_ed_sk: ed25519_dalek::SigningKey,
    pub master_seed: [u8; 32],
    pub my_prefix: Option<String>,
}

impl SyncIdentityContext {
    pub(crate) fn from_service(service: &AppService) -> Result<Self, FrontendError> {
        service
            .with_wallet_and_identity(|_w, id| {
                let my_ed_sk = id.signing_key.clone();
                let master_seed = my_ed_sk.to_bytes();
                let my_prefix = get_prefix_from_user_id(&id.user_id).map(|s| s.to_string());
                Self {
                    my_did: id.user_id.clone(),
                    my_ed_sk,
                    master_seed,
                    my_prefix,
                }
            })
            .map_err(FrontendError::from)
    }
}

/// Returns the 32-byte wallet Ed25519 master seed from an unlocked `AppService`.
pub(crate) fn get_wallet_master_seed(service: &mut AppService) -> Result<[u8; 32], FrontendError> {
    let signing_key = service
        .with_wallet_and_identity(|_w, id| id.signing_key.clone())
        .map_err(FrontendError::from)?;
    Ok(signing_key.to_bytes())
}

/// Unifies direct public-sync and stealth drop-channel signing across `sync`, `post`, and `backup`.
pub(crate) struct DropChannelSigner<'a> {
    pub is_public: bool,
    pub master_seed: &'a [u8; 32],
    pub my_ed_sk: &'a ed25519_dalek::SigningKey,
    pub drop_desc: &'a humoco_market::DropDescriptor,
}

impl<'a> DropChannelSigner<'a> {
    pub(crate) fn derive_slot_pubkey(&self) -> Result<String, String> {
        if self.is_public || self.drop_desc.slot_id.is_some() {
            let (_, r_vk, _) = humoco_market::derive_public_drop_keypair(
                self.master_seed,
                &self.drop_desc.write_url,
            )
            .map_err(|e| format!("Public sync key derivation error: {}", e))?;
            Ok(hex::encode(r_vk.as_bytes()))
        } else {
            humoco_market::derive_member_stealth_pubkey(self.my_ed_sk, &self.drop_desc.drop_pub)
                .map_err(|e| format!("Stealth pubkey derivation error: {}", e))
        }
    }

    pub(crate) fn sign_request(
        &self,
        method: &str,
        channel: &str,
        prefix: Option<&str>,
        now_secs: u64,
        nonce: &str,
        payload: &[u8],
    ) -> Result<humoco_market::SignedDropHeaders, String> {
        if self.is_public || self.drop_desc.slot_id.is_some() {
            let (r_sk, _, _) = humoco_market::derive_public_drop_keypair(
                self.master_seed,
                &self.drop_desc.write_url,
            )
            .map_err(|e| format!("Public drop key derivation error: {}", e))?;
            Ok(humoco_market::sign_member_request_direct(
                &r_sk, method, channel, prefix, now_secs, nonce, payload,
            ))
        } else {
            humoco_market::sign_member_request_stealth(
                self.my_ed_sk,
                &self.drop_desc.drop_pub,
                method,
                channel,
                prefix,
                now_secs,
                nonce,
                payload,
            )
            .map_err(|e| format!("Stealth signing error: {}", e))
        }
    }
}

/// Compares two DIDs by exact string match or underlying Ed25519 public key equality (ignoring prefix).
pub(crate) fn are_dids_equal(a: &str, b: &str) -> bool {
    let a_trim = a.trim();
    let b_trim = b.trim();
    if a_trim == b_trim {
        return true;
    }
    match (
        get_pubkey_from_user_id(a_trim),
        get_pubkey_from_user_id(b_trim),
    ) {
        (Ok(pk_a), Ok(pk_b)) => pk_a.as_bytes() == pk_b.as_bytes(),
        _ => false,
    }
}

/// Loads built-in (`minuto_v1`, `freetaler_v1`), caller-supplied, and custom voucher standard TOMLs.
pub(crate) fn load_all_standard_tomls(
    app: Option<&tauri::AppHandle>,
    extra: Option<&HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let minuto_toml =
        include_str!("../../../voucher_standards/minuto_v1/standard.toml").to_string();
    if let Ok((std, _)) = VoucherStandardDefinition::from_toml(&minuto_toml) {
        map.insert(std.immutable.identity.uuid, minuto_toml);
    }
    let freetaler_toml =
        include_str!("../../../voucher_standards/freetaler_v1/standard.toml").to_string();
    if let Ok((std, _)) = VoucherStandardDefinition::from_toml(&freetaler_toml) {
        map.insert(std.immutable.identity.uuid, freetaler_toml);
    }
    if let Some(extra_map) = extra {
        map.extend(extra_map.clone());
    }
    if let Some(a) = app {
        let custom = collect_custom_standards(a);
        for (_id, toml) in custom {
            if let Ok((std, _)) = VoucherStandardDefinition::from_toml(&toml) {
                map.insert(std.immutable.identity.uuid, toml);
            }
        }
    }
    map
}

/// Canonical URL normalization for sync endpoints and probe cache lookups.
/// Lowercases scheme and host (via `url::Url`) while preserving path case and stripping trailing slashes.
pub(crate) fn normalize_sync_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Ok(parsed) = url::Url::parse(trimmed) {
        parsed.as_str().trim_end_matches('/').to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

/// Computes the maximum `pow_base_bits` across probed lock servers for a committee.
pub(crate) fn max_pow_base_bits_for_servers<'a>(
    settings: &AppSettings,
    write_urls: impl IntoIterator<Item = &'a str>,
) -> u32 {
    let mut bits = DEFAULT_LOCK_POW_BASE_BITS;
    for url in write_urls {
        let norm = normalize_sync_url(url);
        if let Some(probe) = settings.l2_probe_cache.get(&norm) {
            bits = bits.max(probe.pow_base_bits);
        }
    }
    bits
}

/// Opens the profile-scoped encrypted `FileVoucherArchive` in `app_data_dir/voucher_archives/<hash>`.
pub(crate) fn open_wallet_archive(
    app: &tauri::AppHandle,
    service: &AppService,
) -> Option<FileVoucherArchive> {
    let app_data_dir = app.path().app_data_dir().ok()?;
    let (user_id, key) = service
        .with_wallet_and_identity(|w, id| {
            let uid = w.get_user_id().to_string();
            let derived =
                blake3::derive_key("HUMOCO_APP_ARCHIVE_KEY_V1", &id.signing_key.to_bytes());
            (uid, derived)
        })
        .ok()?;
    if key.iter().all(|&b| b == 0) {
        return None;
    }
    let folder_hash = &blake3::hash(user_id.as_bytes()).to_hex()[..32];
    let archive_dir = app_data_dir.join("voucher_archives").join(folder_hash);
    Some(FileVoucherArchive::with_key(archive_dir, key))
}

/// Spawns asynchronous static-lock L2 reconciliation if a restored wallet still has pending checks.
pub(crate) fn spawn_pending_l2_reconciliation(app: &tauri::AppHandle, secret: &str) {
    let app_handle = app.clone();
    let owned_secret = secret.to_string();
    tauri::async_runtime::spawn(async move {
        let app_state = app_handle.state::<AppState>();
        let _ = crate::services::l2_anchor_service::reconcile_restored_wallet_if_pending(
            &app_state,
            Some(&owned_secret),
        )
        .await;
    });
}
