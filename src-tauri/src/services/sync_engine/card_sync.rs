use std::collections::{BTreeMap, HashMap, HashSet};

use human_money_core::app_service::AppService;
use human_money_core::models::conflict::TrustStatus;
use sha2::Digest;

use crate::models::{
    FrontendAddressBook, FrontendDiscoveredSync, FrontendError, FrontendSyncDescriptor,
    StoredOwnMarketProfile,
};
use crate::services::common::{
    canonical_drop_pub_hex, normalize_sync_url, DropChannelSigner, SyncIdentityContext,
};
use crate::services::market_service::sync_own_card_drops_on_settings_change;
use crate::services::recovery_service::enqueue_guardian_locator_updates_if_configured;
use crate::services::sync_endpoints::{
    descriptors_compatible_for_policy, descriptors_overlap, is_endpoint_revoked,
    sanitize_untrusted_sync_descriptors, validate_upload_sync_descriptor,
};
use crate::session::SessionToken;
use crate::settings::{
    SecurityPolicy, UiPreferences, SYNC_BASE_MAX_AGE_SECS, SYNC_CARD_MAX_BYTES, SYNC_HEARTBEAT_SECS,
};
use crate::state::{
    apply_revocation_notice, apply_revocation_notice_to_address_book, canonicalize_did,
    clear_revocation, is_revocation_effective, AppState, StoredSyncState, CONTACTS_DATA_NAME,
    MARKET_CARD_STORE_KEY,
};
use crate::sync_client::{self, PutSlotOutcome, SyncRevocationNotice};

use super::admin::load_stored_sync_admin_opt;
use super::channel_reader::{
    collect_upload_targets_for_channel, delete_delta_card_slot, ensure_cached_channel_max_bytes,
    save_channel_max_bytes_updates,
};
use super::media_sync::{seal_active_ad_media_blobs, upload_media_blobs_to_target};

const DELTA_MAX_BYTES: usize = 32 * 1024;

struct CardUploadContext {
    own_card_opt: Option<humoco_market::SdCard>,
    own_profile_opt: Option<StoredOwnMarketProfile>,
    readers: Vec<ed25519_dalek::VerifyingKey>,
    evicted_reader_keys: Vec<ed25519_dalek::VerifyingKey>,
    readers_hash: String,
}

type ProbeCacheEntry = (String, String, String, bool, Option<SyncRevocationNotice>);

fn is_known_offender(service: &AppService, did: &str) -> bool {
    service
        .with_wallet(|wallet| matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_)))
        .unwrap_or(false)
}

fn contact_display_label(c: &crate::models::FrontendContact) -> String {
    let first = c.profile.first_name.as_deref().unwrap_or("").trim();
    let last = c.profile.last_name.as_deref().unwrap_or("").trim();
    match (first.is_empty(), last.is_empty()) {
        (false, false) => format!("{} {}", first, last),
        (false, true) => first.to_string(),
        (true, false) => last.to_string(),
        (true, true) => c
            .profile
            .organization
            .clone()
            .unwrap_or_else(|| c.did.clone()),
    }
}

pub async fn discover_available_syncs_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendDiscoveredSync>, FrontendError> {
    let auth =
        state.authorize_session(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    let pw = auth.pw();

    let (mut descriptors, security_policy) = state.with_session_required(
        &auth.token,
        "Wallet session changed during sync discovery",
        |service| {
            let mut descriptors: Vec<(FrontendSyncDescriptor, String)> = Vec::new();
            let now = sync_client::current_unix_timestamp();

            if pw.is_some() || service.is_session_active() {
                // 1. Own drop admin
                if let Some(admin) = load_stored_sync_admin_opt(service, pw)? {
                    if !admin.descriptor.read_url.is_empty()
                        && !admin.descriptor.write_url.is_empty()
                    {
                        let mut desc: FrontendSyncDescriptor = admin.descriptor.into();
                        desc.status = "active".to_string();
                        if let Some(ref rev) = admin.revocation {
                            apply_revocation_notice(&mut desc, rev, now);
                        }
                        descriptors.push((desc, "Eigener Sync-Dienst".to_string()));
                    }
                }

                // 2. Contacts
                if let Ok(contacts) = state.load_contacts(service, pw) {
                    for c in &contacts {
                        let name = contact_display_label(c);
                        for d in &c.syncs {
                            descriptors.push((d.clone(), name.clone()));
                        }
                    }
                }

                // 3. Card store
                let store: humoco_market::CardStore = crate::state::load_encrypted_json_or_default(
                    service,
                    MARKET_CARD_STORE_KEY,
                    pw,
                )
                .map_err(FrontendError::from)?;
                for sc in store.cards.values() {
                    let profile = humoco_market::extract_public_profile(&sc.card);
                    let author = profile.first_name.as_deref().unwrap_or("Visitenkarte");
                    for d in &sc.card.payload.drops {
                        descriptors.push((d.clone().into(), author.to_string()));
                    }
                }

                let policy = state
                    .get_or_load_security_policy(service, pw)
                    .map_err(FrontendError::from)?;
                Ok((descriptors, policy))
            } else {
                // timeout == 0 graceful cache fallback (no master key in RAM)
                if let Some(Some(desc)) =
                    state.read_cache_slot_for_service(service, |c| &c.own_sync_descriptor)
                {
                    if !desc.read_url.is_empty() && !desc.write_url.is_empty() {
                        let mut fe_desc: FrontendSyncDescriptor = desc.into();
                        fe_desc.status = "active".to_string();
                        descriptors.push((fe_desc, "Eigener Sync-Dienst".to_string()));
                    }
                }
                if let Some(ab) = state.read_cache_slot_for_service(service, |c| &c.contacts) {
                    for c in ab.contacts.values() {
                        let name = contact_display_label(c);
                        for d in &c.syncs {
                            descriptors.push((d.clone(), name.clone()));
                        }
                    }
                }
                let policy = state
                    .read_cache_slot_for_service(service, |c| &c.security_policy)
                    .unwrap_or_default();
                Ok((descriptors, policy))
            }
        },
    )?;

    // 4. Already selected upload drops
    for sel in &security_policy.market_selected_syncs {
        descriptors.push((sel.clone(), "Ausgewählter Sync-Dienst".to_string()));
    }

    let mut deduped: BTreeMap<String, (FrontendSyncDescriptor, String)> = BTreeMap::new();
    for (d, label) in descriptors {
        let key = if !d.write_url.is_empty() {
            normalize_sync_url(&d.write_url)
        } else {
            normalize_sync_url(&d.read_url)
        };
        if key.is_empty() {
            continue;
        }
        match deduped.entry(key) {
            std::collections::btree_map::Entry::Vacant(v) => {
                v.insert((d, label));
            }
            std::collections::btree_map::Entry::Occupied(mut o) => {
                let (existing, _) = o.get_mut();
                if d.status == "deprecating" || d.status == "revoked" {
                    existing.status = d.status;
                    existing.sunset_at = d.sunset_at;
                    existing.revocation_reason = d.revocation_reason;
                    existing.replacement = d.replacement;
                } else if (existing.status.is_empty() || existing.status == "unverified")
                    && !d.status.is_empty()
                    && d.status != "unverified"
                {
                    existing.status = d.status;
                }
            }
        }
    }

    let mut results = Vec::new();
    for (_, (mut desc, label)) in deduped {
        let is_selected = security_policy
            .market_selected_syncs
            .iter()
            .any(|d| descriptors_overlap(d, &desc));

        let status = if desc.status.is_empty() {
            "unverified".to_string()
        } else {
            desc.status.clone()
        };
        let is_authorized = status == "active" || status == "deprecating";
        desc.status = status.clone();

        results.push(FrontendDiscoveredSync {
            descriptor: desc,
            is_authorized,
            is_selected,
            source_label: label,
            status,
        });
    }

    Ok(results)
}

fn should_skip_endpoint_probe(
    ep: &FrontendSyncDescriptor,
    initial_has_healthy: bool,
    force: bool,
    now: u64,
) -> bool {
    if initial_has_healthy && !force {
        let needs_initial_probe =
            ep.status == "unverified" || ep.read_url.is_empty() || ep.drop_pub.is_empty();
        return !needs_initial_probe;
    }
    if !force {
        if let Some(last_probed) = ep.last_probed_at {
            if last_probed > now {
                return false;
            }
            let elapsed = now - last_probed;
            return match ep.status.as_str() {
                "unverified" => false,
                "pending_auth" => {
                    let min_wait = if ep.fail_count > 3 { 300 } else { 60 };
                    elapsed < min_wait
                }
                "unreachable" => {
                    let exp = ep.fail_count.min(4);
                    let backoff = (60u64 * (1u64 << exp)).min(900);
                    elapsed < backoff
                }
                "revoked" => elapsed < 3600,
                _ => elapsed < 60,
            };
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
async fn probe_single_contact_endpoint(
    client: &reqwest::Client,
    id_ctx: &SyncIdentityContext,
    ep: &mut FrontendSyncDescriptor,
    now: u64,
    probe_cache: &mut HashMap<String, ProbeCacheEntry>,
    verified_revocations: &mut Vec<(String, SyncRevocationNotice)>,
    channel_max_bytes_updates: &mut Vec<(String, String, usize)>,
) -> bool {
    let target_url = if !ep.write_url.is_empty() {
        ep.write_url.clone()
    } else {
        ep.read_url.clone()
    };
    if target_url.is_empty() {
        return false;
    }

    let canonical_write = humoco_market::canonicalize_write_url(&target_url).unwrap_or(target_url);
    let sync_endpoint_key = normalize_sync_url(&canonical_write);
    let pinned_drop_pub = canonical_drop_pub_hex(&ep.drop_pub).ok();
    let cache_key = match &pinned_drop_pub {
        Some(dp) => format!("{}::{}", sync_endpoint_key, dp),
        None => sync_endpoint_key.clone(),
    };

    if let Some((c_read, c_drop, c_status, c_ok, c_rev)) = probe_cache.get(&cache_key) {
        if *c_ok {
            ep.write_url = canonical_write;
            ep.read_url = c_read.clone();
            if ep.drop_pub.trim().is_empty()
                || ep.drop_pub.trim().eq_ignore_ascii_case(c_drop.trim())
            {
                ep.drop_pub = c_drop.clone();
            }
            let sticky_revoked = is_revocation_effective(ep, now);
            if !sticky_revoked {
                ep.status = c_status.clone();
            } else if !ep.status.eq_ignore_ascii_case("revoked") {
                ep.status = "revoked".to_string();
            }
            ep.fail_count = 0;
            ep.last_seen_at = Some(now);
            ep.last_probed_at = Some(now);
            if let Some(ref rev) = c_rev {
                apply_revocation_notice(ep, rev, now);
                verified_revocations.push((c_read.clone(), rev.clone()));
            } else {
                clear_revocation(ep, c_status, now);
            }
        } else {
            if ep.status != "revoked" && ep.status != "deprecating" {
                ep.status = c_status.clone();
            }
            ep.fail_count = ep.fail_count.saturating_add(1);
            ep.last_probed_at = Some(now);
            if let Some(ref rev) = c_rev {
                apply_revocation_notice(ep, rev, now);
                verified_revocations.push((c_read.clone(), rev.clone()));
            }
        }
        return true;
    }

    match sync_client::fetch_sync_info(client, &canonical_write).await {
        Err(e) => {
            log::warn!("Sync probe ?info failed for {}: {}", canonical_write, e);
            if e.is_local_validation() {
                return false;
            }
            if ep.status != "revoked" && ep.status != "deprecating" {
                ep.status = "unreachable".to_string();
            }
            ep.fail_count = ep.fail_count.saturating_add(1);
            ep.last_probed_at = Some(now);
            probe_cache.insert(
                cache_key,
                (
                    ep.read_url.clone(),
                    ep.drop_pub.clone(),
                    "unreachable".to_string(),
                    false,
                    None,
                ),
            );
            true
        }
        Ok(info) => {
            let parsed_info_drop_pub = canonical_drop_pub_hex(&info.drop_pub).ok();

            if let Some(ref pinned) = pinned_drop_pub {
                if parsed_info_drop_pub.as_ref() != Some(pinned) {
                    let verified_pinned_rev = info.revocation.as_ref().and_then(|n| {
                        sync_client::verify_revocation_notice(n, pinned)
                            .ok()
                            .map(|_| n.clone())
                    });
                    if let Some(ref notice) = verified_pinned_rev {
                        apply_revocation_notice(ep, notice, now);
                        ep.fail_count = 0;
                        ep.last_seen_at = Some(now);
                        ep.last_probed_at = Some(now);
                        verified_revocations.push((ep.read_url.clone(), notice.clone()));
                        probe_cache.insert(
                            cache_key,
                            (
                                ep.read_url.clone(),
                                pinned.clone(),
                                ep.status.clone(),
                                true,
                                verified_pinned_rev,
                            ),
                        );
                        return true;
                    }
                    log::warn!(
                        "Sync probe key pin mismatch for {}: pinned {}, got '{}'",
                        canonical_write,
                        pinned,
                        info.drop_pub
                    );
                    if ep.status != "revoked" && ep.status != "deprecating" {
                        ep.status = "unreachable".to_string();
                    }
                    ep.fail_count = ep.fail_count.saturating_add(1);
                    ep.last_probed_at = Some(now);
                    probe_cache.insert(
                        cache_key,
                        (
                            ep.read_url.clone(),
                            pinned.clone(),
                            "unreachable".to_string(),
                            false,
                            None,
                        ),
                    );
                    return true;
                }
            }

            let effective_drop_pub = pinned_drop_pub
                .clone()
                .or(parsed_info_drop_pub)
                .unwrap_or_default();
            let verified_rev = if !effective_drop_pub.is_empty() {
                info.revocation.as_ref().and_then(|n| {
                    sync_client::verify_revocation_notice(n, &effective_drop_pub)
                        .ok()
                        .map(|_| n.clone())
                })
            } else {
                None
            };
            let trimmed_read = normalize_sync_url(&info.read_url);
            let read_url_valid = !trimmed_read.is_empty()
                && sync_client::validate_sync_read_url(&trimmed_read).is_ok();
            if !read_url_valid {
                log::warn!(
                    "Sync server at {} returned invalid read_url '{}' in ?info",
                    canonical_write,
                    info.read_url
                );
            }
            let resolved_read_url = if read_url_valid {
                trimmed_read
            } else {
                ep.read_url.clone()
            };

            ep.write_url = canonical_write.clone();
            ep.read_url = resolved_read_url.clone();
            if !effective_drop_pub.is_empty() {
                ep.drop_pub = effective_drop_pub.clone();
            }
            ep.last_seen_at = Some(now);

            let sticky_revoked = is_revocation_effective(ep, now);
            if verified_rev.is_none() {
                clear_revocation(ep, "unverified", now);
            }

            // Always derive OUR OWN stealth pubkey + prefix rather than probing a contact's foreign slot_id.
            let stealth_pk_res = if effective_drop_pub.is_empty() {
                Err(humoco_market::MarketError::Validation(
                    "Sync server ?info missing valid drop_pub".to_string(),
                ))
            } else {
                humoco_market::derive_member_stealth_pubkey(&id_ctx.my_ed_sk, &effective_drop_pub)
            };

            let probe_status: String;
            match stealth_pk_res {
                Err(e) => {
                    log::warn!(
                        "Sync probe failed to derive stealth pubkey for {} (drop_pub='{}'): {}",
                        canonical_write,
                        ep.drop_pub,
                        e
                    );
                    // Server responded to ?info (reachable), so report pending_auth rather than offline/unreachable.
                    probe_status = "pending_auth".to_string();
                    if sticky_revoked && verified_rev.is_none() {
                        ep.status = "revoked".to_string();
                    } else {
                        ep.status = "pending_auth".to_string();
                    }
                    ep.fail_count = 0;
                    ep.last_probed_at = Some(now);
                }
                Ok(stealth_pk) => {
                    let cap_prefix = id_ctx.my_prefix.as_deref();
                    let card_cap = sync_client::head_slot_capability(
                        client,
                        &canonical_write,
                        &stealth_pk,
                        cap_prefix,
                        "card",
                    )
                    .await;
                    if let Err(ref e) = card_cap {
                        log::warn!(
                            "Sync probe HEAD capability (card) failed on {}: {}",
                            canonical_write,
                            e
                        );
                    }
                    if let Ok(ref cap) = card_cap {
                        if let Some(mb) = cap.max_bytes.filter(|&v| v > 0) {
                            channel_max_bytes_updates.push((
                                canonical_write.clone(),
                                "card".to_string(),
                                mb,
                            ));
                        }
                    }
                    let post_cap = if card_cap.as_ref().map(|c| c.authorized).unwrap_or(false) {
                        Ok(sync_client::SyncCapabilityResult {
                            authorized: true,
                            ..Default::default()
                        })
                    } else {
                        let res = sync_client::head_slot_capability(
                            client,
                            &canonical_write,
                            &stealth_pk,
                            cap_prefix,
                            "post",
                        )
                        .await;
                        if let Err(ref e) = res {
                            log::warn!(
                                "Sync probe HEAD capability (post) failed on {}: {}",
                                canonical_write,
                                e
                            );
                        }
                        res
                    };

                    let is_authorized = card_cap.as_ref().map(|c| c.authorized).unwrap_or(false)
                        || post_cap.as_ref().map(|c| c.authorized).unwrap_or(false);

                    if is_authorized && read_url_valid {
                        probe_status = "active".to_string();
                        if sticky_revoked && verified_rev.is_none() {
                            ep.status = "revoked".to_string();
                        } else {
                            ep.status = "active".to_string();
                        }
                    } else {
                        // Server answered ?info (reachable), so never classify as offline/unreachable.
                        probe_status = "pending_auth".to_string();
                        if sticky_revoked && verified_rev.is_none() {
                            ep.status = "revoked".to_string();
                        } else {
                            ep.status = "pending_auth".to_string();
                        }
                    }
                    ep.fail_count = 0;
                    ep.last_probed_at = Some(now);
                }
            }

            if let Some(ref notice) = verified_rev {
                apply_revocation_notice(ep, notice, now);
                verified_revocations.push((resolved_read_url.clone(), notice.clone()));
            }

            probe_cache.insert(
                cache_key,
                (
                    resolved_read_url,
                    effective_drop_pub,
                    probe_status,
                    true,
                    verified_rev,
                ),
            );
            true
        }
    }
}

fn apply_probe_result_fields(target: &mut FrontendSyncDescriptor, probed: &FrontendSyncDescriptor) {
    target.write_url = probed.write_url.clone();
    target.read_url = probed.read_url.clone();
    // Preserve target.slot_id; never inherit a contact's foreign slot_id.
    if target.drop_pub.trim().is_empty()
        || target
            .drop_pub
            .trim()
            .eq_ignore_ascii_case(probed.drop_pub.trim())
    {
        target.drop_pub = probed.drop_pub.clone();
    }
    target.last_seen_at = probed.last_seen_at;
    target.last_error_at = probed.last_error_at;
    target.fail_count = probed.fail_count;
    target.status = probed.status.clone();
    target.last_probed_at = probed.last_probed_at;
    target.sunset_at = probed.sunset_at;
    target.revocation_reason = probed.revocation_reason.clone();
    target.replacement = probed.replacement.clone();
}

pub async fn probe_contact_syncs_impl(
    force: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    let auth =
        state.authorize_session(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    probe_contact_syncs_with_session(&auth.token, force, auth.pw(), state).await
}

pub(crate) async fn probe_contact_syncs_with_session(
    session: &crate::session::SessionToken,
    force: bool,
    password: Option<&str>,
    state: &AppState,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    let (id_ctx, initial_policy, has_own_sync, mut probe_targets) = state.with_session_required(
        session,
        "Wallet session changed before sync probe started",
        |service| {
            let id_ctx = SyncIdentityContext::from_service(service, password)?;
            let s = state
                .get_or_load_security_policy(service, password)
                .map_err(FrontendError::from)?;
            let own = state
                .get_or_load_own_sync_descriptor(service, password)?
                .is_some_and(|d| !d.write_url.is_empty() && !d.read_url.is_empty());
            let ab: FrontendAddressBook =
                crate::state::load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)
                    .map_err(FrontendError::from)?;
            let targets: Vec<(String, String, FrontendSyncDescriptor)> = ab
                .contacts
                .into_iter()
                .flat_map(|(c_key, c)| {
                    c.syncs.into_iter().map(move |ep| {
                        let norm = normalize_sync_url(&ep.write_url);
                        (c_key.clone(), norm, ep)
                    })
                })
                .collect();
            Ok((id_ctx, s, own, targets))
        },
    )?;

    let has_active_friend_sync = initial_policy.market_selected_syncs.iter().any(|d| {
        d.status == "active"
            && d.fail_count == 0
            && !d.read_url.is_empty()
            && !d.drop_pub.is_empty()
    });
    let has_healthy_public_sync = initial_policy.public_syncs.iter().any(|pd| {
        pd.enabled
            && !is_endpoint_revoked(&pd.status)
            && !pd.authorized_channels.is_empty()
            && !pd.read_url.is_empty()
    });

    let initial_has_healthy = has_own_sync || has_active_friend_sync || has_healthy_public_sync;

    let now = sync_client::current_unix_timestamp();
    let client = sync_client::create_sync_probe_client();
    let mut probed_updates: Vec<(String, String, FrontendSyncDescriptor)> = Vec::new();
    let mut probed_endpoints: Vec<FrontendSyncDescriptor> = Vec::new();
    let mut probe_cache: HashMap<String, ProbeCacheEntry> = HashMap::new();
    let mut verified_revocations: Vec<(String, SyncRevocationNotice)> = Vec::new();
    let mut channel_max_bytes_updates: Vec<(String, String, usize)> = Vec::new();

    for (contact_key, orig_norm_write, ep) in &mut probe_targets {
        if !state.is_generation_current(session.generation) {
            return Ok(Vec::new());
        }
        if should_skip_endpoint_probe(ep, initial_has_healthy, force, now) {
            continue;
        }

        if probe_single_contact_endpoint(
            &client,
            &id_ctx,
            ep,
            now,
            &mut probe_cache,
            &mut verified_revocations,
            &mut channel_max_bytes_updates,
        )
        .await
        {
            probed_updates.push((contact_key.clone(), orig_norm_write.clone(), ep.clone()));
            probed_endpoints.push(ep.clone());
        }
    }

    let final_book_opt = state.with_session(session, |service| {
        let mut fresh_ab: FrontendAddressBook =
            crate::state::load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)
                .map_err(FrontendError::from)?;
        let mut contacts_modified = false;

        for (contact_key, orig_norm_write, probed_ep) in &probed_updates {
            if let Some(contact) = fresh_ab.contacts.get_mut(contact_key) {
                if let Some(ep) = contact.syncs.iter_mut().find(|existing| {
                    normalize_sync_url(&existing.write_url) == *orig_norm_write
                        && (existing.drop_pub.trim().is_empty()
                            || probed_ep.drop_pub.trim().is_empty()
                            || existing
                                .drop_pub
                                .trim()
                                .eq_ignore_ascii_case(probed_ep.drop_pub.trim()))
                }) {
                    apply_probe_result_fields(ep, probed_ep);
                    contacts_modified = true;
                }
            }
        }

        for (rev_read_url, notice) in &verified_revocations {
            if apply_revocation_notice_to_address_book(&mut fresh_ab, rev_read_url, notice, now) {
                contacts_modified = true;
            }
        }

        if contacts_modified {
            crate::state::save_encrypted_json(service, CONTACTS_DATA_NAME, &fresh_ab, password)
                .map_err(FrontendError::from)?;
            state.set_cached_contacts_for_service(service, Some(fresh_ab.clone()));
        }
        Ok(fresh_ab)
    })?;

    let Some(final_book) = final_book_opt else {
        return Ok(Vec::new());
    };

    if !probed_endpoints.is_empty() {
        let mut auto_activated = false;
        let updated_policy =
            state.mutate_security_policy_for_session(session, password, |policy| {
                let mut modified = false;
                let mut current_healthy = has_own_sync
                    || policy.market_selected_syncs.iter().any(|d| {
                        d.status == "active"
                            && d.fail_count == 0
                            && !d.read_url.is_empty()
                            && !d.drop_pub.is_empty()
                    })
                    || policy.public_syncs.iter().any(|pd| {
                        pd.enabled
                            && !is_endpoint_revoked(&pd.status)
                            && !pd.authorized_channels.is_empty()
                            && !pd.read_url.is_empty()
                    });
                for ep in &probed_endpoints {
                    for sel in &mut policy.market_selected_syncs {
                        if descriptors_compatible_for_policy(sel, ep) {
                            apply_probe_result_fields(sel, ep);
                            modified = true;
                        }
                    }

                    if !current_healthy
                        && ep.status == "active"
                        && !ep.read_url.is_empty()
                        && !ep.drop_pub.is_empty()
                    {
                        let already_selected = policy
                            .market_selected_syncs
                            .iter()
                            .any(|s| descriptors_compatible_for_policy(s, ep));
                        if !already_selected && policy.market_selected_syncs.len() < 3 {
                            policy.market_selected_syncs.push(FrontendSyncDescriptor {
                                write_url: ep.write_url.clone(),
                                read_url: ep.read_url.clone(),
                                slot_id: None,
                                drop_pub: ep.drop_pub.clone(),
                                last_seen_at: ep.last_seen_at,
                                last_error_at: None,
                                fail_count: 0,
                                status: "active".to_string(),
                                last_probed_at: Some(now),
                                ..Default::default()
                            });
                            current_healthy = true;
                            modified = true;
                            auto_activated = true;
                        }
                    }
                }
                modified
            })?;
        if auto_activated {
            if let Some(policy) = updated_policy {
                let _ = state.with_session(session, |service| {
                    sync_own_card_drops_on_settings_change(
                        state,
                        service,
                        &policy.market_selected_syncs,
                        password,
                    );
                    Ok(())
                });
            }
        }
    }

    save_channel_max_bytes_updates(state, session, password, &channel_max_bytes_updates);

    let mut all_syncs = Vec::new();
    for c in final_book.contacts.values() {
        all_syncs.extend(c.syncs.clone());
    }
    Ok(all_syncs)
}

pub async fn set_selected_upload_syncs_impl(
    syncs: Vec<FrontendSyncDescriptor>,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    let auth = state.authorize_session(crate::state::RequiredAccess::Full, password.as_deref())?;
    if syncs.len() > 3 {
        return Err(FrontendError::from(
            "Maximum 3 upload drops allowed".to_string(),
        ));
    }

    for d in &syncs {
        validate_upload_sync_descriptor(d)?;
    }

    {
        // Serialize SecurityPolicy + OwnMarketProfile RMW under market_op_lock, and drop it before
        // enqueue_guardian_locator_updates_if_configured (which acquires recovery_op_lock -> post_op_lock).
        let _market_guard = state.market_op_lock.lock().await;
        state.with_session_required(
            &auth.token,
            "Active wallet session changed before upload syncs could be saved",
            |service| {
                let mut policy = state
                    .get_or_load_security_policy(service, auth.pw())
                    .map_err(FrontendError::from)?;
                policy.market_selected_syncs = syncs.clone();

                let updated_profile = super::admin::build_resigned_own_card_profile_for_policy(
                    state,
                    service,
                    &policy,
                    auth.pw(),
                );

                let mut batch = crate::state::EncryptedBatchCommit::new();
                batch.security_policy(policy);
                if let Some(prof) = updated_profile {
                    batch.own_market_profile(prof);
                }
                batch
                    .commit(state, service, auth.pw())
                    .map_err(FrontendError::from)?;
                Ok(())
            },
        )?;
    }

    if state.is_generation_current(auth.generation()) {
        let _ = enqueue_guardian_locator_updates_if_configured(state, &auth.token, auth.pw()).await;
    }

    // Trigger upload to selected drops.
    if state.is_generation_current(auth.generation()) {
        let _ = super::sync_market_channels_impl(
            None,
            Some(true),
            auth.pw().map(str::to_string),
            state,
        )
        .await;
    }

    Ok(syncs)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn import_decrypted_card_bundle(
    bundle: humoco_market::CardBundle,
    read_url: &str,
    now_secs: u64,
    max_stored_cards: usize,
    service: &AppService,
    store: &mut humoco_market::CardStore,
    sync_state: &mut StoredSyncState,
    address_book: &mut FrontendAddressBook,
    contacts_changed: &mut bool,
) -> (usize, usize) {
    if let Err(e) = humoco_market::verify_card_bundle(&bundle) {
        log::warn!(
            "Rejected decrypted CardBundle from {} with invalid writer signature: {}",
            read_url,
            e
        );
        return (0, 0);
    }

    let mut count = 0;
    let mut new_ads = 0;
    let mut before_ads_by_did: HashMap<String, HashSet<String>> = HashMap::new();
    for sc in store.cards.values() {
        let ids: HashSet<String> = humoco_market::sd_card::active_ads(&sc.card, now_secs)
            .into_iter()
            .map(|a| a.id)
            .collect();
        before_ads_by_did.insert(sc.card.payload.recipient_id().to_string(), ids);
    }

    for env in &bundle.envelopes {
        let outcome = service.with_wallet(|wallet| {
            let is_offender =
                |did: &str| matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_));
            let own_coords = wallet
                .profile
                .coordinates
                .as_deref()
                .and_then(humoco_market::parse_coordinates_pair);
            humoco_market::accept_envelope_with_writer(
                store,
                env,
                Some(bundle.writer_did.as_str()),
                now_secs,
                max_stored_cards,
                own_coords,
                &is_offender,
            )
        });

        if let Ok(humoco_market::AcceptOutcome::Accepted { did, .. }) = outcome {
            count += 1;
            sync_state
                .sync_sources_by_did
                .insert(did.clone(), read_url.to_string());

            if let Some(stored) = store.cards.get(&did) {
                let prev_ids = before_ads_by_did.entry(did.clone()).or_default();
                for ad in humoco_market::sd_card::active_ads(&stored.card, now_secs) {
                    if prev_ids.insert(ad.id) {
                        new_ads += 1;
                    }
                }
                let card_syncs = sanitize_untrusted_sync_descriptors(
                    &stored.card.payload.drops,
                    humoco_market::MAX_CARD_DROPS,
                );
                let card_profile = humoco_market::extract_public_profile(&stored.card);
                if crate::state::apply_contact_update_from_card(
                    address_book,
                    &did,
                    &card_syncs,
                    Some(&card_profile),
                ) {
                    *contacts_changed = true;
                }
            }
        }
    }
    (count, new_ads)
}

fn load_card_upload_context(
    state: &AppState,
    session: &SessionToken,
    ui_prefs: &UiPreferences,
    id_ctx: &SyncIdentityContext,
    password: Option<&str>,
) -> Result<CardUploadContext, FrontendError> {
    state.with_session_required(
        session,
        "Wallet session changed during card upload preparation",
        |service| {
            let my_can_did = canonicalize_did(&id_ctx.my_did).ok();
            let own_profile_opt: Option<StoredOwnMarketProfile> = Some(
                crate::services::market_service::load_own_market_profile_state(
                    state, service, password,
                )?,
            );
            // Include unexpired <jws>~ tombstone cards even when `published == false`.
            let own_card_opt = own_profile_opt.as_ref().and_then(|p| p.last_card.clone());

            let reg = state.get_or_load_wot_register(service, password).ok();
            let address_book: FrontendAddressBook =
                crate::state::load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)
                    .map_err(FrontendError::from)?;

            let mut readers_by_pk: HashMap<[u8; 32], (u8, ed25519_dalek::VerifyingKey)> =
                HashMap::new();
            let mut evicted_reader_keys = Vec::new();
            if let Some(r) = reg {
                for (did, entry) in &r.own_contacts {
                    if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(did) {
                        let offender = is_known_offender(service, did);
                        if entry.level >= ui_prefs.share_min_level && !offender {
                            let bytes = *pk.as_bytes();
                            readers_by_pk
                                .entry(bytes)
                                .and_modify(|(lvl, _)| {
                                    if entry.level > *lvl {
                                        *lvl = entry.level;
                                    }
                                })
                                .or_insert((entry.level, pk));
                        } else if !offender {
                            evicted_reader_keys.push(pk);
                        }
                    }
                }
            }
            let mut prioritized_readers: Vec<(u8, ed25519_dalek::VerifyingKey)> =
                readers_by_pk.into_values().collect();
            prioritized_readers.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| a.1.as_bytes().cmp(b.1.as_bytes()))
            });
            prioritized_readers.truncate(humoco_market::MAX_SLOT_RECIPIENTS);
            let mut readers: Vec<ed25519_dalek::VerifyingKey> =
                prioritized_readers.into_iter().map(|(_, pk)| pk).collect();
            readers.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            readers.dedup();
            let reader_set: HashSet<[u8; 32]> = readers.iter().map(|k| *k.as_bytes()).collect();
            for contact in address_book.contacts.values() {
                let can = canonicalize_did(&contact.did).unwrap_or_else(|_| contact.did.clone());
                if my_can_did.as_deref() == Some(can.as_str()) {
                    continue;
                }
                if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(&can) {
                    if !reader_set.contains(pk.as_bytes()) && !is_known_offender(service, &can) {
                        evicted_reader_keys.push(pk);
                    }
                }
            }
            evicted_reader_keys.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            evicted_reader_keys.dedup();
            evicted_reader_keys.truncate(humoco_market::MAX_SLOT_RECIPIENTS);

            let mut hasher = sha2::Sha256::new();
            hasher.update(b"active");
            for k in &readers {
                hasher.update(k.as_bytes());
            }
            hasher.update(b"evicted");
            for k in &evicted_reader_keys {
                hasher.update(k.as_bytes());
            }
            let readers_hash = hex::encode(hasher.finalize());

            Ok(CardUploadContext {
                own_card_opt,
                own_profile_opt,
                readers,
                evicted_reader_keys,
                readers_hash,
            })
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn build_delta_card_blob(
    desired_versions: &BTreeMap<String, u64>,
    prev_base_versions: Option<&BTreeMap<String, u64>>,
    prev_base_size: Option<usize>,
    own_wire_info: Option<&(String, String, u64)>,
    card_by_did: &HashMap<String, &humoco_market::StoredCard>,
    own_coords: Option<(f64, f64)>,
    now_secs: u64,
    trust_level_of: &dyn Fn(&str) -> u8,
    readers: &[ed25519_dalek::VerifyingKey],
    effective_card_max_bytes: usize,
    writer_did: &str,
    writer_signing_key: &ed25519_dalek::SigningKey,
) -> Option<Vec<u8>> {
    let base_vers = prev_base_versions?;
    let any_removed = base_vers
        .keys()
        .any(|did| !desired_versions.contains_key(did));
    let diff_dids: HashSet<String> = desired_versions
        .iter()
        .filter(|(did, v)| base_vers.get(*did) != Some(*v))
        .map(|(did, _)| did.clone())
        .collect();
    if any_removed || diff_dids.is_empty() {
        return None;
    }

    let delta_own_wire = own_wire_info
        .filter(|(did, _, _)| diff_dids.contains(did))
        .map(|(_, w, _)| w.as_str());
    let delta_cards: Vec<&humoco_market::StoredCard> = diff_dids
        .iter()
        .filter_map(|did| card_by_did.get(did).copied())
        .collect();
    let delta_bundle = humoco_market::build_card_bundle(
        delta_own_wire,
        &delta_cards,
        own_coords,
        now_secs,
        trust_level_of,
        readers.len(),
        effective_card_max_bytes,
        writer_did,
        writer_signing_key,
    )
    .ok()?;
    let base_sz = prev_base_size.unwrap_or(0);
    humoco_market::seal_slot_blob(&delta_bundle, readers, effective_card_max_bytes)
        .ok()
        .filter(|blob| {
            blob.len() <= DELTA_MAX_BYTES
                && blob.len().saturating_mul(4) <= base_sz
                && base_sz.saturating_add(blob.len()) <= effective_card_max_bytes
        })
}

fn seal_base_card_blob(
    use_tomb_only: bool,
    tomb_bytes_opt: Option<&Vec<u8>>,
    full_bundle: &humoco_market::CardBundle,
    readers: &[ed25519_dalek::VerifyingKey],
    evicted_reader_keys: &[ed25519_dalek::VerifyingKey],
    effective_card_max_bytes: usize,
) -> Result<Vec<u8>, humoco_market::MarketError> {
    match (use_tomb_only, tomb_bytes_opt) {
        (true, Some(tb)) => {
            humoco_market::seal_slot_payload(tb, evicted_reader_keys, effective_card_max_bytes)
        }
        (false, Some(tb)) if !full_bundle.envelopes.is_empty() => {
            let fb = humoco_market::encode_signed_card_bundle(full_bundle)?;
            humoco_market::seal_multi_class_slot_blob(
                &[
                    (fb.as_slice(), readers),
                    (tb.as_slice(), evicted_reader_keys),
                ],
                effective_card_max_bytes,
            )
        }
        (false, Some(tb)) => {
            humoco_market::seal_slot_payload(tb, evicted_reader_keys, effective_card_max_bytes)
        }
        (_, None) => humoco_market::seal_slot_blob(full_bundle, readers, effective_card_max_bytes),
    }
}

#[allow(clippy::too_many_arguments)]
async fn delete_stale_card_slots(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    id_ctx: &SyncIdentityContext,
    upload_targets: &[humoco_market::DropDescriptor],
    now_secs: u64,
    sync_state: &mut StoredSyncState,
    errors: &mut super::SyncErrorTracker,
) -> bool {
    let delta_prefix = humoco_market::derive_delta_prefix(id_ctx.my_prefix.as_deref());
    let mut all_targets_succeeded = true;

    for drop_desc in upload_targets {
        if !state.is_generation_current(session.generation) {
            return false;
        }
        let target_key = normalize_sync_url(&drop_desc.write_url);
        if target_key.is_empty() {
            continue;
        }
        if state.is_origin_cooling_down(&drop_desc.write_url, std::time::Instant::now()) {
            all_targets_succeeded = false;
            errors.push(format!(
                "Card slot DELETE on {} deferred: server origin cooling down after HTTP 429",
                drop_desc.write_url
            ));
            continue;
        }
        let signer = DropChannelSigner {
            is_public: false,
            master_seed: &id_ctx.master_seed,
            my_ed_sk: &id_ctx.my_ed_sk,
            drop_desc,
        };
        let mut target_ok = true;
        for pfx in [id_ctx.my_prefix.as_deref(), Some(delta_prefix.as_str())] {
            match signer.sign_request(
                "DELETE",
                "card",
                pfx,
                now_secs,
                &sync_client::generate_sync_nonce(),
                b"",
            ) {
                Ok(signed) => {
                    if let Err(e) =
                        sync_client::delete_slot(client, &drop_desc.write_url, &signed, "card")
                            .await
                    {
                        target_ok = false;
                        all_targets_succeeded = false;
                        errors.record_sync_error(
                            state,
                            &drop_desc.write_url,
                            &e,
                            format!(
                                "Failed to delete stale card slot on {}: {}",
                                drop_desc.write_url, e
                            ),
                        );
                    }
                }
                Err(e) => {
                    target_ok = false;
                    all_targets_succeeded = false;
                    errors.push(format!(
                        "Failed to sign stale card DELETE for {}: {}",
                        drop_desc.write_url, e
                    ));
                }
            }
        }
        if target_ok {
            sync_state
                .card_base_uploaded_at_by_write_url
                .remove(&target_key);
            sync_state.card_base_size_by_write_url.remove(&target_key);
            sync_state
                .card_base_versions_by_write_url
                .remove(&target_key);
            sync_state
                .card_delta_active_by_write_url
                .remove(&target_key);
            sync_state
                .card_last_uploaded_versions_by_write_url
                .remove(&target_key);
        }
    }

    if all_targets_succeeded {
        sync_state.last_readers_hash = None;
    }
    true
}

/// Uploads encrypted business card bundles (base, delta, and revocation tombstones) plus active ad media blobs.
/// Returns the set of currently active ad media `blob_id`s for downstream orphan GC.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync_card_upload_channel(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    ui_prefs: &UiPreferences,
    security_policy: &SecurityPolicy,
    id_ctx: &SyncIdentityContext,
    force_upload: Option<bool>,
    now_secs: u64,
    password: Option<&str>,
    sync_state: &mut StoredSyncState,
    uploaded_to: &mut usize,
    errors: &mut super::SyncErrorTracker,
) -> Option<HashSet<String>> {
    if !ui_prefs.market_sync_enabled {
        return None;
    }
    let upload_targets = collect_upload_targets_for_channel(security_policy, "card");
    if upload_targets.is_empty() {
        return None;
    }
    let ctx = match load_card_upload_context(state, session, ui_prefs, id_ctx, password) {
        Ok(c) => c,
        Err(e) => {
            errors.push(format!("Card upload context failed: {}", e.message));
            return None;
        }
    };
    let active_own_card = ctx
        .own_card_opt
        .as_ref()
        .filter(|c| now_secs < c.payload.valid_until);
    let mut active_media_blob_ids: HashSet<String> = HashSet::new();
    let has_stale_slots = sync_state.last_readers_hash.is_some()
        || !sync_state.card_base_uploaded_at_by_write_url.is_empty()
        || !sync_state.card_delta_active_by_write_url.is_empty();

    if !ctx.readers.is_empty() || (active_own_card.is_some() && !ctx.evicted_reader_keys.is_empty())
    {
        let own_wire_info: Option<(String, String, u64)> = active_own_card.and_then(|own_card| {
            let own_key = own_card.payload.recipient_id().to_string();
            humoco_market::render_wire(own_card, 1, humoco_market::WireFilter::Full)
                .map(|wire| (own_key, wire, own_card.payload.v))
        });
        let own_tombstone_bundle: Option<humoco_market::CardBundle> =
            active_own_card.and_then(|own_card| {
                humoco_market::sign_card_bundle(
                    vec![humoco_market::CardEnvelope {
                        hops: 1,
                        card_wire: format!("{}~", own_card.jws),
                    }],
                    &id_ctx.my_did,
                    &id_ctx.my_ed_sk,
                )
                .ok()
            });

        let load_res = state.with_session_required(
            session,
            "Wallet session changed during card upload data load",
            |service| {
                let store: humoco_market::CardStore = crate::state::load_encrypted_json_or_default(
                    service,
                    MARKET_CARD_STORE_KEY,
                    password,
                )
                .map_err(FrontendError::from)?;
                let reg = state.get_or_load_wot_register(service, password).ok();
                let own_coords = service
                    .with_wallet(|wallet| {
                        wallet
                            .profile
                            .coordinates
                            .as_deref()
                            .and_then(humoco_market::parse_coordinates_pair)
                    })
                    .ok()
                    .flatten();

                let mut trust_by_did = HashMap::new();
                let mut offenders = HashSet::new();
                if let Some(ref r) = reg {
                    for (did, entry) in &r.own_contacts {
                        let can = canonicalize_did(did).unwrap_or_else(|_| did.clone());
                        trust_by_did.insert(can, entry.level);
                    }
                }
                for sc in store.cards.values() {
                    let can = canonicalize_did(&sc.card.payload.did)
                        .unwrap_or_else(|_| sc.card.payload.did.clone());
                    if is_known_offender(service, &sc.card.payload.did) {
                        offenders.insert(can);
                    }
                }

                let media_blobs = if !ctx.readers.is_empty() {
                    seal_active_ad_media_blobs(
                        service,
                        active_own_card,
                        ctx.own_profile_opt.as_ref(),
                        now_secs,
                        password,
                    )?
                } else {
                    Vec::new()
                };
                Ok((store, own_coords, trust_by_did, offenders, media_blobs))
            },
        );

        let (store, own_coords, trust_by_did, offenders, active_media_blobs) = match load_res {
            Ok(tuple) => tuple,
            Err(e) => {
                errors.push(format!("Card upload data load failed: {}", e.message));
                return None;
            }
        };

        let trust_level_of = |did: &str| -> u8 {
            let can = canonicalize_did(did).unwrap_or_else(|_| did.to_string());
            trust_by_did.get(&can).copied().unwrap_or(0)
        };

        let my_can_did = canonicalize_did(&id_ctx.my_did).ok();
        let mut wire_meta: HashMap<String, (String, u64)> = HashMap::new();
        let mut card_by_did: HashMap<String, &humoco_market::StoredCard> = HashMap::new();

        if let Some((ref own_did, ref own_wire, own_v)) = own_wire_info {
            wire_meta.insert(own_wire.clone(), (own_did.clone(), own_v));
        }

        let mut forward_candidates: Vec<&humoco_market::StoredCard> = Vec::new();
        for sc in store.cards.values() {
            if sc.card.payload.valid_until <= now_secs {
                continue;
            }
            let can = canonicalize_did(&sc.card.payload.did)
                .unwrap_or_else(|_| sc.card.payload.did.clone());
            if my_can_did.as_deref() == Some(can.as_str()) || offenders.contains(&can) {
                continue;
            }
            if let Some(env) = humoco_market::forward_envelope(sc) {
                let card_key = sc.card.payload.recipient_id().to_string();
                wire_meta.insert(env.card_wire, (card_key.clone(), sc.card.payload.v));
                card_by_did.insert(card_key, sc);
                forward_candidates.push(sc);
            }
        }

        for (blob_id, _) in &active_media_blobs {
            active_media_blob_ids.insert(blob_id.clone());
        }

        let should_publish =
            own_wire_info.is_some() || (!forward_candidates.is_empty() && !ctx.readers.is_empty());

        if should_publish {
            let readers_changed =
                sync_state.last_readers_hash.as_deref() != Some(&ctx.readers_hash);
            let heartbeat_due = now_secs.saturating_sub(sync_state.last_upload_at.unwrap_or(0))
                >= SYNC_HEARTBEAT_SECS;
            let delta_prefix = humoco_market::derive_delta_prefix(id_ctx.my_prefix.as_deref());
            let mut any_card_uploaded = false;
            let mut any_card_failed = false;

            for drop_desc in &upload_targets {
                if !state.is_generation_current(session.generation) {
                    return None;
                }
                let target_key = normalize_sync_url(&drop_desc.write_url);
                if target_key.is_empty() {
                    continue;
                }
                if state.is_origin_cooling_down(&drop_desc.write_url, std::time::Instant::now()) {
                    any_card_failed = true;
                    errors.push(format!(
                        "Card upload to {} deferred: server origin cooling down after HTTP 429",
                        drop_desc.write_url
                    ));
                    continue;
                }

                let effective_card_max_bytes = ensure_cached_channel_max_bytes(
                    client,
                    sync_state,
                    id_ctx,
                    drop_desc,
                    id_ctx.my_prefix.as_deref(),
                    "card",
                    SYNC_CARD_MAX_BYTES,
                )
                .await;
                if !state.is_generation_current(session.generation) {
                    return None;
                }

                let tomb_bytes_opt: Option<Vec<u8>> = if !ctx.evicted_reader_keys.is_empty() {
                    own_tombstone_bundle
                        .as_ref()
                        .and_then(|tb| humoco_market::encode_signed_card_bundle(tb).ok())
                } else {
                    None
                };

                let (full_bundle, use_tomb_only) = if ctx.readers.is_empty() {
                    let tb = match own_tombstone_bundle.clone() {
                        Some(tb) => tb,
                        None => match humoco_market::sign_card_bundle(
                            Vec::new(),
                            &id_ctx.my_did,
                            &id_ctx.my_ed_sk,
                        ) {
                            Ok(b) => b,
                            Err(e) => {
                                any_card_failed = true;
                                errors.push(format!("Card bundle signing failed: {}", e));
                                continue;
                            }
                        },
                    };
                    (tb, true)
                } else {
                    let tomb_overhead = if let Some(ref tb) = tomb_bytes_opt {
                        humoco_market::slot_overhead_bytes(ctx.evicted_reader_keys.len())
                            .unwrap_or(8_250)
                            .saturating_add(tb.len())
                            .saturating_add(64)
                    } else {
                        0
                    };
                    let active_budget = effective_card_max_bytes.saturating_sub(tomb_overhead);
                    let built = match humoco_market::build_card_bundle(
                        own_wire_info.as_ref().map(|(_, w, _)| w.as_str()),
                        &forward_candidates,
                        own_coords,
                        now_secs,
                        &trust_level_of,
                        ctx.readers.len(),
                        active_budget,
                        &id_ctx.my_did,
                        &id_ctx.my_ed_sk,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            any_card_failed = true;
                            errors.push(format!("Card bundle signing failed: {}", e));
                            continue;
                        }
                    };
                    (built, false)
                };

                if !full_bundle.envelopes.is_empty() || tomb_bytes_opt.is_some() {
                    let mut desired_versions: BTreeMap<String, u64> = BTreeMap::new();
                    if use_tomb_only {
                        if let Some((ref own_did, _, own_v)) = own_wire_info {
                            desired_versions.insert(own_did.clone(), own_v);
                        }
                    } else {
                        for env in &full_bundle.envelopes {
                            if let Some((did, v)) = wire_meta.get(&env.card_wire) {
                                desired_versions.insert(did.clone(), *v);
                            }
                        }
                    }

                    let prev_base_at = sync_state
                        .card_base_uploaded_at_by_write_url
                        .get(&target_key)
                        .copied();
                    let prev_base_size = sync_state
                        .card_base_size_by_write_url
                        .get(&target_key)
                        .copied();
                    let prev_base_versions = sync_state
                        .card_base_versions_by_write_url
                        .get(&target_key)
                        .cloned();
                    let prev_last_versions = sync_state
                        .card_last_uploaded_versions_by_write_url
                        .get(&target_key)
                        .cloned();
                    let delta_active = sync_state
                        .card_delta_active_by_write_url
                        .get(&target_key)
                        .copied()
                        .unwrap_or(false);
                    let base_fresh = prev_base_at
                        .is_some_and(|ts| now_secs.saturating_sub(ts) < SYNC_BASE_MAX_AGE_SECS);

                    let skip_card_put = force_upload != Some(true)
                        && !readers_changed
                        && base_fresh
                        && !heartbeat_due
                        && prev_last_versions.as_ref() == Some(&desired_versions);

                    if skip_card_put
                        && delta_active
                        && prev_base_versions.as_ref() == Some(&desired_versions)
                        && delete_delta_card_slot(
                            client,
                            id_ctx,
                            drop_desc,
                            &delta_prefix,
                            now_secs,
                        )
                        .await
                    {
                        sync_state
                            .card_delta_active_by_write_url
                            .insert(target_key.clone(), false);
                    }

                    if !skip_card_put {
                        let maybe_delta_blob = if !readers_changed
                            && base_fresh
                            && tomb_bytes_opt.is_none()
                            && !use_tomb_only
                        {
                            build_delta_card_blob(
                                &desired_versions,
                                prev_base_versions.as_ref(),
                                prev_base_size,
                                own_wire_info.as_ref(),
                                &card_by_did,
                                own_coords,
                                now_secs,
                                &trust_level_of,
                                &ctx.readers,
                                effective_card_max_bytes,
                                &id_ctx.my_did,
                                &id_ctx.my_ed_sk,
                            )
                        } else {
                            None
                        };

                        let signer = DropChannelSigner {
                            is_public: false,
                            master_seed: &id_ctx.master_seed,
                            my_ed_sk: &id_ctx.my_ed_sk,
                            drop_desc,
                        };

                        if let Some(delta_blob) = maybe_delta_blob {
                            match signer.sign_request(
                                "PUT",
                                "card",
                                Some(&delta_prefix),
                                now_secs,
                                &sync_client::generate_sync_nonce(),
                                &delta_blob,
                            ) {
                                Ok(signed) => {
                                    match sync_client::put_slot(
                                        client,
                                        &drop_desc.write_url,
                                        &signed,
                                        "card",
                                        &delta_blob,
                                        None,
                                    )
                                    .await
                                    {
                                        Ok(PutSlotOutcome::Saved { .. }) => {
                                            *uploaded_to += 1;
                                            any_card_uploaded = true;
                                            sync_state
                                                .card_delta_active_by_write_url
                                                .insert(target_key.clone(), true);
                                            sync_state
                                                .card_last_uploaded_versions_by_write_url
                                                .insert(target_key.clone(), desired_versions);
                                        }
                                        Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                            any_card_failed = true;
                                            errors.push(format!(
                                                "Upload delta to {}: Precondition failed (ETag)",
                                                drop_desc.write_url
                                            ));
                                        }
                                        Err(e) => {
                                            if e.http_status() == Some(413) {
                                                sync_state.channel_max_bytes_by_write_url.remove(
                                                    &super::channel_reader::channel_max_bytes_key(
                                                        &drop_desc.write_url,
                                                        "card",
                                                    ),
                                                );
                                            }
                                            any_card_failed = true;
                                            errors.record_sync_error(
                                                state,
                                                &drop_desc.write_url,
                                                &e,
                                                format!(
                                                    "Upload delta to {}: {}",
                                                    drop_desc.write_url, e
                                                ),
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    any_card_failed = true;
                                    errors.push(format!(
                                        "Signing delta error for {}: {}",
                                        drop_desc.write_url, e
                                    ));
                                }
                            }
                        } else {
                            match seal_base_card_blob(
                                use_tomb_only,
                                tomb_bytes_opt.as_ref(),
                                &full_bundle,
                                &ctx.readers,
                                &ctx.evicted_reader_keys,
                                effective_card_max_bytes,
                            ) {
                                Ok(base_blob) => {
                                    let base_len = base_blob.len();
                                    match signer.sign_request(
                                        "PUT",
                                        "card",
                                        id_ctx.my_prefix.as_deref(),
                                        now_secs,
                                        &sync_client::generate_sync_nonce(),
                                        &base_blob,
                                    ) {
                                        Ok(signed) => {
                                            match sync_client::put_slot(
                                                client,
                                                &drop_desc.write_url,
                                                &signed,
                                                "card",
                                                &base_blob,
                                                None,
                                            )
                                            .await
                                            {
                                                Ok(PutSlotOutcome::Saved { .. }) => {
                                                    *uploaded_to += 1;
                                                    any_card_uploaded = true;
                                                    sync_state
                                                        .card_base_uploaded_at_by_write_url
                                                        .insert(target_key.clone(), now_secs);
                                                    sync_state
                                                        .card_base_size_by_write_url
                                                        .insert(target_key.clone(), base_len);
                                                    sync_state
                                                        .card_base_versions_by_write_url
                                                        .insert(
                                                            target_key.clone(),
                                                            desired_versions.clone(),
                                                        );
                                                    sync_state
                                                        .card_last_uploaded_versions_by_write_url
                                                        .insert(
                                                            target_key.clone(),
                                                            desired_versions,
                                                        );
                                                    let delta_cleared = if delta_active {
                                                        delete_delta_card_slot(
                                                            client,
                                                            id_ctx,
                                                            drop_desc,
                                                            &delta_prefix,
                                                            now_secs,
                                                        )
                                                        .await
                                                    } else {
                                                        true
                                                    };
                                                    sync_state
                                                        .card_delta_active_by_write_url
                                                        .insert(target_key.clone(), !delta_cleared);
                                                }
                                                Ok(PutSlotOutcome::PreconditionFailed {
                                                    ..
                                                }) => {
                                                    any_card_failed = true;
                                                    errors.push(format!(
                                                        "Upload to {}: Precondition failed (ETag)",
                                                        drop_desc.write_url
                                                    ));
                                                }
                                                Err(e) => {
                                                    if e.http_status() == Some(413) {
                                                        sync_state
                                                            .channel_max_bytes_by_write_url
                                                            .remove(
                                                                &super::channel_reader::channel_max_bytes_key(
                                                                    &drop_desc.write_url,
                                                                    "card",
                                                                ),
                                                            );
                                                    }
                                                    any_card_failed = true;
                                                    errors.record_sync_error(
                                                        state,
                                                        &drop_desc.write_url,
                                                        &e,
                                                        format!(
                                                            "Upload to {}: {}",
                                                            drop_desc.write_url, e
                                                        ),
                                                    );
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            any_card_failed = true;
                                            errors.push(format!(
                                                "Signing error for {}: {}",
                                                drop_desc.write_url, e
                                            ));
                                        }
                                    }
                                }
                                Err(e) => {
                                    any_card_failed = true;
                                    errors.push(format!("Failed to seal slot blob: {}", e));
                                }
                            }
                        }
                    }
                }

                upload_media_blobs_to_target(
                    client,
                    state,
                    session,
                    id_ctx,
                    drop_desc,
                    &active_media_blobs,
                    force_upload == Some(true),
                    now_secs,
                    sync_state,
                    errors,
                )
                .await;
            }

            if any_card_uploaded {
                sync_state.last_upload_at = Some(now_secs);
                if !any_card_failed {
                    sync_state.last_readers_hash = Some(ctx.readers_hash);
                }
            }
        } else if has_stale_slots
            && !delete_stale_card_slots(
                client,
                state,
                session,
                id_ctx,
                &upload_targets,
                now_secs,
                sync_state,
                errors,
            )
            .await
        {
            return None;
        }
    } else if has_stale_slots
        && !delete_stale_card_slots(
            client,
            state,
            session,
            id_ctx,
            &upload_targets,
            now_secs,
            sync_state,
            errors,
        )
        .await
    {
        return None;
    }

    Some(active_media_blob_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE};

    #[tokio::test]
    async fn test_probe_contact_syncs_corrupt_contacts_fails_without_overwrite() {
        let base_dir = temp_test_dir("card_sync_corrupt_contacts");
        let password = "TestPassword123!";
        let service = setup_test_app(&base_dir, "Alice", ALICE_PHRASE, password);
        let state = create_test_state(service);

        {
            let mut s = state.service.lock().unwrap();
            crate::state::save_encrypted_bytes(
                &mut s,
                CONTACTS_DATA_NAME,
                b"{corrupt-contacts",
                Some(password),
            )
            .unwrap();
        }
        state.set_cached_contacts(None);

        let res = probe_contact_syncs_impl(false, Some(password.to_string()), &state).await;
        assert!(
            res.is_err(),
            "Corrupt address book must abort probe_contact_syncs_impl with Err"
        );

        let stored_raw = {
            let mut s = state.service.lock().unwrap();
            crate::state::load_encrypted_bytes_opt(&mut s, CONTACTS_DATA_NAME, Some(password))
                .unwrap()
                .unwrap()
        };
        assert_eq!(stored_raw, b"{corrupt-contacts");

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn test_h1_probe_contact_syncs_preserves_concurrent_add_and_delete() {
        use crate::models::{FrontendContact, FrontendUserProfile};
        use crate::test_support::{MockSyncServer, BOB_PHRASE, CHARLIE_PHRASE, READER1_PHRASE};
        use std::sync::Arc;

        let base_dir = temp_test_dir("h1_probe_concurrency");
        let password = "TestPassword123!";
        let alice_state = Arc::new(create_test_state(setup_test_app(
            &base_dir.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        )));

        let bob_did = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password)
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let del_did = setup_test_app(&base_dir.join("del"), "Del", CHARLIE_PHRASE, password)
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let added_did = setup_test_app(&base_dir.join("add"), "Add", READER1_PHRASE, password)
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let admin_sk = ed25519_dalek::SigningKey::from_bytes(&[0x42u8; 32]);
        let admin_pub = hex::encode(admin_sk.verifying_key().as_bytes());
        let server = MockSyncServer::start("d-h1".to_string(), admin_pub.clone()).await;

        let mk_ep = |write_url: &str| FrontendSyncDescriptor {
            write_url: write_url.to_string(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: admin_pub.clone(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: bob_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("Bob".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".to_string(),
                        notes: None,
                        trust_level: Some(2),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![mk_ep(&format!("{}/sync.php", server.base_url))],
                    },
                    Some(password),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: del_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("ToDelete".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".to_string(),
                        notes: None,
                        trust_level: Some(1),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![mk_ep(&format!("{}/sync.php", server.base_url))],
                    },
                    Some(password),
                )
                .unwrap();
        }

        let (hit_rx, release_tx) = server.arm_pause_gate("info");
        let alice_bg = Arc::clone(&alice_state);
        let probe_handle = tokio::spawn(async move {
            probe_contact_syncs_impl(true, Some(password.to_string()), &alice_bg).await
        });

        hit_rx.await.unwrap();

        // Concurrently during probe .await: add AddedContact AND delete ToDelete
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: added_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("AddedDuringProbe".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".to_string(),
                        notes: None,
                        trust_level: Some(2),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![],
                    },
                    Some(password),
                )
                .unwrap();
            alice_state
                .delete_contact(&mut s, &del_did, Some(password))
                .unwrap();
        }

        let _ = release_tx.send(true);
        let _ = probe_handle.await.unwrap().unwrap();

        let mut s = alice_state.service.lock().unwrap();
        let contacts = alice_state.load_contacts(&mut s, Some(password)).unwrap();
        assert!(
            contacts.iter().any(|c| c.did == added_did),
            "Concurrently added contact must NOT be overwritten by probe snapshot"
        );
        assert!(
            !contacts.iter().any(|c| c.did == del_did),
            "Concurrently deleted contact must NOT be resurrected by probe snapshot"
        );
        let bob_c = contacts.iter().find(|c| c.did == bob_did).unwrap();
        assert_ne!(bob_c.syncs[0].status, "unverified");
        assert!(bob_c.syncs[0].last_probed_at.is_some());

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn s10_x3_set_selected_upload_syncs_atomic_and_drops_market_lock_before_locator_enqueue()
    {
        use crate::models::{FrontendSaveMyMarketProfileRequest, FrontendUserProfile};
        use crate::services::market_service::save_my_market_profile_impl;
        use std::sync::Arc;
        use std::time::Duration;

        let base_dir = crate::test_support::temp_test_dir("s10_x3_selected_syncs_lock_order");
        let password = "StrongPw-S10-X3!";
        let alice_svc = crate::test_support::setup_test_app(
            &base_dir.join("alice"),
            "Alice",
            crate::test_support::ALICE_PHRASE,
            password,
        );
        let state = Arc::new(crate::test_support::create_test_state(alice_svc));

        save_my_market_profile_impl(
            FrontendSaveMyMarketProfileRequest {
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                visibility: Default::default(),
                skills: vec![],
                max_hops: 2,
                validity_years: 2,
                allow_inquiries: true,
                published: Some(true),
            },
            Some(password.to_string()),
            &state,
        )
        .unwrap();

        let desc = FrontendSyncDescriptor {
            write_url: "https://sync-a.example.org/sync.php".to_string(),
            read_url: "https://sync-a.example.org/d".to_string(),
            slot_id: None,
            drop_pub: crate::test_support::test_ed25519_pub_hex(0x11),
            status: "active".to_string(),
            ..Default::default()
        };

        // Hold recovery_op_lock while set_selected_upload_syncs_impl runs.
        // Because market_op_lock MUST be dropped before enqueue_guardian_locator_updates_if_configured,
        // this task holding recovery_op_lock can acquire market_op_lock without deadlocking.
        let recovery_guard = state.recovery_op_lock.lock().await;
        let state_bg = Arc::clone(&state);
        let desc_bg = desc.clone();
        let handle = tokio::spawn(async move {
            set_selected_upload_syncs_impl(vec![desc_bg], Some(password.to_string()), &state_bg)
                .await
        });

        // Wait until the atomic SecurityPolicy + OwnMarketProfile commit has completed.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if state
                    .cache_security_policy()
                    .is_some_and(|p| p.market_selected_syncs.len() == 1)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect(
            "set_selected_upload_syncs_impl must persist policy while recovery_op_lock is held",
        );

        // market_op_lock MUST already be released before enqueue_guardian_locator_updates_if_configured waits on recovery_op_lock.
        let market_guard =
            tokio::time::timeout(Duration::from_secs(5), state.market_op_lock.lock())
                .await
                .expect("S10-X3: market_op_lock must be dropped before guardian locator enqueue");
        drop(market_guard);
        drop(recovery_guard);

        let saved = handle.await.unwrap().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(
            state
                .cache_security_policy()
                .unwrap()
                .market_selected_syncs
                .len(),
            1
        );
        let own_prof = state.cache_own_market_profile().unwrap();
        let drops = own_prof.last_card.unwrap().payload.drops;
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].write_url, "https://sync-a.example.org/sync.php");

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn test_revoked_own_sync_allows_contact_probe_and_friend_auto_activation() {
        use crate::models::{FrontendContact, FrontendUserProfile};
        use crate::test_support::{MockSyncServer, ALICE_PHRASE, BOB_PHRASE};

        let base_dir = temp_test_dir("revoked_own_sync_friend_activation");
        let password = "TestPassword123!";
        let bob_state = create_test_state(setup_test_app(
            &base_dir.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));
        let alice_did = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password)
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let server_admin_pub = crate::test_support::test_ed25519_pub_hex(0x21);
        let server = MockSyncServer::start("d-friend".to_string(), server_admin_pub.clone()).await;

        let bob_signing_key = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet_and_identity(|_w, id| id.signing_key.clone())
                .unwrap()
        };
        let stealth_pk =
            humoco_market::derive_member_stealth_pubkey(&bob_signing_key, &server_admin_pub)
                .unwrap();
        server.members.lock().unwrap().push(stealth_pk);

        let own_drop_pub = crate::test_support::test_ed25519_pub_hex(0x31);
        let revoked_admin = crate::state::StoredSyncAdmin {
            descriptor: humoco_market::DropDescriptor {
                write_url: "https://revoked-own.example.org/sync.php".to_string(),
                read_url: "https://revoked-own.example.org/d-own".to_string(),
                slot_id: None,
                drop_pub: own_drop_pub.clone(),
            },
            sync_index: 0,
            data_dir: "d-own".to_string(),
            max_members: 50,
            max_total_storage_mb: 100,
            max_user_storage_mb: 10,
            policies: Default::default(),
            allow: vec![],
            last_members_hash: Default::default(),
            revocation: Some(SyncRevocationNotice {
                drop_pub: own_drop_pub,
                issued_at: 1_760_000_000,
                effective_at: 1_760_000_000,
                reason: Some("Decommissioned".to_string()),
                replacement: None,
                signature: "00".repeat(64),
            }),
        };

        let now = sync_client::current_unix_timestamp();
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut batch = crate::state::EncryptedBatchCommit::new();
            batch.sync_admin(revoked_admin);
            batch.commit(&bob_state, &mut s, Some(password)).unwrap();

            bob_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: alice_did,
                        profile: FrontendUserProfile {
                            first_name: Some("Alice".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".to_string(),
                        notes: None,
                        trust_level: Some(2),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![FrontendSyncDescriptor {
                            write_url: format!("{}/sync.php", server.base_url),
                            read_url: format!("{}/d-friend", server.base_url),
                            slot_id: None,
                            drop_pub: server_admin_pub.clone(),
                            last_seen_at: None,
                            last_error_at: None,
                            fail_count: 0,
                            status: "pending_auth".to_string(),
                            last_probed_at: Some(now.saturating_sub(120)),
                            ..Default::default()
                        }],
                    },
                    Some(password),
                )
                .unwrap();
        }

        let auth = bob_state
            .authorize_session(crate::state::RequiredAccess::Unlocked, Some(password))
            .unwrap();
        let probed = probe_contact_syncs_with_session(&auth.token, false, auth.pw(), &bob_state)
            .await
            .unwrap();

        assert_eq!(
            probed.len(),
            1,
            "Revoked own sync must not suppress contact sync re-probing (force=false)"
        );
        assert_eq!(probed[0].status, "active");

        let policy = {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .get_or_load_security_policy(&mut s, Some(password))
                .unwrap()
        };
        assert_eq!(
            policy.market_selected_syncs.len(),
            1,
            "Revoked own sync must not block auto-activating a healthy friend sync"
        );
        assert_eq!(policy.market_selected_syncs[0].drop_pub, server_admin_pub);
        assert_eq!(policy.market_selected_syncs[0].status, "active");

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
