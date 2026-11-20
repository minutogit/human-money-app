use crate::models::FrontendError;
use crate::services::fs_helpers::collect_custom_standards;
use crate::settings::InternalLifecycleState;
use crate::AppState;
use human_money_core::app_service::AppService;
use human_money_core::crypto::get_prefix_from_user_id;
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

/// Compares two User IDs after strict syntax/checksum validation via their underlying root public key.
pub(crate) fn are_dids_equal(a: &str, b: &str) -> bool {
    match (
        human_money_core::ValidatedUserId::parse(a),
        human_money_core::ValidatedUserId::parse(b),
    ) {
        (Ok(va), Ok(vb)) => va.same_root_identity(&vb),
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

/// Upserts a lock server probe keyed by lowercase 64-hex `lock_pub`, keeping verified
/// delegation metadata independent from live health/liveness observations so a newer failure or
/// revocation cannot be dropped while signed locators are never downgraded.
pub(crate) fn upsert_verified_lock_probe(
    cache: &mut std::collections::BTreeMap<
        String,
        crate::sync_client::FrontendLockServerProbeResult,
    >,
    mut probe: crate::sync_client::FrontendLockServerProbeResult,
) -> bool {
    let lock_key = probe.lock_pub.trim().to_ascii_lowercase();
    if human_money_core::humoco_proto::decode_hex_fixed::<32>(&lock_key).is_err() {
        return false;
    }
    probe.lock_pub = lock_key.clone();
    probe.admin_pub = probe.admin_pub.trim().to_ascii_lowercase();
    probe.drop_pub = probe.drop_pub.trim().to_ascii_lowercase();

    let Some(existing) = cache.get_mut(&lock_key) else {
        cache.insert(lock_key, probe);
        return true;
    };

    let before = existing.clone();
    let incoming_del_valid = probe.delegation_valid && probe.delegation.is_some();
    let incoming_issued_at = probe.delegation.as_ref().map(|d| d.issued_at).unwrap_or(0);
    let existing_issued_at = existing
        .delegation
        .as_ref()
        .map(|d| d.issued_at)
        .unwrap_or(0);

    let update_delegation = if existing.delegation_valid {
        incoming_del_valid && incoming_issued_at >= existing_issued_at
    } else {
        incoming_del_valid || (!probe.delegation_valid && probe.probed_at_secs >= existing.probed_at_secs)
    };

    if update_delegation {
        if !probe.write_url.trim().is_empty() {
            existing.write_url = probe.write_url;
        }
        if !probe.read_url.trim().is_empty() {
            existing.read_url = probe.read_url;
        }
        if !probe.drop_pub.is_empty() {
            existing.drop_pub = probe.drop_pub;
        }
        if !probe.admin_pub.is_empty() {
            existing.admin_pub = probe.admin_pub;
        }
        existing.delegation_valid = probe.delegation_valid;
        existing.delegation = probe.delegation;
    }

    let update_health = if probe.probed_at_secs > existing.probed_at_secs {
        true
    } else if probe.probed_at_secs == existing.probed_at_secs {
        probe.passed || probe.error.is_some() || !existing.passed
    } else {
        false
    };

    if update_health {
        existing.waf_probe_passed = probe.waf_probe_passed;
        existing.selftest_passed = probe.selftest_passed;
        existing.key_leak_safe = probe.key_leak_safe;
        existing.passed = probe.passed;
        existing.latency_ms = probe.latency_ms;
        if probe.max_retention_quarter > 0 || probe.delegation_valid {
            existing.max_retention_quarter = probe.max_retention_quarter;
            existing.pow_base_bits = probe.pow_base_bits;
            existing.allow_pow_tickets = probe.allow_pow_tickets;
            existing.ticket_pow_bits = probe.ticket_pow_bits;
            existing.inode_warning = probe.inode_warning;
            existing.used_inodes = probe.used_inodes;
            existing.max_inodes = probe.max_inodes;
        }
        existing.probed_at_secs = probe.probed_at_secs;
        existing.error = probe.error;
    }

    *existing != before
}

/// Resolves the verified live locator `(write_url, read_url)` for a manifest `ServerEntry` from
/// `probe_cache` ONLY when a cryptographically valid 5-field delegation matching `(lock_pub, admin_pub)`
/// exists and passes SSRF/URL validation.
pub(crate) fn resolve_verified_server_locator(
    entry: &human_money_core::humoco_proto::manifest::ServerEntry,
    probe_cache: &std::collections::BTreeMap<
        String,
        crate::sync_client::FrontendLockServerProbeResult,
    >,
) -> Option<(String, String)> {
    let lock_hex = hex::encode(entry.lock_pub);
    let admin_hex = hex::encode(entry.admin_pub);
    let probe = probe_cache.get(&lock_hex)?;
    let admin_matches = probe.admin_pub.trim().eq_ignore_ascii_case(&admin_hex);
    let lock_matches = probe.lock_pub.trim().eq_ignore_ascii_case(&lock_hex);
    let del_valid = probe.delegation_valid
        && probe.delegation.as_ref().is_some_and(|d| {
            crate::sync_client::verify_lock_delegation(&lock_hex, d, Some(&admin_hex))
                && normalize_sync_url(&d.write_url) == normalize_sync_url(&probe.write_url)
                && normalize_sync_url(&d.read_url) == normalize_sync_url(&probe.read_url)
                && crate::sync_client::validate_sync_url(d.write_url.trim()).is_ok()
                && crate::sync_client::validate_sync_url(d.read_url.trim()).is_ok()
        });
    if admin_matches && lock_matches && del_valid {
        Some((
            probe.write_url.trim().to_string(),
            probe.read_url.trim().to_string(),
        ))
    } else {
        None
    }
}

/// Resolves the live locator `(write_url, read_url)` for a manifest `ServerEntry`, preferring
/// any newer verified delegation/probe entry matching `(lock_pub, admin_pub)` in `probe_cache`.
pub(crate) fn resolve_server_locator(
    entry: &human_money_core::humoco_proto::manifest::ServerEntry,
    probe_cache: &std::collections::BTreeMap<
        String,
        crate::sync_client::FrontendLockServerProbeResult,
    >,
) -> (String, String) {
    resolve_verified_server_locator(entry, probe_cache)
        .unwrap_or_else(|| (entry.write_url.clone(), entry.read_url.clone()))
}

/// Computes the maximum `pow_base_bits` across probed lock servers identified by `lock_pub`.
pub(crate) fn max_pow_base_bits_for_servers(
    lifecycle: &InternalLifecycleState,
    lock_pubs: impl IntoIterator<Item = [u8; 32]>,
) -> u32 {
    let mut bits = DEFAULT_LOCK_POW_BASE_BITS;
    for lp in lock_pubs {
        let key = hex::encode(lp);
        if let Some(probe) = lifecycle.l2_probe_cache.get(&key) {
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

/// Blocks voucher spending operations until the mandatory post-restore L2 reconciliation completes.
pub(crate) fn ensure_restore_l2_reconciled(
    state: &crate::state::AppState,
    service: &mut AppService,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let lifecycle = state
        .get_or_load_internal_lifecycle(service, password)
        .map_err(FrontendError::from)?;
    if lifecycle.pending_restore_l2_reconciliation {
        return Err(FrontendError::from(
            "Nach einer Wiederherstellung aus der Sicherung ist vor dem Ausgeben von Gutscheinen ein einmaliger Online-Sicherheitsabgleich erforderlich.".to_string(),
        ));
    }
    Ok(())
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
