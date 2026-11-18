use std::collections::HashSet;

use crate::models::{
    FrontendAddressBook, FrontendError, FrontendPublicSyncInfo, FrontendSyncDescriptor,
};
use crate::services::common::{normalize_sync_url, DropChannelSigner, SyncIdentityContext};
use crate::services::market_service::sync_own_card_drops_on_settings_change;
use crate::services::recovery_service::enqueue_guardian_locator_updates_if_configured;
use crate::settings::{AppSettings, StoredPublicSync, MAX_POLLED_SYNCS};
use crate::state::{
    apply_revocation_notice, apply_revocation_notice_to_address_book, canonicalize_did,
    clear_revocation, AppState, StoredBackupSyncState, StoredPostState, StoredSyncState,
    BACKUP_SYNC_STATE_KEY, CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY, MARKET_POST_STATE_KEY,
    MARKET_SYNC_KEY,
};
use crate::sync_client::{self, IndexFetchResult, SlotFetchResult};

use super::card_sync::import_decrypted_card_bundle;
use super::wot_sync::import_decrypted_wot_package;

pub(crate) struct PollOutcome {
    pub polled_syncs: usize,
    pub decrypted_slots: usize,
    pub imported_cards: usize,
    pub new_ads: usize,
    pub imported_wot_lists: usize,
    pub sync_health_updates: Vec<(String, bool)>,
    pub updated_card_store: humoco_market::CardStore,
}

pub(crate) fn channel_max_bytes_key(write_url: &str, channel: &str) -> String {
    format!("{}:{}", normalize_sync_url(write_url), channel)
}

pub(crate) fn save_channel_max_bytes_updates(
    state: &AppState,
    password: Option<&str>,
    updates: &[(String, String, usize)],
) {
    if updates.is_empty() {
        return;
    }
    let Ok(mut service) = state.lock_service() else {
        return;
    };
    let mut sync_state: StoredSyncState = service
        .load_encrypted_data(MARKET_SYNC_KEY, password)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut changed = false;
    for (write_url, channel, mb) in updates {
        if *mb > 0 {
            let key = channel_max_bytes_key(write_url, channel);
            if sync_state.channel_max_bytes_by_write_url.get(&key) != Some(mb) {
                sync_state.channel_max_bytes_by_write_url.insert(key, *mb);
                changed = true;
            }
        }
    }
    if changed {
        if let Ok(bytes) = serde_json::to_vec(&sync_state) {
            let _ = service.save_encrypted_data(MARKET_SYNC_KEY, &bytes, password);
        }
    }
}

pub(crate) fn collect_upload_targets_for_channel(
    settings: &AppSettings,
    channel: &str,
) -> Vec<humoco_market::DropDescriptor> {
    let mut targets: Vec<humoco_market::DropDescriptor> = settings
        .market_selected_syncs
        .iter()
        .cloned()
        .map(Into::into)
        .collect();

    for pd in &settings.public_syncs {
        if pd.enabled
            && !pd.write_url.is_empty()
            && (pd.authorized_channels.is_empty()
                || pd.authorized_channels.iter().any(|c| c == channel))
        {
            targets.push(humoco_market::DropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(pd.access_pubkey.clone()),
                drop_pub: pd.drop_pub.clone(),
            });
        }
    }

    targets
}

pub(crate) async fn ensure_cached_channel_max_bytes(
    client: &reqwest::Client,
    sync_state: &mut StoredSyncState,
    id_ctx: &SyncIdentityContext,
    drop_desc: &humoco_market::DropDescriptor,
    prefix: Option<&str>,
    channel: &str,
    default_max: usize,
) -> usize {
    let key = channel_max_bytes_key(&drop_desc.write_url, channel);
    if let Some(&cached) = sync_state.channel_max_bytes_by_write_url.get(&key) {
        return cached;
    }
    let signer = DropChannelSigner {
        is_public: false,
        master_seed: &id_ctx.master_seed,
        my_ed_sk: &id_ctx.my_ed_sk,
        drop_desc,
    };
    if let Ok(slot_pk) = signer.derive_slot_pubkey() {
        if let Ok(cap) = sync_client::head_slot_capability(
            client,
            &drop_desc.write_url,
            &slot_pk,
            prefix,
            channel,
        )
        .await
        {
            if let Some(mb) = cap.max_bytes.filter(|&v| v > 0) {
                sync_state.channel_max_bytes_by_write_url.insert(key, mb);
                return mb;
            }
        }
    }
    default_max
}

pub(crate) async fn delete_delta_card_slot(
    client: &reqwest::Client,
    id_ctx: &SyncIdentityContext,
    drop_desc: &humoco_market::DropDescriptor,
    delta_prefix: &str,
    now_secs: u64,
) -> bool {
    let nonce = sync_client::generate_sync_nonce();
    let Ok(del_signed) = (DropChannelSigner {
        is_public: false,
        master_seed: &id_ctx.master_seed,
        my_ed_sk: &id_ctx.my_ed_sk,
        drop_desc,
    })
    .sign_request("DELETE", "card", Some(delta_prefix), now_secs, &nonce, b"") else {
        return false;
    };
    sync_client::delete_slot(client, &drop_desc.write_url, &del_signed, "card")
        .await
        .is_ok()
}

/// Checks `revocation.json` (max once per 24h per `read_url`) across own and contact syncs.
pub(crate) async fn check_revocations(
    client: &reqwest::Client,
    state: &AppState,
    settings: &mut AppSettings,
    sync_state: &mut StoredSyncState,
    now_secs: u64,
    password: Option<&str>,
) -> Vec<FrontendSyncDescriptor> {
    let mut revoked_own_syncs: Vec<FrontendSyncDescriptor> = Vec::new();
    let revocation_candidates: Vec<(String, String)> = {
        let Ok(mut service) = state.lock_service() else {
            return revoked_own_syncs;
        };
        let mut seen_urls = HashSet::new();
        let mut list = Vec::new();
        let mut push_pair = |read_url: &str, drop_pub: &str| {
            let r = read_url.trim();
            let p = drop_pub.trim().to_lowercase();
            if !r.is_empty() && p.len() == 64 && p.chars().all(|c| c.is_ascii_hexdigit()) {
                let norm = normalize_sync_url(r);
                if seen_urls.insert(norm) {
                    list.push((r.to_string(), p));
                }
            }
        };
        for d in &settings.market_selected_syncs {
            push_pair(&d.read_url, &d.drop_pub);
        }
        for ps in &settings.public_syncs {
            push_pair(&ps.read_url, &ps.drop_pub);
        }
        if let Ok(contacts) = state.load_contacts(&mut service, password) {
            for c in contacts {
                for d in &c.syncs {
                    push_pair(&d.read_url, &d.drop_pub);
                }
            }
        }
        list
    };

    for (rev_read_url, rev_drop_pub) in revocation_candidates {
        let last_chk = sync_state
            .last_revocation_check_at
            .get(&rev_read_url)
            .copied()
            .unwrap_or(0);
        if now_secs.saturating_sub(last_chk) < 86_400 {
            continue;
        }
        sync_state
            .last_revocation_check_at
            .insert(rev_read_url.clone(), now_secs);

        match sync_client::fetch_revocation_notice(client, &rev_read_url, &rev_drop_pub).await {
            Ok(Some(notice)) => {
                let norm_target = normalize_sync_url(&rev_read_url);
                let Ok(mut service) = state.lock_service() else {
                    continue;
                };

                // Update own settings first (`service -> settings`), then contacts (`service -> contacts`).
                let mut settings_changed = false;
                for d in &mut settings.market_selected_syncs {
                    if normalize_sync_url(&d.read_url) == norm_target
                        && d.drop_pub.trim().eq_ignore_ascii_case(&notice.drop_pub)
                    {
                        apply_revocation_notice(d, &notice, now_secs);
                        revoked_own_syncs.push(d.clone());
                        settings_changed = true;
                    }
                }
                for ps in &mut settings.public_syncs {
                    if normalize_sync_url(&ps.read_url) == norm_target
                        && ps.drop_pub.trim().eq_ignore_ascii_case(&notice.drop_pub)
                    {
                        apply_revocation_notice(ps, &notice, now_secs);
                        revoked_own_syncs.push(FrontendSyncDescriptor {
                            write_url: ps.write_url.clone(),
                            read_url: ps.read_url.clone(),
                            slot_id: None,
                            drop_pub: ps.drop_pub.clone(),
                            status: ps.status.clone(),
                            sunset_at: ps.sunset_at,
                            revocation_reason: ps.revocation_reason.clone(),
                            replacement: ps.replacement.clone(),
                            ..Default::default()
                        });
                        settings_changed = true;
                    }
                }
                if settings_changed {
                    let _ = state.save_settings(&mut service, settings.clone(), password);
                }

                if let Ok(data) = service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
                    if let Ok(mut ab) = serde_json::from_slice::<FrontendAddressBook>(&data) {
                        if apply_revocation_notice_to_address_book(
                            &mut ab,
                            &rev_read_url,
                            &notice,
                            now_secs,
                        ) {
                            if let Ok(bytes) = serde_json::to_vec(&ab) {
                                let _ = service.save_encrypted_data(
                                    CONTACTS_DATA_NAME,
                                    &bytes,
                                    password,
                                );
                            }
                            *state.contacts.lock().unwrap() = Some(ab);
                        }
                    }
                }
            }
            Ok(None) => {
                // Server returned 404 (no revocation / revocation cleared): clear stale local state.
                let norm_target = normalize_sync_url(&rev_read_url);
                let Ok(mut service) = state.lock_service() else {
                    continue;
                };

                let mut settings_changed = false;
                for d in &mut settings.market_selected_syncs {
                    if normalize_sync_url(&d.read_url) == norm_target
                        && d.drop_pub.trim().eq_ignore_ascii_case(&rev_drop_pub)
                        && clear_revocation(d, "active")
                    {
                        settings_changed = true;
                    }
                }
                for ps in &mut settings.public_syncs {
                    if normalize_sync_url(&ps.read_url) == norm_target
                        && ps.drop_pub.trim().eq_ignore_ascii_case(&rev_drop_pub)
                    {
                        let fallback = if ps.authorized_channels.is_empty() {
                            "pending_auth"
                        } else {
                            "active"
                        };
                        if clear_revocation(ps, fallback) {
                            settings_changed = true;
                        }
                    }
                }
                if settings_changed {
                    let _ = state.save_settings(&mut service, settings.clone(), password);
                }

                if let Ok(data) = service.load_encrypted_data(CONTACTS_DATA_NAME, password) {
                    if let Ok(mut ab) = serde_json::from_slice::<FrontendAddressBook>(&data) {
                        let mut ab_changed = false;
                        for contact in ab.contacts.values_mut() {
                            for ep in &mut contact.syncs {
                                if normalize_sync_url(&ep.read_url) == norm_target
                                    && ep.drop_pub.trim().eq_ignore_ascii_case(&rev_drop_pub)
                                    && clear_revocation(ep, "unverified")
                                {
                                    ab_changed = true;
                                }
                            }
                        }
                        if ab_changed {
                            if let Ok(bytes) = serde_json::to_vec(&ab) {
                                let _ = service.save_encrypted_data(
                                    CONTACTS_DATA_NAME,
                                    &bytes,
                                    password,
                                );
                            }
                            *state.contacts.lock().unwrap() = Some(ab);
                        }
                    }
                }
            }
            Err(_) => {}
        }
    }

    revoked_own_syncs
}

type DirectPollTarget = (String, String, Option<String>);
type PollTargetSets = (Vec<DirectPollTarget>, Vec<String>);

fn collect_poll_targets(
    state: &AppState,
    settings: &AppSettings,
    my_prefix: Option<&str>,
    password: Option<&str>,
) -> PollTargetSets {
    let Ok(mut service) = state.lock_service() else {
        return (Vec::new(), Vec::new());
    };

    let mut direct_fetches: Vec<(String, String, Option<String>)> = Vec::new();
    let mut index_urls = Vec::new();

    for d in &settings.market_selected_syncs {
        if !d.read_url.is_empty() {
            if let Some(ref sid) = d.slot_id {
                direct_fetches.push((
                    d.read_url.clone(),
                    sid.clone(),
                    my_prefix.map(|s| s.to_string()),
                ));
            } else {
                index_urls.push(d.read_url.clone());
            }
        }
    }

    let reg = state.get_or_load_wot_register(&mut service, password).ok();
    let card_store: Option<humoco_market::CardStore> = service
        .load_encrypted_data(MARKET_CARD_STORE_KEY, password)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    if let Ok(contacts) = state.load_contacts(&mut service, password) {
        for c in contacts {
            let lvl = c.trust_level.unwrap_or_else(|| {
                if let Some(ref r) = reg {
                    let can = canonicalize_did(&c.did).unwrap_or_else(|_| c.did.clone());
                    r.own_contacts.get(&can).map(|e| e.level).unwrap_or(0)
                } else {
                    0
                }
            });
            if lvl >= settings.share_min_level {
                for d in c.syncs {
                    if !d.read_url.is_empty() {
                        if let Some(ref sid) = d.slot_id {
                            direct_fetches.push((d.read_url.clone(), sid.clone(), None));
                        } else {
                            index_urls.push(d.read_url.clone());
                        }
                    }
                    if let Some(ref rep) = d.replacement {
                        if !rep.read_url.is_empty() && rep.read_url != d.read_url {
                            index_urls.push(rep.read_url.clone());
                        }
                    }
                }
            }
        }
    }

    if let Some(ref store) = card_store {
        for sc in store.cards.values() {
            let author_can = canonicalize_did(&sc.card.payload.did)
                .unwrap_or_else(|_| sc.card.payload.did.clone());
            let lvl = if let Some(ref r) = reg {
                r.own_contacts
                    .get(&author_can)
                    .map(|e| e.level)
                    .unwrap_or(0)
            } else {
                0
            };
            if lvl >= settings.share_min_level {
                for d in &sc.card.payload.drops {
                    if !d.read_url.is_empty() {
                        if let Some(ref sid) = d.slot_id {
                            direct_fetches.push((d.read_url.clone(), sid.clone(), None));
                        } else {
                            index_urls.push(d.read_url.clone());
                        }
                    }
                }
            }
        }
    }

    let mut deduped_urls = Vec::new();
    let mut seen_urls = HashSet::new();
    for u in index_urls {
        if seen_urls.insert(u.clone()) {
            deduped_urls.push(u);
            if deduped_urls.len() >= MAX_POLLED_SYNCS {
                break;
            }
        }
    }

    let mut deduped_direct = Vec::new();
    let mut seen_direct = HashSet::new();
    for item in direct_fetches {
        if seen_direct.insert(item.clone()) {
            deduped_direct.push(item);
        }
    }

    (deduped_direct, deduped_urls)
}

struct SlotDecodeContext<'a> {
    state: &'a AppState,
    my_x25519_sk: &'a x25519_dalek::StaticSecret,
    now_secs: u64,
    max_stored_cards: usize,
    password: Option<&'a str>,
    store: &'a mut humoco_market::CardStore,
    sync_state: &'a mut StoredSyncState,
    outcome: &'a mut PollOutcome,
    errors: &'a mut Vec<String>,
}

fn process_card_slot_blob(
    ctx: &mut SlotDecodeContext<'_>,
    blob: &[u8],
    read_url: &str,
    slot_label: &str,
    is_delta: bool,
) {
    match humoco_market::open_slot_blob(blob, ctx.my_x25519_sk) {
        Ok(Some(bundle)) => {
            ctx.outcome.decrypted_slots += 1;
            let (cards_count, ads_count) = import_decrypted_card_bundle(
                bundle,
                read_url,
                ctx.now_secs,
                ctx.max_stored_cards,
                ctx.state,
                ctx.store,
                ctx.sync_state,
                ctx.password,
            );
            ctx.outcome.imported_cards += cards_count;
            ctx.outcome.new_ads += ads_count;
        }
        Ok(None) => {}
        Err(e) => {
            let kind = if is_delta {
                "delta card slot"
            } else {
                "card slot"
            };
            ctx.errors
                .push(format!("Error opening {} {}: {}", kind, slot_label, e));
        }
    }
}

fn process_wot_slot_blob(
    ctx: &mut SlotDecodeContext<'_>,
    blob: &[u8],
    read_url: &str,
    slot_label: &str,
    is_direct: bool,
) {
    match humoco_market::open_slot_payload(blob, ctx.my_x25519_sk) {
        Ok(Some(bytes)) => {
            ctx.outcome.decrypted_slots += 1;
            match serde_json::from_slice::<humoco_wot::WotSyncPackage>(&bytes) {
                Ok(pkg) => {
                    ctx.outcome.imported_wot_lists += import_decrypted_wot_package(
                        &pkg,
                        read_url,
                        ctx.now_secs,
                        ctx.state,
                        ctx.password,
                        ctx.errors,
                    );
                }
                Err(e) => {
                    let kind = if is_direct {
                        "direct wot sync package"
                    } else {
                        "wot sync package"
                    };
                    ctx.errors
                        .push(format!("Failed to parse {} from {}: {}", kind, read_url, e));
                }
            }
        }
        Ok(None) => {}
        Err(e) => {
            let kind = if is_direct {
                "direct wot slot"
            } else {
                "wot slot"
            };
            ctx.errors
                .push(format!("Error opening {} {}: {}", kind, slot_label, e));
        }
    }
}

/// Polls direct O(1) slots and index-listed sync drops for card and WoT updates.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn poll_sync_channels(
    client: &reqwest::Client,
    state: &AppState,
    settings: &AppSettings,
    id_ctx: &SyncIdentityContext,
    now_secs: u64,
    password: Option<&str>,
    sync_state: &mut StoredSyncState,
    errors: &mut Vec<String>,
) -> PollOutcome {
    sync_state.last_poll_at = Some(now_secs);

    let (direct_slot_fetches, candidate_read_urls) =
        collect_poll_targets(state, settings, id_ctx.my_prefix.as_deref(), password);

    let mut store: humoco_market::CardStore = state
        .lock_service()
        .ok()
        .and_then(|mut service| {
            service
                .load_encrypted_data(MARKET_CARD_STORE_KEY, password)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        })
        .unwrap_or_default();

    let max_stored_cards = settings.max_stored_cards.clamp(100, 10_000);
    let my_x25519_sk = human_money_core::crypto::ed25519_sk_to_x25519_sk(&id_ctx.my_ed_sk);

    let mut outcome = PollOutcome {
        polled_syncs: 0,
        decrypted_slots: 0,
        imported_cards: 0,
        new_ads: 0,
        imported_wot_lists: 0,
        sync_health_updates: Vec::new(),
        updated_card_store: humoco_market::CardStore::default(),
    };

    let mut decode_ctx = SlotDecodeContext {
        state,
        my_x25519_sk: &my_x25519_sk,
        now_secs,
        max_stored_cards,
        password,
        store: &mut store,
        sync_state,
        outcome: &mut outcome,
        errors,
    };

    // 1. Direct O(1) fetches
    for (read_url, slot_id, prefix_opt) in direct_slot_fetches {
        decode_ctx.outcome.polled_syncs += 1;
        let (base_slot_pk, slot_prefix) =
            humoco_market::parse_slot_id(&slot_id).unwrap_or_else(|_| (slot_id.clone(), None));
        let card_prefix = slot_prefix.as_deref().or(prefix_opt.as_deref());

        if settings.market_sync_enabled {
            let base_pfx_str = card_prefix.unwrap_or("");
            let base_etag_key = format!("{}:card:{}:{}", read_url, base_slot_pk, base_pfx_str);
            let base_cached_etag = decode_ctx
                .sync_state
                .slot_etags
                .get(&base_etag_key)
                .map(|s| s.as_str());
            match sync_client::get_slot(
                client,
                &read_url,
                &base_slot_pk,
                card_prefix,
                "card",
                base_cached_etag,
            )
            .await
            {
                Ok(SlotFetchResult::Modified { bytes: blob, etag }) => {
                    decode_ctx
                        .outcome
                        .sync_health_updates
                        .push((read_url.clone(), true));
                    if let Some(et) = etag {
                        decode_ctx.sync_state.slot_etags.insert(base_etag_key, et);
                    }
                    process_card_slot_blob(&mut decode_ctx, &blob, &read_url, &slot_id, false);
                }
                Ok(SlotFetchResult::NotModified) => {
                    decode_ctx
                        .outcome
                        .sync_health_updates
                        .push((read_url.clone(), true));
                }
                Ok(SlotFetchResult::NotFound) => {
                    decode_ctx
                        .outcome
                        .sync_health_updates
                        .push((read_url.clone(), true));
                    decode_ctx.sync_state.slot_etags.remove(&base_etag_key);
                }
                Err(e) => {
                    decode_ctx
                        .outcome
                        .sync_health_updates
                        .push((read_url.clone(), false));
                    decode_ctx.errors.push(format!(
                        "Failed to get direct card slot {} from {}: {}",
                        slot_id, read_url, e
                    ));
                }
            }

            let delta_pfx = humoco_market::derive_delta_prefix(card_prefix);
            let delta_etag_key = format!("{}:card:{}:{}", read_url, base_slot_pk, delta_pfx);
            let delta_cached_etag = decode_ctx
                .sync_state
                .slot_etags
                .get(&delta_etag_key)
                .map(|s| s.as_str());
            match sync_client::get_slot(
                client,
                &read_url,
                &base_slot_pk,
                Some(&delta_pfx),
                "card",
                delta_cached_etag,
            )
            .await
            {
                Ok(SlotFetchResult::Modified { bytes: blob, etag }) => {
                    if let Some(et) = etag {
                        decode_ctx.sync_state.slot_etags.insert(delta_etag_key, et);
                    }
                    process_card_slot_blob(&mut decode_ctx, &blob, &read_url, &slot_id, true);
                }
                Ok(SlotFetchResult::NotModified) => {}
                Ok(SlotFetchResult::NotFound) => {
                    decode_ctx.sync_state.slot_etags.remove(&delta_etag_key);
                }
                Err(e) => {
                    decode_ctx.errors.push(format!(
                        "Failed to get direct delta card slot {} from {}: {}",
                        slot_id, read_url, e
                    ));
                }
            }
        }

        // Fetch WoT slot
        let wot_etag_key = format!("{}:wot:{}:", read_url, base_slot_pk);
        let wot_cached_etag = decode_ctx
            .sync_state
            .slot_etags
            .get(&wot_etag_key)
            .map(|s| s.as_str());
        match sync_client::get_slot(
            client,
            &read_url,
            &base_slot_pk,
            None,
            "wot",
            wot_cached_etag,
        )
        .await
        {
            Ok(SlotFetchResult::Modified { bytes: blob, etag }) => {
                decode_ctx
                    .outcome
                    .sync_health_updates
                    .push((read_url.clone(), true));
                if let Some(et) = etag {
                    decode_ctx.sync_state.slot_etags.insert(wot_etag_key, et);
                }
                process_wot_slot_blob(&mut decode_ctx, &blob, &read_url, &slot_id, true);
            }
            Ok(SlotFetchResult::NotModified) => {
                decode_ctx
                    .outcome
                    .sync_health_updates
                    .push((read_url.clone(), true));
            }
            Ok(SlotFetchResult::NotFound) => {
                decode_ctx
                    .outcome
                    .sync_health_updates
                    .push((read_url.clone(), true));
                decode_ctx.sync_state.slot_etags.remove(&wot_etag_key);
            }
            Err(e) => {
                decode_ctx
                    .outcome
                    .sync_health_updates
                    .push((read_url.clone(), false));
                decode_ctx.errors.push(format!(
                    "Failed to get direct wot slot {} from {}: {}",
                    slot_id, read_url, e
                ));
            }
        }
    }

    // 2. Index-based fetches
    for read_url in candidate_read_urls {
        decode_ctx.outcome.polled_syncs += 1;
        let cached_etag = decode_ctx
            .sync_state
            .etags_by_read_url
            .get(&read_url)
            .map(|s| s.as_str());
        let cached_lm = decode_ctx
            .sync_state
            .last_modified_by_read_url
            .get(&read_url)
            .map(|s| s.as_str());

        match sync_client::list_index(client, &read_url, cached_etag, cached_lm).await {
            Ok(IndexFetchResult::NotModified) => {
                decode_ctx
                    .outcome
                    .sync_health_updates
                    .push((read_url.clone(), true));
            }
            Ok(IndexFetchResult::Modified {
                mut entries,
                etag,
                last_modified,
            }) => {
                decode_ctx
                    .outcome
                    .sync_health_updates
                    .push((read_url.clone(), true));
                if let Some(et) = etag {
                    decode_ctx
                        .sync_state
                        .etags_by_read_url
                        .insert(read_url.clone(), et);
                }
                if let Some(lm) = last_modified {
                    decode_ctx
                        .sync_state
                        .last_modified_by_read_url
                        .insert(read_url.clone(), lm);
                }

                // Order Base card entries before Delta card entries
                entries.sort_by_key(|e| {
                    let ch_rank = match e.channel.as_str() {
                        "card" => 0u8,
                        "wot" => 1u8,
                        _ => 2u8,
                    };
                    let delta_rank = if e.channel == "card"
                        && e.prefix
                            .as_deref()
                            .is_some_and(|p| p == "delta" || p.ends_with("-delta"))
                    {
                        1u8
                    } else {
                        0u8
                    };
                    (ch_rank, delta_rank)
                });

                // Prune slot ETags for entries no longer advertised by this read_url index
                let present_keys: HashSet<String> = entries
                    .iter()
                    .filter(|e| e.channel == "card" || e.channel == "wot")
                    .map(|e| {
                        let pfx = if e.channel == "wot" {
                            ""
                        } else {
                            e.prefix.as_deref().unwrap_or("")
                        };
                        format!("{}:{}:{}:{}", read_url, e.channel, e.id, pfx)
                    })
                    .collect();
                let url_prefix = format!("{}:", read_url);
                decode_ctx.sync_state.slot_etags.retain(|k, _| {
                    if k.starts_with(&url_prefix) {
                        present_keys.contains(k)
                    } else {
                        true
                    }
                });

                for entry in entries {
                    if entry.channel != "card" && entry.channel != "wot" {
                        continue;
                    }
                    if entry.channel == "card" && !settings.market_sync_enabled {
                        continue;
                    }
                    let pfx_opt = if entry.channel == "wot" {
                        None
                    } else {
                        entry.prefix.as_deref()
                    };
                    let pfx_str = pfx_opt.unwrap_or("");
                    let etag_key =
                        format!("{}:{}:{}:{}", read_url, entry.channel, entry.id, pfx_str);
                    let cached_slot_etag = decode_ctx
                        .sync_state
                        .slot_etags
                        .get(&etag_key)
                        .map(|s| s.as_str());
                    if let (Some(e_tag), Some(c_tag)) = (
                        entry
                            .etag
                            .as_deref()
                            .map(|s| s.trim().trim_matches('"'))
                            .filter(|s| !s.is_empty()),
                        cached_slot_etag,
                    ) {
                        if e_tag == c_tag.trim().trim_matches('"') {
                            continue;
                        }
                    }

                    match sync_client::get_slot(
                        client,
                        &read_url,
                        &entry.id,
                        pfx_opt,
                        &entry.channel,
                        cached_slot_etag,
                    )
                    .await
                    {
                        Ok(SlotFetchResult::Modified { bytes: blob, etag }) => {
                            if let Some(et) = etag.or_else(|| entry.etag.clone()) {
                                decode_ctx.sync_state.slot_etags.insert(etag_key, et);
                            }
                            if entry.channel == "card" {
                                process_card_slot_blob(
                                    &mut decode_ctx,
                                    &blob,
                                    &read_url,
                                    &entry.id,
                                    false,
                                );
                            } else {
                                process_wot_slot_blob(
                                    &mut decode_ctx,
                                    &blob,
                                    &read_url,
                                    &entry.id,
                                    false,
                                );
                            }
                        }
                        Ok(SlotFetchResult::NotModified) => {
                            if cached_slot_etag.is_none() {
                                if let Some(et) = entry.etag.clone() {
                                    decode_ctx.sync_state.slot_etags.insert(etag_key, et);
                                }
                            }
                        }
                        Ok(SlotFetchResult::NotFound) => {
                            decode_ctx.sync_state.slot_etags.remove(&etag_key);
                        }
                        Err(e) => {
                            decode_ctx.errors.push(format!(
                                "Failed to get {} slot {}: {}",
                                entry.channel, entry.id, e
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                decode_ctx
                    .outcome
                    .sync_health_updates
                    .push((read_url.clone(), false));
                decode_ctx
                    .errors
                    .push(format!("Failed to list index from {}: {}", read_url, e));
            }
        }
    }

    outcome.updated_card_store = store;
    outcome
}

pub async fn delete_own_sync_slots_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<usize, FrontendError> {
    let (id_ctx, drops) = {
        let mut service = state.lock_service()?;
        let id_ctx = SyncIdentityContext::from_service(&service)?;
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        (id_ctx, settings.market_selected_syncs)
    };

    let client = sync_client::create_sync_http_client();
    let mut deleted_count = 0;
    let now = sync_client::current_unix_timestamp();
    let delta_prefix = humoco_market::derive_delta_prefix(id_ctx.my_prefix.as_deref());

    for drop_desc in &drops {
        let desc: humoco_market::DropDescriptor = drop_desc.clone().into();
        let signer = DropChannelSigner {
            is_public: false,
            master_seed: &id_ctx.master_seed,
            my_ed_sk: &id_ctx.my_ed_sk,
            drop_desc: &desc,
        };
        for (ch, ch_prefix) in [
            ("card", id_ctx.my_prefix.as_deref()),
            ("card", Some(delta_prefix.as_str())),
            ("wot", None),
            ("backup", id_ctx.my_prefix.as_deref()),
            ("backup", Some(delta_prefix.as_str())),
            ("post", id_ctx.my_prefix.as_deref()),
        ] {
            if let Ok(signed) = signer.sign_request(
                "DELETE",
                ch,
                ch_prefix,
                now,
                &sync_client::generate_sync_nonce(),
                b"",
            ) {
                if sync_client::delete_slot(&client, &drop_desc.write_url, &signed, ch)
                    .await
                    .is_ok()
                {
                    deleted_count += 1;
                }
            }
        }
    }

    // Clear drop_sent_log, sync_state, post slot ETags, and backup mirror state
    {
        let mut service = state.lock_service()?;
        if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
            reg.clear_drop_sent_log(None);
            let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
        }

        if let Ok(mut sync_state) = service
            .load_encrypted_data(MARKET_SYNC_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<StoredSyncState>(&bytes)
                    .map_err(|e| FrontendError::from(format!("Parse error: {}", e)))
            })
        {
            sync_state.last_upload_at = None;
            sync_state.last_poll_at = None;
            sync_state.last_readers_hash = None;
            sync_state.card_base_uploaded_at_by_write_url.clear();
            sync_state.card_base_size_by_write_url.clear();
            sync_state.card_base_versions_by_write_url.clear();
            sync_state.card_delta_active_by_write_url.clear();
            sync_state.card_last_uploaded_versions_by_write_url.clear();
            if let Ok(bytes) = serde_json::to_vec(&sync_state) {
                let _ = service.save_encrypted_data(MARKET_SYNC_KEY, &bytes, password.as_deref());
            }
        }

        if let Ok(mut post_state) = service
            .load_encrypted_data(MARKET_POST_STATE_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<StoredPostState>(&bytes)
                    .map_err(|e| FrontendError::from(format!("Parse error: {}", e)))
            })
        {
            post_state.slot_etags_by_write_url.clear();
            if let Ok(bytes) = serde_json::to_vec(&post_state) {
                let _ =
                    service.save_encrypted_data(MARKET_POST_STATE_KEY, &bytes, password.as_deref());
            }
        }

        if let Ok(bytes) = serde_json::to_vec(&StoredBackupSyncState::default()) {
            let _ = service.save_encrypted_data(BACKUP_SYNC_STATE_KEY, &bytes, password.as_deref());
        }
    }

    Ok(deleted_count)
}

async fn probe_public_sync_capabilities(
    probe_client: &reqwest::Client,
    canonical_url: &str,
    access_pubkey: &str,
    my_prefix: Option<&str>,
    state: &AppState,
    password: Option<&str>,
) -> Vec<String> {
    let mut authorized_channels = Vec::new();
    let mut channel_max_bytes_updates = Vec::new();
    for ch in ["card", "wot", "backup", "post"] {
        let ch_prefix = if ch == "wot" { None } else { my_prefix };
        if let Ok(cap) = sync_client::head_slot_capability(
            probe_client,
            canonical_url,
            access_pubkey,
            ch_prefix,
            ch,
        )
        .await
        {
            if let Some(mb) = cap.max_bytes.filter(|&v| v > 0) {
                channel_max_bytes_updates.push((canonical_url.to_string(), ch.to_string(), mb));
            }
            if cap.authorized {
                authorized_channels.push(ch.to_string());
            }
        }
    }
    save_channel_max_bytes_updates(state, password, &channel_max_bytes_updates);
    authorized_channels
}

pub async fn probe_public_sync_impl(
    url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPublicSyncInfo, FrontendError> {
    let canonical_url = humoco_market::canonicalize_write_url(&url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&canonical_url).map_err(FrontendError::from)?;

    let probe_client = sync_client::create_sync_probe_client();
    let info = sync_client::fetch_sync_info(&probe_client, &canonical_url)
        .await
        .map_err(FrontendError::from)?;

    let id_ctx = {
        let service = state.lock_service()?;
        SyncIdentityContext::from_service(&service)?
    };

    let (_r_sk, _r_vk, access_pubkey) =
        humoco_market::derive_public_drop_keypair(&id_ctx.master_seed, &canonical_url)
            .map_err(|e| FrontendError::from(e.to_string()))?;

    let authorized_channels = probe_public_sync_capabilities(
        &probe_client,
        &canonical_url,
        &access_pubkey,
        id_ctx.my_prefix.as_deref(),
        state,
        password.as_deref(),
    )
    .await;

    Ok(FrontendPublicSyncInfo {
        write_url: canonical_url,
        read_url: info.read_url,
        drop_pub: info.drop_pub,
        register_url: info.register_url,
        access_pubkey,
        authorized_channels,
    })
}

pub async fn save_public_sync_impl(
    write_url: String,
    read_url: String,
    drop_pub: String,
    register_url: Option<String>,
    enabled: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPublicSyncInfo, FrontendError> {
    let canonical_url = humoco_market::canonicalize_write_url(&write_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&canonical_url).map_err(FrontendError::from)?;
    sync_client::validate_sync_url(&read_url).map_err(FrontendError::from)?;
    humoco_market::parse_ed25519_pubkey_hex64(&drop_pub)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let id_ctx = {
        let service = state.lock_service()?;
        SyncIdentityContext::from_service(&service)?
    };

    let (_r_sk, _r_vk, access_pubkey) =
        humoco_market::derive_public_drop_keypair(&id_ctx.master_seed, &canonical_url)
            .map_err(|e| FrontendError::from(e.to_string()))?;

    let probe_client = sync_client::create_sync_probe_client();
    let info_opt = sync_client::fetch_sync_info(&probe_client, &canonical_url)
        .await
        .ok();
    let clean_register_url = register_url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty() && sync_client::validate_sync_url(u).is_ok())
        .or_else(|| info_opt.as_ref().and_then(|i| i.register_url.clone()));

    let authorized_channels = probe_public_sync_capabilities(
        &probe_client,
        &canonical_url,
        &access_pubkey,
        id_ctx.my_prefix.as_deref(),
        state,
        password.as_deref(),
    )
    .await;

    {
        let mut service = state.lock_service()?;
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        let mut stored = StoredPublicSync {
            write_url: canonical_url.clone(),
            read_url: read_url.clone(),
            drop_pub: drop_pub.clone(),
            register_url: clean_register_url.clone(),
            access_pubkey: access_pubkey.clone(),
            enabled,
            authorized_channels: authorized_channels.clone(),
            status: if !authorized_channels.is_empty() {
                "active".to_string()
            } else {
                "pending_auth".to_string()
            },
            ..Default::default()
        };

        if let Some(ref info) = info_opt {
            if let Some(ref notice) = info.revocation {
                if sync_client::verify_revocation_notice(notice, &drop_pub).is_ok() {
                    apply_revocation_notice(
                        &mut stored,
                        notice,
                        sync_client::current_unix_timestamp(),
                    );
                }
            }
        }

        if let Some(pos) = settings
            .public_syncs
            .iter()
            .position(|d| d.write_url == canonical_url)
        {
            settings.public_syncs[pos] = stored;
        } else {
            settings.public_syncs.push(stored);
        }

        let selected_syncs = settings.market_selected_syncs.clone();
        state
            .save_settings(&mut service, settings, password.as_deref())
            .map_err(FrontendError::from)?;

        sync_own_card_drops_on_settings_change(
            state,
            &mut service,
            &selected_syncs,
            password.as_deref(),
        );
    }

    let _ = enqueue_guardian_locator_updates_if_configured(state, password.clone()).await;

    Ok(FrontendPublicSyncInfo {
        write_url: canonical_url,
        read_url,
        drop_pub,
        register_url: clean_register_url,
        access_pubkey,
        authorized_channels,
    })
}

pub async fn remove_public_sync_impl(
    write_url: String,
    delete_remote_slots: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    {
        let mut service = state.lock_service()?;
        crate::services::common::ensure_access_mode(
            &mut service,
            human_money_core::AccessMode::Full,
            password.as_deref(),
        )?;
    }
    let canonical_url = humoco_market::canonicalize_write_url(&write_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    if delete_remote_slots {
        let id_ctx = {
            let service = state.lock_service()?;
            SyncIdentityContext::from_service(&service)?
        };
        if let Ok((r_sk, _, _)) =
            humoco_market::derive_public_drop_keypair(&id_ctx.master_seed, &canonical_url)
        {
            let client = sync_client::create_sync_http_client();
            let now = sync_client::current_unix_timestamp();
            let acct_nonce = sync_client::generate_sync_nonce();
            let acct_signed = humoco_market::sign_delete_account_request(&r_sk, now, &acct_nonce);
            if sync_client::delete_account(&client, &canonical_url, &acct_signed)
                .await
                .is_err()
            {
                let delta_prefix = humoco_market::derive_delta_prefix(id_ctx.my_prefix.as_deref());
                for (ch, ch_prefix) in [
                    ("card", id_ctx.my_prefix.as_deref()),
                    ("card", Some(delta_prefix.as_str())),
                    ("wot", None),
                    ("backup", id_ctx.my_prefix.as_deref()),
                    ("backup", Some(delta_prefix.as_str())),
                    ("post", id_ctx.my_prefix.as_deref()),
                ] {
                    let nonce = sync_client::generate_sync_nonce();
                    let signed = humoco_market::sign_member_request_direct(
                        &r_sk, "DELETE", ch, ch_prefix, now, &nonce, b"",
                    );
                    let _ = sync_client::delete_slot(&client, &canonical_url, &signed, ch).await;
                }
                let media_nonce = sync_client::generate_sync_nonce();
                let media_signed = humoco_market::sign_member_request_direct(
                    &r_sk,
                    "DELETE",
                    "media",
                    None,
                    now,
                    &media_nonce,
                    b"",
                );
                let _ =
                    sync_client::delete_media_blob(&client, &canonical_url, &media_signed, None)
                        .await;
            }
        }
    }

    {
        let mut service = state.lock_service()?;
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        settings
            .public_syncs
            .retain(|d| d.write_url != canonical_url);
        let selected_syncs = settings.market_selected_syncs.clone();
        state
            .save_settings(&mut service, settings, password.as_deref())
            .map_err(FrontendError::from)?;

        sync_own_card_drops_on_settings_change(
            state,
            &mut service,
            &selected_syncs,
            password.as_deref(),
        );

        if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
            reg.clear_drop_sent_log(Some(&canonical_url));
            let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
        }

        if let Ok(mut post_state) = service
            .load_encrypted_data(MARKET_POST_STATE_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<StoredPostState>(&bytes)
                    .map_err(|e| FrontendError::from(format!("Parse error: {}", e)))
            })
        {
            post_state.slot_etags_by_write_url.remove(&canonical_url);
            post_state.slot_etags_by_write_url.remove(&write_url);
            if let Ok(bytes) = serde_json::to_vec(&post_state) {
                let _ =
                    service.save_encrypted_data(MARKET_POST_STATE_KEY, &bytes, password.as_deref());
            }
        }

        if let Ok(bytes) = service.load_encrypted_data(BACKUP_SYNC_STATE_KEY, password.as_deref()) {
            if let Ok(mut backup_state) = serde_json::from_slice::<StoredBackupSyncState>(&bytes) {
                let removed = backup_state
                    .mirrors_by_write_url
                    .remove(&canonical_url)
                    .is_some()
                    || backup_state
                        .mirrors_by_write_url
                        .remove(&write_url)
                        .is_some();
                if removed {
                    if let Ok(encoded) = serde_json::to_vec(&backup_state) {
                        let _ = service.save_encrypted_data(
                            BACKUP_SYNC_STATE_KEY,
                            &encoded,
                            password.as_deref(),
                        );
                    }
                }
            }
        }
    }

    let _ = enqueue_guardian_locator_updates_if_configured(state, password.clone()).await;

    Ok(())
}
