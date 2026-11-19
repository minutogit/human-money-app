use std::collections::{BTreeMap, HashMap, HashSet};

use human_money_core::app_service::AppService;
use human_money_core::models::conflict::TrustStatus;
use sha2::Digest;

use crate::models::{
    FrontendAddressBook, FrontendDiscoveredSync, FrontendError, FrontendSyncDescriptor,
    StoredOwnMarketProfile,
};
use crate::services::common::{normalize_sync_url, DropChannelSigner, SyncIdentityContext};
use crate::services::market_service::sync_own_card_drops_on_settings_change;
use crate::services::recovery_service::enqueue_guardian_locator_updates_if_configured;
use crate::settings::{
    SecurityPolicy, UiPreferences, SYNC_BASE_MAX_AGE_SECS, SYNC_CARD_MAX_BYTES,
    SYNC_HEARTBEAT_SECS,
};
use crate::state::{
    apply_revocation_notice, apply_revocation_notice_to_address_book, canonicalize_did,
    clear_revocation, AppState, StoredSyncState, CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY,
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

fn descriptors_overlap(a: &FrontendSyncDescriptor, b: &FrontendSyncDescriptor) -> bool {
    let ar = normalize_sync_url(&a.read_url);
    let aw = normalize_sync_url(&a.write_url);
    let br = normalize_sync_url(&b.read_url);
    let bw = normalize_sync_url(&b.write_url);
    (!ar.is_empty() && (br == ar || bw == ar)) || (!aw.is_empty() && (br == aw || bw == aw))
}

pub async fn discover_available_syncs_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendDiscoveredSync>, FrontendError> {
    let mut descriptors: Vec<(FrontendSyncDescriptor, String)> = Vec::new();

    {
        let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;

        // 1. Own drop admin
        if let Some(admin) = load_stored_sync_admin_opt(&mut service, password.as_deref())? {
            if !admin.descriptor.read_url.is_empty() && !admin.descriptor.write_url.is_empty() {
                let mut desc: FrontendSyncDescriptor = admin.descriptor.into();
                desc.status = "active".to_string();
                if let Some(ref rev) = admin.revocation {
                    apply_revocation_notice(&mut desc, rev, sync_client::current_unix_timestamp());
                }
                descriptors.push((desc, "Eigener Sync-Dienst".to_string()));
            }
        }

        // 2. Contacts
        if let Ok(contacts) = state.load_contacts(&mut service, password.as_deref()) {
            for c in contacts {
                let first = c.profile.first_name.as_deref().unwrap_or("").trim();
                let last = c.profile.last_name.as_deref().unwrap_or("").trim();
                let name = match (first.is_empty(), last.is_empty()) {
                    (false, false) => format!("{} {}", first, last),
                    (false, true) => first.to_string(),
                    (true, false) => last.to_string(),
                    (true, true) => c.profile.organization.unwrap_or_else(|| c.did.clone()),
                };
                for d in c.syncs {
                    descriptors.push((d, name.clone()));
                }
            }
        }

        // 3. Card store
        let store: humoco_market::CardStore = crate::state::load_encrypted_json_or_default(
            &mut service,
            MARKET_CARD_STORE_KEY,
            password.as_deref(),
        )
        .map_err(FrontendError::from)?;
        for sc in store.cards.values() {
            let profile = humoco_market::extract_public_profile(&sc.card);
            let author = profile.first_name.as_deref().unwrap_or("Visitenkarte");
            for d in &sc.card.payload.drops {
                descriptors.push((d.clone().into(), author.to_string()));
            }
        }
    }

    let security_policy = state.get_or_load_security_policy_locked(password.as_deref())?;

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
            let elapsed = now.saturating_sub(last_probed);
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

    if let Some((c_read, c_drop, c_status, c_ok, c_rev)) = probe_cache.get(&sync_endpoint_key) {
        if *c_ok {
            ep.write_url = canonical_write;
            ep.read_url = c_read.clone();
            ep.drop_pub = c_drop.clone();
            ep.status = c_status.clone();
            ep.fail_count = 0;
            ep.last_seen_at = Some(now);
            ep.last_probed_at = Some(now);
        } else {
            ep.status = c_status.clone();
            ep.fail_count = ep.fail_count.saturating_add(1);
            ep.last_probed_at = Some(now);
        }
        if let Some(ref rev) = c_rev {
            apply_revocation_notice(ep, rev, now);
            verified_revocations.push((c_read.clone(), rev.clone()));
        } else {
            clear_revocation(ep, c_status);
        }
        return true;
    }

    match sync_client::fetch_sync_info(client, &canonical_write).await {
        Err(e) => {
            log::warn!("Sync probe ?info failed for {}: {}", canonical_write, e);
            ep.status = "unreachable".to_string();
            ep.fail_count = ep.fail_count.saturating_add(1);
            ep.last_probed_at = Some(now);
            probe_cache.insert(
                sync_endpoint_key,
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
            let verified_rev = info.revocation.as_ref().and_then(|n| {
                sync_client::verify_revocation_notice(n, &info.drop_pub)
                    .ok()
                    .map(|_| n.clone())
            });
            let trimmed_read = normalize_sync_url(&info.read_url);
            let read_url_valid =
                !trimmed_read.is_empty() && sync_client::validate_sync_url(&trimmed_read).is_ok();
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
            let info_drop_pub = info.drop_pub.clone();

            ep.write_url = canonical_write.clone();
            ep.read_url = resolved_read_url.clone();
            ep.drop_pub = info.drop_pub;
            ep.last_seen_at = Some(now);

            if verified_rev.is_none() {
                clear_revocation(ep, "unverified");
            }

            let stealth_pk_res = if let Some(ref slot_id) = ep.slot_id {
                if !slot_id.is_empty() {
                    humoco_market::parse_slot_id(slot_id)
                } else {
                    humoco_market::derive_member_stealth_pubkey(&id_ctx.my_ed_sk, &ep.drop_pub)
                        .map(|pk| (pk, None))
                }
            } else {
                humoco_market::derive_member_stealth_pubkey(&id_ctx.my_ed_sk, &ep.drop_pub)
                    .map(|pk| (pk, None))
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
                    ep.status = "pending_auth".to_string();
                    ep.fail_count = 0;
                    ep.last_probed_at = Some(now);
                }
                Ok((stealth_pk, slot_pfx)) => {
                    let cap_prefix = slot_pfx.as_deref().or(id_ctx.my_prefix.as_deref());
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
                        ep.status = "active".to_string();
                    } else {
                        // Server answered ?info (reachable), so never classify as offline/unreachable.
                        probe_status = "pending_auth".to_string();
                        ep.status = "pending_auth".to_string();
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
                sync_endpoint_key,
                (
                    resolved_read_url,
                    info_drop_pub,
                    probe_status,
                    true,
                    verified_rev,
                ),
            );
            true
        }
    }
}

pub async fn probe_contact_syncs_impl(
    force: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    let (owner_did, id_ctx, initial_policy, has_own_sync, mut address_book) = {
        let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
        let owner_did = service
            .with_wallet(|w| w.get_user_id().to_string())
            .map_err(FrontendError::from)?;
        let id_ctx = SyncIdentityContext::from_service(&service)?;
        let s = state
            .get_or_load_security_policy(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        let own = load_stored_sync_admin_opt(&mut service, password.as_deref())?
            .is_some_and(|a| !a.descriptor.write_url.is_empty() && !a.descriptor.read_url.is_empty());
        let ab: FrontendAddressBook = crate::state::load_encrypted_json_or_default(
            &mut service,
            CONTACTS_DATA_NAME,
            password.as_deref(),
        )
        .map_err(FrontendError::from)?;
        (owner_did, id_ctx, s, own, ab)
    };

    let has_active_friend_sync = initial_policy.market_selected_syncs.iter().any(|d| {
        d.status == "active"
            && d.fail_count == 0
            && !d.read_url.is_empty()
            && !d.drop_pub.is_empty()
    });
    let has_healthy_public_sync = initial_policy
        .public_syncs
        .iter()
        .any(|pd| pd.enabled && !pd.authorized_channels.is_empty() && !pd.read_url.is_empty());

    let mut has_healthy_sync = has_own_sync || has_active_friend_sync || has_healthy_public_sync;
    let initial_has_healthy = has_healthy_sync;

    let now = chrono::Utc::now().timestamp() as u64;
    let client = sync_client::create_sync_probe_client();
    let mut contacts_modified = false;
    let mut probed_endpoints: Vec<FrontendSyncDescriptor> = Vec::new();
    let mut probe_cache: HashMap<String, ProbeCacheEntry> = HashMap::new();
    let mut verified_revocations: Vec<(String, SyncRevocationNotice)> = Vec::new();
    let mut channel_max_bytes_updates: Vec<(String, String, usize)> = Vec::new();

    for contact in address_book.contacts.values_mut() {
        for ep in &mut contact.syncs {
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
                contacts_modified = true;
                probed_endpoints.push(ep.clone());
            }
        }
    }

    for (rev_read_url, notice) in verified_revocations {
        if apply_revocation_notice_to_address_book(&mut address_book, &rev_read_url, &notice, now) {
            contacts_modified = true;
        }
    }

    if contacts_modified {
        let mut service = state.lock_service()?;
        let current_did = service.with_wallet(|w| w.get_user_id().to_string()).ok();
        if current_did.as_deref() == Some(owner_did.as_str()) {
            crate::state::save_encrypted_json(
                &mut service,
                CONTACTS_DATA_NAME,
                &address_book,
                password.as_deref(),
            )
            .map_err(FrontendError::from)?;
            *state.contacts.lock().unwrap() = Some(address_book.clone());
        }
    }

    if !probed_endpoints.is_empty() {
        let mut auto_activated = false;
        let updated_policy = state.mutate_security_policy_for_owner(
            &owner_did,
            password.as_deref(),
            |policy| {
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
                            && !pd.authorized_channels.is_empty()
                            && !pd.read_url.is_empty()
                    });
                for ep in &probed_endpoints {
                    for sel in &mut policy.market_selected_syncs {
                        if descriptors_overlap(sel, ep) {
                            sel.write_url = ep.write_url.clone();
                            sel.read_url = ep.read_url.clone();
                            sel.drop_pub = ep.drop_pub.clone();
                            sel.status = ep.status.clone();
                            sel.fail_count = ep.fail_count;
                            sel.last_probed_at = ep.last_probed_at;
                            sel.sunset_at = ep.sunset_at;
                            sel.revocation_reason = ep.revocation_reason.clone();
                            sel.replacement = ep.replacement.clone();
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
                            .any(|s| descriptors_overlap(s, ep));
                        if !already_selected && policy.market_selected_syncs.len() < 3 {
                            policy.market_selected_syncs.push(FrontendSyncDescriptor {
                                write_url: ep.write_url.clone(),
                                read_url: ep.read_url.clone(),
                                slot_id: ep.slot_id.clone(),
                                drop_pub: ep.drop_pub.clone(),
                                last_seen_at: ep.last_seen_at,
                                last_error_at: None,
                                fail_count: 0,
                                status: "active".to_string(),
                                last_probed_at: Some(now),
                                ..Default::default()
                            });
                            current_healthy = true;
                            has_healthy_sync = true;
                            modified = true;
                            auto_activated = true;
                        }
                    }
                }
                modified
            },
        )?;
        let _ = has_healthy_sync;
        if auto_activated {
            if let Some(policy) = updated_policy {
                let mut service = state.lock_service()?;
                sync_own_card_drops_on_settings_change(
                    state,
                    &mut service,
                    &policy.market_selected_syncs,
                    password.as_deref(),
                );
            }
        }
    }

    save_channel_max_bytes_updates(state, password.as_deref(), &channel_max_bytes_updates);

    let mut all_syncs = Vec::new();
    for c in address_book.contacts.values() {
        all_syncs.extend(c.syncs.clone());
    }
    Ok(all_syncs)
}

pub async fn set_selected_upload_syncs_impl(
    syncs: Vec<FrontendSyncDescriptor>,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    state.authorize(crate::state::RequiredAccess::Full, password.as_deref())?;
    if syncs.len() > 3 {
        return Err(FrontendError::from(
            "Maximum 3 upload drops allowed".to_string(),
        ));
    }

    for d in &syncs {
        sync_client::validate_sync_url(&d.write_url).map_err(FrontendError::from)?;
        sync_client::validate_sync_url(&d.read_url).map_err(FrontendError::from)?;
        humoco_market::parse_ed25519_pubkey_hex64(&d.drop_pub)
            .map_err(|e| FrontendError::from(format!("Invalid drop_pub: {}", e)))?;
        if let Some(ref sid) = d.slot_id {
            humoco_market::parse_slot_id(sid)
                .map_err(|e| FrontendError::from(format!("Invalid slot_id: {}", e)))?;
        }
    }

    {
        let mut service = state.lock_service()?;
        state
            .mutate_security_policy(&mut service, password.as_deref(), |policy| {
                policy.market_selected_syncs = syncs.clone();
            })
            .map_err(FrontendError::from)?;

        sync_own_card_drops_on_settings_change(state, &mut service, &syncs, password.as_deref());
    }

    let _ = enqueue_guardian_locator_updates_if_configured(state, password.clone()).await;

    // Trigger upload to selected drops.
    let _ = super::sync_market_channels_impl(None, Some(true), password, state).await;

    Ok(syncs)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn import_decrypted_card_bundle(
    bundle: humoco_market::CardBundle,
    read_url: &str,
    now_secs: u64,
    max_stored_cards: usize,
    state: &AppState,
    store: &mut humoco_market::CardStore,
    sync_state: &mut StoredSyncState,
    password: Option<&str>,
) -> (usize, usize) {
    let mut count = 0;
    let mut new_ads = 0;
    let mut before_ads_by_did: HashMap<String, HashSet<String>> = HashMap::new();
    for sc in store.cards.values() {
        let ids: HashSet<String> = humoco_market::sd_card::active_ads(&sc.card, now_secs)
            .into_iter()
            .map(|a| a.id)
            .collect();
        before_ads_by_did.insert(sc.card.payload.did.clone(), ids);
    }

    for env in bundle.envelopes {
        let Ok(mut service) = state.lock_service() else {
            break;
        };
        let outcome = service.with_wallet(|wallet| {
            let is_offender =
                |did: &str| matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_));
            let own_coords = wallet
                .profile
                .coordinates
                .as_deref()
                .and_then(humoco_market::parse_coordinates_pair);
            humoco_market::accept_envelope(
                store,
                &env,
                now_secs,
                max_stored_cards,
                own_coords,
                &is_offender,
            )
        });

        if let Ok(humoco_market::AcceptOutcome::Accepted { did, .. }) = outcome {
            count += 1;
            let can_did = canonicalize_did(&did).unwrap_or_else(|_| did.clone());
            sync_state
                .sync_sources_by_did
                .insert(can_did, read_url.to_string());

            if let Some(stored) = store.cards.values().find(|sc| sc.card.payload.did == did) {
                let prev_ids = before_ads_by_did.entry(did.clone()).or_default();
                for ad in humoco_market::sd_card::active_ads(&stored.card, now_secs) {
                    if prev_ids.insert(ad.id) {
                        new_ads += 1;
                    }
                }
                let card_syncs: Vec<FrontendSyncDescriptor> = stored
                    .card
                    .payload
                    .drops
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect();
                let card_profile = humoco_market::extract_public_profile(&stored.card);
                state.update_contact_from_card(
                    &mut service,
                    &did,
                    &card_syncs,
                    Some(&card_profile),
                    password,
                );
            }
        }
    }
    (count, new_ads)
}

fn load_card_upload_context(
    state: &AppState,
    ui_prefs: &UiPreferences,
    id_ctx: &SyncIdentityContext,
    password: Option<&str>,
) -> Result<CardUploadContext, FrontendError> {
    let mut service = state.lock_service()?;

    let my_can_did = canonicalize_did(&id_ctx.my_did).ok();
    let own_profile_opt: Option<StoredOwnMarketProfile> =
        Some(crate::services::market_service::load_own_market_profile_state(
            state,
            &mut service,
            password,
        )?);
    // Include unexpired <jws>~ tombstone cards even when `published == false`.
    let own_card_opt = own_profile_opt.as_ref().and_then(|p| p.last_card.clone());

    let reg = state.get_or_load_wot_register(&mut service, password).ok();
    let address_book: FrontendAddressBook = crate::state::load_encrypted_json_or_default(
        &mut service,
        CONTACTS_DATA_NAME,
        password,
    )
    .map_err(FrontendError::from)?;

    let mut readers = Vec::new();
    let mut evicted_reader_keys = Vec::new();
    if let Some(r) = reg {
        for (did, entry) in &r.own_contacts {
            if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(did) {
                if entry.level >= ui_prefs.share_min_level {
                    readers.push(pk);
                } else if !is_known_offender(&service, did) {
                    evicted_reader_keys.push(pk);
                }
            }
        }
    }
    let reader_set: HashSet<[u8; 32]> = readers.iter().map(|k| *k.as_bytes()).collect();
    for contact in address_book.contacts.values() {
        let can = canonicalize_did(&contact.did).unwrap_or_else(|_| contact.did.clone());
        if my_can_did.as_deref() == Some(can.as_str()) {
            continue;
        }
        if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(&can) {
            if !reader_set.contains(pk.as_bytes()) && !is_known_offender(&service, &can) {
                evicted_reader_keys.push(pk);
            }
        }
    }
    readers.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    readers.dedup();
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
    );
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
            let fb = serde_json::to_vec(full_bundle)
                .map_err(|e| humoco_market::MarketError::Serialization(e.to_string()))?;
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

/// Uploads encrypted business card bundles (base, delta, and revocation tombstones) plus active ad media blobs.
/// Returns the set of currently active ad media `blob_id`s for downstream orphan GC.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync_card_upload_channel(
    client: &reqwest::Client,
    state: &AppState,
    ui_prefs: &UiPreferences,
    security_policy: &SecurityPolicy,
    id_ctx: &SyncIdentityContext,
    force_upload: Option<bool>,
    now_secs: u64,
    password: Option<&str>,
    sync_state: &mut StoredSyncState,
    uploaded_to: &mut usize,
    errors: &mut Vec<String>,
) -> Option<HashSet<String>> {
    if !ui_prefs.market_sync_enabled {
        return None;
    }
    let upload_targets = collect_upload_targets_for_channel(security_policy, "card");
    if upload_targets.is_empty() {
        return None;
    }
    let ctx = match load_card_upload_context(state, ui_prefs, id_ctx, password) {
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

    if !ctx.readers.is_empty() || (active_own_card.is_some() && !ctx.evicted_reader_keys.is_empty())
    {
        let own_wire_info: Option<(String, String, u64)> = active_own_card.and_then(|own_card| {
            let can_did = canonicalize_did(&own_card.payload.did)
                .unwrap_or_else(|_| own_card.payload.did.clone());
            humoco_market::render_wire(own_card, 1, humoco_market::WireFilter::Full)
                .map(|wire| (can_did, wire, own_card.payload.v))
        });
        let own_tombstone_bundle: Option<humoco_market::CardBundle> =
            active_own_card.map(|own_card| humoco_market::CardBundle {
                envelopes: vec![humoco_market::CardEnvelope {
                    hops: 1,
                    card_wire: format!("{}~", own_card.jws),
                }],
            });

        let load_res: Result<_, FrontendError> = (|| {
            let mut service = state.lock_service()?;
            let store: humoco_market::CardStore = crate::state::load_encrypted_json_or_default(
                &mut service,
                MARKET_CARD_STORE_KEY,
                password,
            )
            .map_err(FrontendError::from)?;
            let reg = state.get_or_load_wot_register(&mut service, password).ok();
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
                if is_known_offender(&service, &sc.card.payload.did) {
                    offenders.insert(can);
                }
            }

            let media_blobs = seal_active_ad_media_blobs(
                &mut service,
                active_own_card,
                ctx.own_profile_opt.as_ref(),
                now_secs,
                password,
            )?;
            Ok((store, own_coords, trust_by_did, offenders, media_blobs))
        })();

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

        let own_can_did_opt = own_wire_info.as_ref().map(|(d, _, _)| d.clone());
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
            if own_can_did_opt.as_ref() == Some(&can) || offenders.contains(&can) {
                continue;
            }
            if let Some(env) = humoco_market::forward_envelope(sc) {
                wire_meta.insert(env.card_wire, (can.clone(), sc.card.payload.v));
                card_by_did.insert(can, sc);
                forward_candidates.push(sc);
            }
        }

        for (blob_id, _) in &active_media_blobs {
            active_media_blob_ids.insert(blob_id.clone());
        }

        if own_wire_info.is_some() || (!forward_candidates.is_empty() && !ctx.readers.is_empty()) {
            let readers_changed =
                sync_state.last_readers_hash.as_deref() != Some(&ctx.readers_hash);
            let heartbeat_due = now_secs.saturating_sub(sync_state.last_upload_at.unwrap_or(0))
                >= SYNC_HEARTBEAT_SECS;
            let delta_prefix = humoco_market::derive_delta_prefix(id_ctx.my_prefix.as_deref());
            let mut any_card_uploaded = false;
            let mut any_card_failed = false;

            for drop_desc in &upload_targets {
                let target_key = normalize_sync_url(&drop_desc.write_url);
                if target_key.is_empty() {
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

                let tomb_bytes_opt: Option<Vec<u8>> = if !ctx.evicted_reader_keys.is_empty() {
                    own_tombstone_bundle
                        .as_ref()
                        .and_then(|tb| serde_json::to_vec(tb).ok())
                } else {
                    None
                };

                let (full_bundle, use_tomb_only) = if ctx.readers.is_empty() {
                    (
                        own_tombstone_bundle
                            .clone()
                            .unwrap_or(humoco_market::CardBundle {
                                envelopes: Vec::new(),
                            }),
                        true,
                    )
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
                    (
                        humoco_market::build_card_bundle(
                            own_wire_info.as_ref().map(|(_, w, _)| w.as_str()),
                            &forward_candidates,
                            own_coords,
                            now_secs,
                            &trust_level_of,
                            ctx.readers.len(),
                            active_budget,
                        ),
                        false,
                    )
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
                                            any_card_failed = true;
                                            errors.push(format!(
                                                "Upload delta to {}: {}",
                                                drop_desc.write_url, e
                                            ));
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
                                                    any_card_failed = true;
                                                    errors.push(format!(
                                                        "Upload to {}: {}",
                                                        drop_desc.write_url, e
                                                    ));
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
                    id_ctx,
                    drop_desc,
                    &active_media_blobs,
                    force_upload == Some(true),
                    now_secs,
                    sync_state,
                )
                .await;
            }

            if any_card_uploaded {
                sync_state.last_upload_at = Some(now_secs);
                if !any_card_failed {
                    sync_state.last_readers_hash = Some(ctx.readers_hash);
                }
            }
        }
    } else if ui_prefs.market_sync_enabled
        && !upload_targets.is_empty()
        && (sync_state.last_readers_hash.is_some()
            || !sync_state.card_base_uploaded_at_by_write_url.is_empty())
    {
        let delta_prefix = humoco_market::derive_delta_prefix(id_ctx.my_prefix.as_deref());
        for drop_desc in &upload_targets {
            if drop_desc.write_url.is_empty() {
                continue;
            }
            let signer = DropChannelSigner {
                is_public: false,
                master_seed: &id_ctx.master_seed,
                my_ed_sk: &id_ctx.my_ed_sk,
                drop_desc,
            };
            for pfx in [id_ctx.my_prefix.as_deref(), Some(delta_prefix.as_str())] {
                if let Ok(signed) = signer.sign_request(
                    "DELETE",
                    "card",
                    pfx,
                    now_secs,
                    &sync_client::generate_sync_nonce(),
                    b"",
                ) {
                    let _ = sync_client::delete_slot(client, &drop_desc.write_url, &signed, "card")
                        .await;
                }
            }
        }
        sync_state.last_readers_hash = None;
        sync_state.card_base_uploaded_at_by_write_url.clear();
        sync_state.card_base_size_by_write_url.clear();
        sync_state.card_base_versions_by_write_url.clear();
        sync_state.card_delta_active_by_write_url.clear();
        sync_state.card_last_uploaded_versions_by_write_url.clear();
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
        *state.contacts.lock().unwrap() = None;

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
}
