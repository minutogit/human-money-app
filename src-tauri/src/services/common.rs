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
    pub master_seed: zeroize::Zeroizing<[u8; 32]>,
    pub my_prefix: Option<String>,
}

impl SyncIdentityContext {
    pub(crate) fn from_service(
        service: &AppService,
        password: Option<&str>,
    ) -> Result<Self, FrontendError> {
        service
            .with_authorized_identity(
                human_money_core::AccessMode::DailyPay,
                password,
                |_w, id| {
                    let my_ed_sk = id.signing_key.clone();
                    let master_seed = zeroize::Zeroizing::new(my_ed_sk.to_bytes());
                    let my_prefix = get_prefix_from_user_id(&id.user_id).map(|s| s.to_string());
                    Self {
                        my_did: id.user_id.clone(),
                        my_ed_sk,
                        master_seed,
                        my_prefix,
                    }
                },
            )
            .map_err(FrontendError::from)
    }
}

/// Returns the 32-byte wallet Ed25519 master seed from an authorized `AppService`.
pub(crate) fn get_wallet_master_seed(
    service: &mut AppService,
    password: Option<&str>,
) -> Result<zeroize::Zeroizing<[u8; 32]>, FrontendError> {
    let signing_key = service
        .with_authorized_identity(
            human_money_core::AccessMode::DailyPay,
            password,
            |_w, id| id.signing_key.clone(),
        )
        .map_err(FrontendError::from)?;
    Ok(zeroize::Zeroizing::new(signing_key.to_bytes()))
}

pub(crate) use crate::services::sync_endpoints::{
    canonical_drop_pub_hex, is_endpoint_revoked, normalize_sync_url,
};

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
            humoco_market::sign_member_request_direct(
                &r_sk, method, channel, prefix, now_secs, nonce, payload,
            )
            .map_err(|e| format!("Direct signing error: {}", e))
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
    let mut built_in_uuids = std::collections::HashSet::new();
    let minuto_toml =
        include_str!("../../../voucher_standards/minuto_v1/standard.toml").to_string();
    if let Ok((std, _)) = VoucherStandardDefinition::from_toml(&minuto_toml) {
        built_in_uuids.insert(std.immutable.identity.uuid.clone());
        map.insert(std.immutable.identity.uuid, minuto_toml);
    }
    let freetaler_toml =
        include_str!("../../../voucher_standards/freetaler_v1/standard.toml").to_string();
    if let Ok((std, _)) = VoucherStandardDefinition::from_toml(&freetaler_toml) {
        built_in_uuids.insert(std.immutable.identity.uuid.clone());
        map.insert(std.immutable.identity.uuid, freetaler_toml);
    }
    if let Some(extra_map) = extra {
        let mut extra_entries: Vec<(&String, &String)> = extra_map.iter().collect();
        extra_entries.sort_by(|a, b| a.0.cmp(b.0));
        for (uuid, toml) in extra_entries {
            if !built_in_uuids.contains(uuid) {
                map.entry(uuid.clone()).or_insert_with(|| toml.clone());
            }
        }
    }
    if let Some(a) = app {
        let mut custom: Vec<(String, String)> = collect_custom_standards(a).into_iter().collect();
        custom.sort_by(|a, b| a.0.cmp(&b.0));
        for (_id, toml) in custom {
            if let Ok((std, _)) = VoucherStandardDefinition::from_toml(&toml) {
                if !built_in_uuids.contains(&std.immutable.identity.uuid) {
                    map.entry(std.immutable.identity.uuid).or_insert(toml);
                }
            }
        }
    }
    map
}

/// Collects active, non-revoked, URL-validated backup sync descriptors in deterministic priority
/// order (`own` -> `market_selected_syncs` -> `public_syncs`), deduplicated by normalized `write_url`.
pub(crate) fn collect_backup_sync_targets(
    security_policy: &crate::settings::SecurityPolicy,
    own_sync_descriptor: Option<&humoco_market::DropDescriptor>,
    user_prefix: Option<&str>,
    max_count: Option<usize>,
) -> Vec<humoco_market::DropDescriptor> {
    let mut targets = Vec::new();
    let mut seen_write_urls = std::collections::HashSet::new();

    let mut try_push = |mut desc: humoco_market::DropDescriptor| {
        if let Some(cap) = max_count {
            if targets.len() >= cap {
                return;
            }
        }
        let write_trimmed = desc.write_url.trim();
        let read_trimmed = desc.read_url.trim();
        if crate::sync_client::validate_sync_url(write_trimmed).is_err()
            || crate::sync_client::validate_sync_read_url(read_trimmed).is_err()
        {
            return;
        }
        let Ok(clean_drop_pub) = canonical_drop_pub_hex(&desc.drop_pub) else {
            return;
        };
        let slot_id = match desc
            .slot_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(sid) => {
                if humoco_market::parse_slot_id(sid).is_err() {
                    return;
                }
                Some(sid.to_string())
            }
            None => None,
        };
        let norm_write = normalize_sync_url(write_trimmed);
        if norm_write.is_empty() || !seen_write_urls.insert(norm_write) {
            return;
        }
        desc.write_url = write_trimmed.to_string();
        desc.read_url = read_trimmed.to_string();
        desc.drop_pub = clean_drop_pub;
        desc.slot_id = slot_id;
        targets.push(desc);
    };

    if let Some(own) = own_sync_descriptor {
        try_push(own.clone());
    }

    for sd in &security_policy.market_selected_syncs {
        if is_endpoint_revoked(&sd.status) {
            continue;
        }
        try_push(sd.clone().into());
    }

    for pd in &security_policy.public_syncs {
        if !pd.enabled || is_endpoint_revoked(&pd.status) {
            continue;
        }
        let Ok(clean_access_pk) = canonical_drop_pub_hex(&pd.access_pubkey) else {
            continue;
        };
        let Ok(slot_id) = humoco_market::format_slot_id(&clean_access_pk, user_prefix) else {
            continue;
        };
        try_push(humoco_market::DropDescriptor {
            write_url: pd.write_url.clone(),
            read_url: pd.read_url.clone(),
            slot_id: Some(slot_id),
            drop_pub: pd.drop_pub.clone(),
        });
    }

    targets
}

/// Collects active backup sync targets for the current profile using cached policy/descriptor getters.
pub(crate) fn collect_backup_sync_targets_with_service(
    state: &AppState,
    service: &mut AppService,
    password: Option<&str>,
    max_count: Option<usize>,
) -> Result<Vec<humoco_market::DropDescriptor>, FrontendError> {
    let security_policy = state
        .get_or_load_security_policy(service, password)
        .map_err(FrontendError::from)?;
    let own_desc = state.get_or_load_own_sync_descriptor(service, password)?;
    let user_prefix = service
        .with_wallet(|w| get_prefix_from_user_id(w.get_user_id()).map(|s| s.to_string()))
        .ok()
        .flatten();
    Ok(collect_backup_sync_targets(
        &security_policy,
        own_desc.as_ref(),
        user_prefix.as_deref(),
        max_count,
    ))
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
    probe.pow_base_bits = probe
        .pow_base_bits
        .min(human_money_core::humoco_proto::crypto::MAX_LOCK_POW_BITS);
    probe.ticket_pow_bits = probe
        .ticket_pow_bits
        .min(human_money_core::humoco_proto::crypto::MAX_LOCK_POW_BITS);

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
        incoming_del_valid
            || (!probe.delegation_valid && probe.probed_at_secs >= existing.probed_at_secs)
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
        let existing_revoked_or_errored = !existing.passed && existing.error.is_some();
        if existing_revoked_or_errored && probe.passed {
            false
        } else {
            probe.passed || probe.error.is_some() || !existing.passed
        }
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

/// Inserts or updates ONLY cryptographically verified locator/parameter metadata (`write_url`,
/// `read_url`, `drop_pub`, `admin_pub`, `delegation`, `max_retention_quarter`, `pow_base_bits`,
/// `allow_pow_tickets`, `ticket_pow_bits`, inode quotas) for a `lock_pub` without touching full
/// health-probe verdicts (`passed`, `waf_probe_passed`, `selftest_passed`, `key_leak_safe`, `error`).
pub(crate) fn upsert_verified_locator_only(
    cache: &mut std::collections::BTreeMap<
        String,
        crate::sync_client::FrontendLockServerProbeResult,
    >,
    mut probe: crate::sync_client::FrontendLockServerProbeResult,
) -> bool {
    let lock_key = probe.lock_pub.trim().to_ascii_lowercase();
    if human_money_core::humoco_proto::decode_hex_fixed::<32>(&lock_key).is_err()
        || !probe.delegation_valid
    {
        return false;
    }
    let Some(ref del) = probe.delegation else {
        return false;
    };
    probe.lock_pub = lock_key.clone();
    probe.admin_pub = probe.admin_pub.trim().to_ascii_lowercase();
    probe.drop_pub = probe.drop_pub.trim().to_ascii_lowercase();
    probe.pow_base_bits = probe
        .pow_base_bits
        .min(human_money_core::humoco_proto::crypto::MAX_LOCK_POW_BITS);
    probe.ticket_pow_bits = probe
        .ticket_pow_bits
        .min(human_money_core::humoco_proto::crypto::MAX_LOCK_POW_BITS);

    let expected_admin = (!probe.admin_pub.is_empty()).then_some(probe.admin_pub.as_str());
    if !del.lock_pub.trim().eq_ignore_ascii_case(&lock_key)
        || !crate::sync_client::verify_lock_delegation(&lock_key, del, expected_admin)
        || crate::sync_client::validate_sync_url(del.write_url.trim()).is_err()
        || crate::sync_client::validate_sync_read_url(del.read_url.trim()).is_err()
        || normalize_sync_url(&del.write_url) != normalize_sync_url(&probe.write_url)
        || normalize_sync_url(&del.read_url) != normalize_sync_url(&probe.read_url)
    {
        return false;
    }
    if probe.admin_pub.is_empty() {
        probe.admin_pub = del.admin_pub.trim().to_ascii_lowercase();
    }

    let Some(existing) = cache.get_mut(&lock_key) else {
        cache.insert(lock_key, probe);
        return true;
    };

    let before = existing.clone();
    let incoming_issued_at = probe.delegation.as_ref().map(|d| d.issued_at).unwrap_or(0);
    let existing_issued_at = existing
        .delegation
        .as_ref()
        .map(|d| d.issued_at)
        .unwrap_or(0);

    let delegation_fresh = !existing.delegation_valid || incoming_issued_at >= existing_issued_at;
    if delegation_fresh {
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
    let can_update_caps = probe.max_retention_quarter > 0
        && probe.probed_at_secs >= existing.probed_at_secs
        && delegation_fresh
        && !(existing.passed && !probe.passed && existing.max_retention_quarter > 0);
    if can_update_caps {
        existing.max_retention_quarter = probe.max_retention_quarter;
        existing.pow_base_bits = probe.pow_base_bits;
        existing.allow_pow_tickets = probe.allow_pow_tickets;
        existing.ticket_pow_bits = probe.ticket_pow_bits;
        existing.inode_warning = probe.inode_warning;
        existing.used_inodes = probe.used_inodes;
        existing.max_inodes = probe.max_inodes;
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
                && crate::sync_client::validate_sync_read_url(d.read_url.trim()).is_ok()
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

/// Cryptographically verifies a signed 5-field `LockDelegation` against a manifest `ServerEntry`
/// (`lock_pub`, `admin_pub`) and SSRF/URL validation rules.
pub(crate) fn is_valid_delegation_for_entry(
    entry: &human_money_core::humoco_proto::manifest::ServerEntry,
    del: &crate::sync_client::SyncLockDelegationDto,
) -> bool {
    let lock_hex = hex::encode(entry.lock_pub);
    let admin_hex = hex::encode(entry.admin_pub);
    del.lock_pub.trim().eq_ignore_ascii_case(&lock_hex)
        && del.admin_pub.trim().eq_ignore_ascii_case(&admin_hex)
        && crate::sync_client::verify_lock_delegation(&lock_hex, del, Some(&admin_hex))
        && crate::sync_client::validate_sync_url(del.write_url.trim()).is_ok()
        && crate::sync_client::validate_sync_read_url(del.read_url.trim()).is_ok()
}

/// Collects all cryptographically verified 5-field `LockDelegation`s from `probe_cache` matching
/// servers in `manifest` and serializes them as compact JSON for embedding in `.ask` / PoST requests.
pub(crate) fn collect_verified_delegations_for_manifest(
    manifest: &human_money_core::humoco_proto::manifest::AnchorManifest,
    probe_cache: &std::collections::BTreeMap<
        String,
        crate::sync_client::FrontendLockServerProbeResult,
    >,
) -> Option<String> {
    let mut delegations = Vec::new();
    for entry in &manifest.servers {
        let lock_hex = hex::encode(entry.lock_pub);
        let Some(probe) = probe_cache.get(&lock_hex) else {
            continue;
        };
        if !probe.delegation_valid {
            continue;
        }
        let Some(ref del) = probe.delegation else {
            continue;
        };
        if normalize_sync_url(&del.write_url) == normalize_sync_url(&probe.write_url)
            && normalize_sync_url(&del.read_url) == normalize_sync_url(&probe.read_url)
            && is_valid_delegation_for_entry(entry, del)
        {
            delegations.push(del.clone());
        }
    }
    if delegations.is_empty() {
        None
    } else {
        serde_json::to_string(&delegations).ok()
    }
}

/// Parses and cryptographically verifies embedded 5-field `LockDelegation`s from JSON against `manifest.servers`.
pub(crate) fn parse_and_verify_embedded_delegations(
    raw_json: Option<&str>,
    manifest: &human_money_core::humoco_proto::manifest::AnchorManifest,
) -> std::collections::BTreeMap<String, crate::sync_client::SyncLockDelegationDto> {
    let mut verified = std::collections::BTreeMap::new();
    let Some(raw) = raw_json
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 65_536)
    else {
        return verified;
    };
    let Ok(list) = serde_json::from_str::<Vec<crate::sync_client::SyncLockDelegationDto>>(raw)
    else {
        return verified;
    };
    for del in list {
        if let Some(entry) = manifest
            .servers
            .iter()
            .find(|s| hex::encode(s.lock_pub).eq_ignore_ascii_case(del.lock_pub.trim()))
        {
            if is_valid_delegation_for_entry(entry, &del) {
                let lock_hex = hex::encode(entry.lock_pub);
                let replace = verified.get(&lock_hex).is_none_or(
                    |prev: &crate::sync_client::SyncLockDelegationDto| {
                        del.issued_at > prev.issued_at
                    },
                );
                if replace {
                    verified.insert(lock_hex, del);
                }
            }
        }
    }
    verified
}

/// Resolves a verified server `(write_url, read_url)` using EITHER local `probe_cache` OR
/// cryptographically verified embedded delegations carried alongside an offline `.ask` request.
pub(crate) fn resolve_locator_with_delegations(
    entry: &human_money_core::humoco_proto::manifest::ServerEntry,
    probe_cache: &std::collections::BTreeMap<
        String,
        crate::sync_client::FrontendLockServerProbeResult,
    >,
    embedded_delegations: &std::collections::BTreeMap<
        String,
        crate::sync_client::SyncLockDelegationDto,
    >,
) -> Option<(String, String)> {
    resolve_verified_server_locator(entry, probe_cache).or_else(|| {
        embedded_delegations
            .get(&hex::encode(entry.lock_pub))
            .map(|del| {
                (
                    del.write_url.trim().to_string(),
                    del.read_url.trim().to_string(),
                )
            })
    })
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

/// Computes the maximum `pow_base_bits` across verified probed lock servers identified by
/// `lock_pub`, clamped to `MAX_LOCK_POW_BITS`.
pub(crate) fn max_pow_base_bits_for_servers(
    lifecycle: &InternalLifecycleState,
    lock_pubs: impl IntoIterator<Item = [u8; 32]>,
) -> u32 {
    let mut bits = DEFAULT_LOCK_POW_BASE_BITS;
    for lp in lock_pubs {
        let key = hex::encode(lp);
        if let Some(probe) = lifecycle.l2_probe_cache.get(&key) {
            if probe.passed && probe.delegation_valid {
                bits = bits.max(probe.pow_base_bits);
            }
        }
    }
    bits.min(human_money_core::humoco_proto::crypto::MAX_LOCK_POW_BITS)
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
            let sk_bytes = zeroize::Zeroizing::new(id.signing_key.to_bytes());
            let derived = zeroize::Zeroizing::new(blake3::derive_key(
                "HUMOCO_APP_ARCHIVE_KEY_V1",
                sk_bytes.as_slice(),
            ));
            (uid, derived)
        })
        .ok()?;
    if key.iter().all(|&b| b == 0) {
        return None;
    }
    let folder_hash = &blake3::hash(user_id.as_bytes()).to_hex()[..32];
    let archive_dir = app_data_dir.join("voucher_archives").join(folder_hash);
    Some(FileVoucherArchive::with_key(archive_dir, *key))
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
pub(crate) fn spawn_pending_l2_reconciliation(
    app: &tauri::AppHandle,
    session: crate::session::SessionToken,
    secret: &str,
) {
    let app_handle = app.clone();
    let owned_secret = zeroize::Zeroizing::new(secret.to_string());
    tauri::async_runtime::spawn(async move {
        let app_state = app_handle.state::<AppState>();
        let _ = crate::services::l2_anchor_service::reconcile_restored_wallet_if_pending(
            &app_state,
            &session,
            Some(owned_secret.as_str()),
        )
        .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::crypto::MAX_LOCK_POW_BITS;
    use human_money_core::humoco_proto::manifest::{AnchorManifest, NominatedBy, ServerEntry};

    fn make_signed_delegation(
        admin_sk: &SigningKey,
        lock_pub: &str,
        write_url: &str,
        read_url: &str,
        issued_at: u64,
    ) -> crate::sync_client::SyncLockDelegationDto {
        let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
        let lock_lower = lock_pub.to_ascii_lowercase();
        let msg = humoco_market::format_lock_delegate_signature_message(
            &admin_hex,
            &lock_lower,
            write_url,
            read_url,
            issued_at,
        );
        let sig = admin_sk.sign(msg.as_bytes());
        crate::sync_client::SyncLockDelegationDto {
            admin_pub: admin_hex,
            lock_pub: lock_lower,
            write_url: write_url.to_string(),
            read_url: read_url.to_string(),
            issued_at,
            sig: hex::encode(sig.to_bytes()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_probe(
        lock_pub: &str,
        admin_pub: &str,
        write_url: &str,
        read_url: &str,
        passed: bool,
        probed_at_secs: u64,
        max_retention_quarter: u16,
        pow_base_bits: u32,
        ticket_pow_bits: u32,
        delegation: Option<crate::sync_client::SyncLockDelegationDto>,
    ) -> crate::sync_client::FrontendLockServerProbeResult {
        crate::sync_client::FrontendLockServerProbeResult {
            write_url: write_url.to_string(),
            read_url: read_url.to_string(),
            drop_pub: admin_pub.to_string(),
            lock_pub: lock_pub.to_string(),
            admin_pub: admin_pub.to_string(),
            delegation_valid: delegation.is_some(),
            waf_probe_passed: passed,
            selftest_passed: passed,
            key_leak_safe: passed,
            passed,
            latency_ms: 10,
            max_retention_quarter,
            pow_base_bits,
            allow_pow_tickets: true,
            ticket_pow_bits,
            inode_warning: false,
            used_inodes: 1,
            max_inodes: 100,
            probed_at_secs,
            delegation,
            error: None,
        }
    }

    #[test]
    fn s07_f6_probe_cache_clamps_pow_verifies_delegation_and_prevents_downgrade() {
        let admin_sk = SigningKey::from_bytes(&[0x33u8; 32]);
        let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
        let lock_bytes = SigningKey::from_bytes(&[0x44u8; 32])
            .verifying_key()
            .to_bytes();
        let lock_hex = hex::encode(lock_bytes);
        let write_url = "https://sync.example.org/sync.php";
        let read_url = "https://sync.example.org/data";

        let valid_del_v2 = make_signed_delegation(&admin_sk, &lock_hex, write_url, read_url, 200);
        let valid_del_v1 = make_signed_delegation(&admin_sk, &lock_hex, write_url, read_url, 100);

        let mut cache = std::collections::BTreeMap::new();

        // 1. upsert_verified_lock_probe must clamp excessive PoW bits on insert
        let high_pow_probe = make_probe(
            &lock_hex,
            &admin_hex,
            write_url,
            read_url,
            true,
            500,
            20304,
            MAX_LOCK_POW_BITS + 12,
            MAX_LOCK_POW_BITS + 9,
            Some(valid_del_v2.clone()),
        );
        assert!(upsert_verified_lock_probe(&mut cache, high_pow_probe));
        let stored = cache.get(&lock_hex).unwrap();
        assert_eq!(stored.pow_base_bits, MAX_LOCK_POW_BITS);
        assert_eq!(stored.ticket_pow_bits, MAX_LOCK_POW_BITS);

        // 2. upsert_verified_locator_only must reject forged delegation even with delegation_valid=true
        let mut forged_del = valid_del_v2.clone();
        forged_del.sig = "00".repeat(64);
        let mut forged_cache = std::collections::BTreeMap::new();
        let forged_probe = make_probe(
            &lock_hex,
            &admin_hex,
            write_url,
            read_url,
            false,
            600,
            20304,
            12,
            12,
            Some(forged_del),
        );
        assert!(
            !upsert_verified_locator_only(&mut forged_cache, forged_probe),
            "S07-F6: upsert_verified_locator_only must cryptographically verify delegation"
        );
        assert!(forged_cache.is_empty());

        // 3. upsert_verified_locator_only must NOT overwrite capability metadata of a full-probed entry
        let locator_probe = make_probe(
            &lock_hex,
            &admin_hex,
            write_url,
            read_url,
            false,
            600,
            20261,
            8,
            8,
            Some(valid_del_v2.clone()),
        );
        upsert_verified_locator_only(&mut cache, locator_probe);
        assert_eq!(
            cache.get(&lock_hex).unwrap().max_retention_quarter,
            20304,
            "S07-F6: locator-only probe must not overwrite full-probed capability metadata"
        );

        // 4. parse_and_verify_embedded_delegations must keep highest issued_at (no downgrade)
        let manifest = AnchorManifest {
            revision: 0,
            threshold: 1,
            servers: vec![ServerEntry {
                lock_pub: lock_bytes,
                admin_pub: admin_sk.verifying_key().to_bytes(),
                write_url: write_url.to_string(),
                read_url: read_url.to_string(),
                nominated_by: NominatedBy::Creator,
            }],
            successors: vec![],
            authority: human_money_core::humoco_proto::manifest::AnchorAuthority {
                creator_mig_pub: [0x11u8; 32],
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: 20284,
        };
        let raw_json = serde_json::to_string(&vec![valid_del_v2.clone(), valid_del_v1]).unwrap();
        let parsed = parse_and_verify_embedded_delegations(Some(&raw_json), &manifest);
        assert_eq!(
            parsed.get(&lock_hex).map(|d| d.issued_at),
            Some(200),
            "S07-F6: embedded delegations must keep highest issued_at and reject downgrade"
        );
    }

    #[test]
    fn s07_f12_collect_backup_sync_targets_prioritizes_dedups_filters_revoked_and_caps() {
        use crate::test_support::test_ed25519_pub_hex;

        let pub1 = test_ed25519_pub_hex(0x11);
        let pub2 = test_ed25519_pub_hex(0x22);
        let pub3 = test_ed25519_pub_hex(0x23);
        let pub4 = test_ed25519_pub_hex(0x33);
        let pub5 = test_ed25519_pub_hex(0x44);
        let pub6 = test_ed25519_pub_hex(0x55);
        let acc1 = test_ed25519_pub_hex(0xaa);
        let acc2 = test_ed25519_pub_hex(0xbb);

        let mut policy = crate::settings::SecurityPolicy::default();
        let own = humoco_market::DropDescriptor {
            write_url: "https://OWN.example.org/sync.php/".to_string(),
            read_url: "https://own.example.org/data".to_string(),
            slot_id: None,
            drop_pub: pub1.clone(),
        };
        // Duplicate of own with different casing/slash + revoked friend + valid friends
        policy.market_selected_syncs = vec![
            crate::models::FrontendSyncDescriptor {
                write_url: "https://own.example.org/sync.php".to_string(),
                read_url: "https://own.example.org/data".to_string(),
                drop_pub: pub1,
                ..Default::default()
            },
            crate::models::FrontendSyncDescriptor {
                write_url: "https://revoked-friend.example.org/sync.php".to_string(),
                read_url: "https://revoked-friend.example.org/data".to_string(),
                drop_pub: pub2,
                status: "revoked".to_string(),
                ..Default::default()
            },
            crate::models::FrontendSyncDescriptor {
                write_url: "http://evil-non-https.example.org/sync.php".to_string(),
                read_url: "http://evil-non-https.example.org/data".to_string(),
                drop_pub: pub3,
                ..Default::default()
            },
            crate::models::FrontendSyncDescriptor {
                write_url: "https://friend1.example.org/sync.php".to_string(),
                read_url: "https://friend1.example.org/data".to_string(),
                drop_pub: pub4,
                ..Default::default()
            },
        ];
        // Add 10 public syncs (1 revoked, 1 disabled, 8 active)
        policy.public_syncs.push(crate::settings::StoredPublicSync {
            write_url: "https://revoked-pub.example.org/sync.php".to_string(),
            read_url: "https://revoked-pub.example.org/data".to_string(),
            drop_pub: pub5,
            register_url: None,
            access_pubkey: acc1,
            enabled: true,
            authorized_channels: vec![],
            status: "revoked".to_string(),
            sunset_at: None,
            revocation_reason: None,
            replacement: None,
        });
        for i in 0..9 {
            policy.public_syncs.push(crate::settings::StoredPublicSync {
                write_url: format!("https://pub{}.example.org/sync.php", i),
                read_url: format!("https://pub{}.example.org/data", i),
                drop_pub: pub6.clone(),
                register_url: None,
                access_pubkey: acc2.clone(),
                enabled: i > 0, // pub0 disabled
                authorized_channels: vec![],
                status: "active".to_string(),
                sunset_at: None,
                revocation_reason: None,
                replacement: None,
            });
        }

        let capped = collect_backup_sync_targets(
            &policy,
            Some(&own),
            Some("pc"),
            Some(humoco_market::MAX_RECOVERY_SYNC_DESCRIPTORS),
        );
        assert_eq!(
            capped.len(),
            humoco_market::MAX_RECOVERY_SYNC_DESCRIPTORS,
            "S07-F12: recovery targets must be capped at MAX_RECOVERY_SYNC_DESCRIPTORS (8)"
        );
        assert_eq!(
            normalize_sync_url(&capped[0].write_url),
            "https://own.example.org/sync.php",
            "S07-F12: own sync must be prioritized first"
        );
        assert_eq!(capped[1].write_url, "https://friend1.example.org/sync.php");
        assert!(capped
            .iter()
            .all(|d| !d.write_url.contains("revoked") && !d.write_url.contains("evil")));
        assert!(capped[2]
            .slot_id
            .as_deref()
            .is_some_and(|s| s.ends_with("_pc")));
    }

    #[test]
    fn s11_x3_collect_backup_sync_targets_skips_invalid_own_drop_pub_and_public_slot_id() {
        use crate::test_support::test_ed25519_pub_hex;

        let valid_pub = test_ed25519_pub_hex(0x61);
        let valid_acc = test_ed25519_pub_hex(0x62);

        let bad_own = humoco_market::DropDescriptor {
            write_url: "https://own.example.org/sync.php".to_string(),
            read_url: "https://own.example.org/data".to_string(),
            slot_id: None,
            drop_pub: "00".repeat(32), // weak small-order key
        };

        let policy = crate::settings::SecurityPolicy {
            public_syncs: vec![
                crate::settings::StoredPublicSync {
                    write_url: "https://bad-acc.example.org/sync.php".to_string(),
                    read_url: "https://bad-acc.example.org/data".to_string(),
                    drop_pub: valid_pub.clone(),
                    access_pubkey: "not-a-valid-key".to_string(),
                    enabled: true,
                    status: "active".to_string(),
                    ..Default::default()
                },
                crate::settings::StoredPublicSync {
                    write_url: "https://bad-drop-pub.example.org/sync.php".to_string(),
                    read_url: "https://bad-drop-pub.example.org/data".to_string(),
                    drop_pub: "not-hex".to_string(),
                    access_pubkey: valid_acc.clone(),
                    enabled: true,
                    status: "active".to_string(),
                    ..Default::default()
                },
                crate::settings::StoredPublicSync {
                    write_url: "https://good.example.org/sync.php".to_string(),
                    read_url: "https://good.example.org/data".to_string(),
                    drop_pub: valid_pub.clone(),
                    access_pubkey: valid_acc.clone(),
                    enabled: true,
                    status: "active".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        // Even with an invalid prefix (which makes format_slot_id fail), public_syncs must NOT fall back to raw access_pubkey!
        let bad_prefix_targets =
            collect_backup_sync_targets(&policy, Some(&bad_own), Some("BAD--PREFIX"), None);
        assert!(bad_prefix_targets.is_empty());

        // With valid prefix, invalid own/public targets are skipped and only good.example.org remains
        let targets = collect_backup_sync_targets(&policy, Some(&bad_own), Some("pc"), None);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].write_url, "https://good.example.org/sync.php");
        assert_eq!(targets[0].slot_id, Some(format!("{}_pc", valid_acc)));
        assert_eq!(targets[0].drop_pub, valid_pub);
    }
}
