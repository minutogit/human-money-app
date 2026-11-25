use std::collections::HashSet;

use crate::models::{
    FrontendAddressBook, FrontendError, FrontendPublicSyncInfo, FrontendSyncDescriptor,
};
use crate::services::common::{
    canonical_drop_pub_hex, normalize_sync_url, DropChannelSigner, SyncIdentityContext,
};
use crate::services::recovery_service::enqueue_guardian_locator_updates_if_configured;
use crate::services::sync_endpoints::{
    drop_pub_matches, is_endpoint_revoked, sanitize_untrusted_sync_descriptors,
};
use crate::session::SessionToken;
use crate::settings::{SecurityPolicy, StoredPublicSync, UiPreferences, MAX_POLLED_SYNCS};
use crate::state::{
    apply_revocation_notice, apply_revocation_notice_to_address_book, canonicalize_did,
    clear_revocation, AppState, EncryptedBatchCommit, StoredBackupSyncState, StoredPostState,
    StoredSyncState, BACKUP_SYNC_STATE_KEY, CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY,
    MARKET_POST_STATE_KEY, MARKET_SYNC_KEY,
};
use crate::sync_client::{self, IndexFetchResult, SlotFetchResult};

pub(crate) struct PollOutcome {
    pub polled_syncs: usize,
    pub decrypted_slots: usize,
    pub decoded_card_bundles: Vec<(String, humoco_market::CardBundle)>,
    pub decoded_wot_packages: Vec<(String, humoco_wot::WotSyncPackage)>,
    pub sync_health_updates: Vec<(String, bool)>,
}

pub(crate) fn channel_max_bytes_key(write_url: &str, channel: &str) -> String {
    format!("{}:{}", normalize_sync_url(write_url), channel)
}

pub(crate) fn save_channel_max_bytes_updates(
    state: &AppState,
    session: &SessionToken,
    password: Option<&str>,
    updates: &[(String, String, usize)],
) {
    if updates.is_empty() {
        return;
    }
    let _ = state.with_session(session, |service| {
        let mut sync_state: StoredSyncState = match crate::state::load_encrypted_json_or_default(
            service,
            MARKET_SYNC_KEY,
            password,
        ) {
            Ok(s) => s,
            Err(e) => {
                log::warn!(
                    "Failed to load {} before updating channel max bytes: {}",
                    MARKET_SYNC_KEY,
                    e
                );
                return Ok(());
            }
        };
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
            if let Err(e) = state.save_sync_state(service, &sync_state, password) {
                log::warn!(
                    "Failed to save {} after channel max bytes update: {}",
                    MARKET_SYNC_KEY,
                    e
                );
            }
        }
        Ok(())
    });
}

pub(crate) fn collect_upload_targets_for_channel(
    security_policy: &SecurityPolicy,
    channel: &str,
) -> Vec<humoco_market::DropDescriptor> {
    let mut targets: Vec<humoco_market::DropDescriptor> = security_policy
        .market_selected_syncs
        .iter()
        .filter(|d| !is_endpoint_revoked(&d.status) && canonical_drop_pub_hex(&d.drop_pub).is_ok())
        .cloned()
        .map(Into::into)
        .collect();

    for pd in &security_policy.public_syncs {
        if pd.enabled
            && !is_endpoint_revoked(&pd.status)
            && !pd.write_url.is_empty()
            && canonical_drop_pub_hex(&pd.drop_pub).is_ok()
            && (pd.authorized_channels.is_empty()
                || pd.authorized_channels.iter().any(|c| c == channel))
        {
            let Ok(clean_access_pk) = canonical_drop_pub_hex(&pd.access_pubkey) else {
                continue;
            };
            targets.push(humoco_market::DropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(clean_access_pk),
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

/// Checks `revocation.json` (max once per 24h per `(normalized read_url, drop_pub)`) across own and contact syncs.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn check_revocations(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    security_policy: &mut SecurityPolicy,
    sync_state: &mut StoredSyncState,
    now_secs: u64,
    password: Option<&str>,
) -> Vec<FrontendSyncDescriptor> {
    let mut revoked_own_syncs: Vec<FrontendSyncDescriptor> = Vec::new();
    let Ok(Some(revocation_candidates)) = state.with_session(session, |service| {
        let mut seen_pairs: HashSet<(String, String)> = HashSet::new();
        let mut list = Vec::new();
        let mut push_pair = |read_url: &str, drop_pub: &str| {
            let r = read_url.trim();
            if let Ok(p) = canonical_drop_pub_hex(drop_pub) {
                let norm = normalize_sync_url(r);
                if !norm.is_empty() && seen_pairs.insert((norm.clone(), p.clone())) {
                    list.push((norm, p));
                }
            }
        };
        for d in &security_policy.market_selected_syncs {
            push_pair(&d.read_url, &d.drop_pub);
        }
        for ps in &security_policy.public_syncs {
            push_pair(&ps.read_url, &ps.drop_pub);
        }
        if let Ok(contacts) = state.load_contacts(service, password) {
            for c in contacts {
                for d in &c.syncs {
                    push_pair(&d.read_url, &d.drop_pub);
                }
            }
        }
        Ok(list)
    }) else {
        return revoked_own_syncs;
    };

    for (norm_target, rev_drop_pub) in revocation_candidates {
        if !state.is_generation_current(session.generation) {
            break;
        }
        let rev_key = format!("{}::{}", norm_target, rev_drop_pub);
        let last_chk = sync_state
            .last_revocation_check_at
            .get(&rev_key)
            .copied()
            .unwrap_or(0);
        if last_chk <= now_secs && now_secs - last_chk < 86_400 {
            continue;
        }
        if state.is_origin_cooling_down(&norm_target, std::time::Instant::now()) {
            continue;
        }

        match sync_client::fetch_revocation_notice(client, &norm_target, &rev_drop_pub).await {
            Ok(Some(notice)) => {
                sync_state
                    .last_revocation_check_at
                    .insert(rev_key, now_secs);
                let committed = state.with_session(session, |service| {
                    let mut batch = EncryptedBatchCommit::new();
                    let mut updated_policy_local: Option<SecurityPolicy> = None;

                    if let Ok(mut fresh_policy) =
                        state.get_or_load_security_policy(service, password)
                    {
                        let mut policy_changed = false;
                        for d in &mut fresh_policy.market_selected_syncs {
                            if normalize_sync_url(&d.read_url) == norm_target
                                && drop_pub_matches(&d.drop_pub, &notice.drop_pub)
                            {
                                apply_revocation_notice(d, &notice, now_secs);
                                revoked_own_syncs.push(d.clone());
                                policy_changed = true;
                            }
                        }
                        for ps in &mut fresh_policy.public_syncs {
                            if normalize_sync_url(&ps.read_url) == norm_target
                                && drop_pub_matches(&ps.drop_pub, &notice.drop_pub)
                            {
                                apply_revocation_notice(ps, &notice, now_secs);
                                if is_endpoint_revoked(&ps.status) {
                                    ps.enabled = false;
                                }
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
                                policy_changed = true;
                            }
                        }
                        if policy_changed {
                            updated_policy_local = Some(fresh_policy.clone());
                            batch.security_policy(fresh_policy);
                        }
                    }

                    match crate::state::load_encrypted_json_opt::<FrontendAddressBook>(
                        service,
                        CONTACTS_DATA_NAME,
                        password,
                    ) {
                        Ok(Some(mut ab)) => {
                            if apply_revocation_notice_to_address_book(
                                &mut ab,
                                &norm_target,
                                &notice,
                                now_secs,
                            ) {
                                batch.contacts(ab);
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            log::warn!(
                                "Failed to load address book during revocation check: {}",
                                e
                            );
                        }
                    }

                    if let Err(e) = batch.commit(state, service, password) {
                        log::warn!("Failed to persist revocation notice updates: {}", e);
                    } else if let Some(pol) = updated_policy_local {
                        *security_policy = pol;
                    }
                    Ok(())
                });
                if !matches!(committed, Ok(Some(()))) {
                    break;
                }
            }
            Ok(None) => {
                // Server returned 404 (no revocation / revocation cleared): clear stale local state.
                sync_state
                    .last_revocation_check_at
                    .insert(rev_key, now_secs);
                let committed = state.with_session(session, |service| {
                    let mut batch = EncryptedBatchCommit::new();
                    let mut updated_policy_local: Option<SecurityPolicy> = None;

                    if let Ok(mut fresh_policy) =
                        state.get_or_load_security_policy(service, password)
                    {
                        let mut policy_changed = false;
                        for d in &mut fresh_policy.market_selected_syncs {
                            if normalize_sync_url(&d.read_url) == norm_target
                                && drop_pub_matches(&d.drop_pub, &rev_drop_pub)
                                && clear_revocation(d, "active", now_secs)
                            {
                                policy_changed = true;
                            }
                        }
                        for ps in &mut fresh_policy.public_syncs {
                            if normalize_sync_url(&ps.read_url) == norm_target
                                && drop_pub_matches(&ps.drop_pub, &rev_drop_pub)
                            {
                                let fallback = if ps.authorized_channels.is_empty() {
                                    "pending_auth"
                                } else {
                                    "active"
                                };
                                if clear_revocation(ps, fallback, now_secs) {
                                    policy_changed = true;
                                }
                            }
                        }
                        if policy_changed {
                            updated_policy_local = Some(fresh_policy.clone());
                            batch.security_policy(fresh_policy);
                        }
                    }

                    match crate::state::load_encrypted_json_opt::<FrontendAddressBook>(
                        service,
                        CONTACTS_DATA_NAME,
                        password,
                    ) {
                        Ok(Some(mut ab)) => {
                            let mut ab_changed = false;
                            for contact in ab.contacts.values_mut() {
                                for ep in &mut contact.syncs {
                                    if normalize_sync_url(&ep.read_url) == norm_target
                                        && drop_pub_matches(&ep.drop_pub, &rev_drop_pub)
                                        && clear_revocation(ep, "unverified", now_secs)
                                    {
                                        ab_changed = true;
                                    }
                                }
                            }
                            if ab_changed {
                                batch.contacts(ab);
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            log::warn!(
                                "Failed to load address book while clearing revocation: {}",
                                e
                            );
                        }
                    }

                    if let Err(e) = batch.commit(state, service, password) {
                        log::warn!("Failed to persist cleared revocation updates: {}", e);
                    } else if let Some(pol) = updated_policy_local {
                        *security_policy = pol;
                    }
                    Ok(())
                });
                if !matches!(committed, Ok(Some(()))) {
                    break;
                }
            }
            Err(e) => {
                state.record_origin_cooldown_for_sync_error(&norm_target, &e);
            }
        }
    }

    revoked_own_syncs
}

type DirectPollTarget = (String, String, Option<String>);
type PollTargetSets = (Vec<DirectPollTarget>, Vec<String>);

fn collect_poll_targets(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    ui_prefs: &UiPreferences,
    security_policy: &SecurityPolicy,
    my_prefix: Option<&str>,
    password: Option<&str>,
) -> PollTargetSets {
    let mut direct_fetches: Vec<(String, String, Option<String>)> = Vec::new();
    let mut index_urls = Vec::new();

    for d in &security_policy.market_selected_syncs {
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

    let reg = state.get_or_load_wot_register(service, password).ok();
    let card_store: Option<humoco_market::CardStore> =
        match crate::state::load_encrypted_json_opt(service, MARKET_CARD_STORE_KEY, password) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to load CardStore in collect_poll_targets: {}", e);
                None
            }
        };

    if let Ok(contacts) = state.load_contacts(service, password) {
        for c in contacts {
            let lvl = c.trust_level.unwrap_or_else(|| {
                if let Some(ref r) = reg {
                    let can = canonicalize_did(&c.did).unwrap_or_else(|_| c.did.clone());
                    r.own_contacts.get(&can).map(|e| e.level).unwrap_or(0)
                } else {
                    0
                }
            });
            if lvl >= ui_prefs.share_min_level {
                let c_prefix = human_money_core::crypto::get_prefix_from_user_id(&c.did);
                for d in c.syncs {
                    if !is_endpoint_revoked(&d.status)
                        && crate::sync_client::validate_sync_read_url(&d.read_url).is_ok()
                    {
                        if let Some(ref sid) = d.slot_id {
                            let sid_trimmed = sid.trim();
                            if humoco_market::parse_slot_id(sid_trimmed).is_ok() {
                                direct_fetches.push((
                                    d.read_url.clone(),
                                    sid_trimmed.to_string(),
                                    c_prefix.map(str::to_string),
                                ));
                            }
                        } else {
                            index_urls.push(d.read_url.clone());
                        }
                    }
                    if let Some(ref rep) = d.replacement {
                        if crate::sync_client::validate_sync_read_url(&rep.read_url).is_ok()
                            && normalize_sync_url(&rep.read_url) != normalize_sync_url(&d.read_url)
                        {
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
            if lvl >= ui_prefs.share_min_level {
                let sc_prefix =
                    human_money_core::crypto::get_prefix_from_user_id(&sc.card.payload.did)
                        .map(str::to_string);
                for d in sanitize_untrusted_sync_descriptors(
                    &sc.card.payload.drops,
                    humoco_market::MAX_CARD_DROPS,
                ) {
                    if let Some(sid) = d.slot_id {
                        direct_fetches.push((d.read_url, sid, sc_prefix.clone()));
                    } else {
                        index_urls.push(d.read_url);
                    }
                }
            }
        }
    }

    let mut deduped_urls = Vec::new();
    let mut seen_urls = HashSet::new();
    for u in index_urls {
        let norm = normalize_sync_url(&u);
        if !norm.is_empty() && seen_urls.insert(norm.clone()) {
            deduped_urls.push(norm);
            if deduped_urls.len() >= MAX_POLLED_SYNCS {
                break;
            }
        }
    }

    let mut deduped_direct = Vec::new();
    let mut seen_direct = HashSet::new();
    for (read_url, sid, pfx) in direct_fetches {
        let norm = normalize_sync_url(&read_url);
        if !norm.is_empty() && seen_direct.insert((norm.clone(), sid.clone(), pfx.clone())) {
            deduped_direct.push((norm, sid, pfx));
        }
    }

    (deduped_direct, deduped_urls)
}

struct SlotDecodeContext<'a> {
    my_x25519_sk: &'a x25519_dalek::StaticSecret,
    sync_state: &'a mut StoredSyncState,
    outcome: &'a mut PollOutcome,
    errors: &'a mut super::SyncErrorTracker,
}

pub(crate) fn index_etag_key(read_url: &str) -> String {
    format!("index:{}", normalize_sync_url(read_url))
}

pub(crate) fn direct_card_full_etag_key(read_url: &str, base_slot_pk: &str, pfx: &str) -> String {
    format!(
        "direct_card_full:{}:{}:{}",
        normalize_sync_url(read_url),
        base_slot_pk,
        pfx
    )
}

pub(crate) fn direct_card_delta_etag_key(
    read_url: &str,
    base_slot_pk: &str,
    delta_pfx: &str,
) -> String {
    format!(
        "direct_card_delta:{}:{}:{}",
        normalize_sync_url(read_url),
        base_slot_pk,
        delta_pfx
    )
}

pub(crate) fn direct_wot_etag_key(read_url: &str, base_slot_pk: &str) -> String {
    format!(
        "direct_wot:{}:{}",
        normalize_sync_url(read_url),
        base_slot_pk
    )
}

pub(crate) fn index_card_etag_key(read_url: &str, entry_id: &str, pfx: &str) -> String {
    format!(
        "index_card:{}:{}:{}",
        normalize_sync_url(read_url),
        entry_id,
        pfx
    )
}

pub(crate) fn index_wot_etag_key(read_url: &str, entry_id: &str) -> String {
    format!("index_wot:{}:{}", normalize_sync_url(read_url), entry_id)
}

pub(crate) fn prune_index_slot_etags(
    slot_etags: &mut std::collections::BTreeMap<String, String>,
    read_url: &str,
    entries: &[crate::sync_client::SyncIndexEntry],
) {
    let norm = normalize_sync_url(read_url);
    let present_keys: HashSet<String> = entries
        .iter()
        .filter(|e| e.channel == "card" || e.channel == "wot")
        .map(|e| {
            if e.channel == "wot" {
                index_wot_etag_key(read_url, &e.id)
            } else {
                index_card_etag_key(read_url, &e.id, e.prefix.as_deref().unwrap_or(""))
            }
        })
        .collect();
    let card_prefix = format!("index_card:{}:", norm);
    let wot_prefix = format!("index_wot:{}:", norm);
    slot_etags.retain(|k, _| {
        if k.starts_with(&card_prefix) || k.starts_with(&wot_prefix) {
            present_keys.contains(k)
        } else {
            true
        }
    });
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
            ctx.outcome
                .decoded_card_bundles
                .push((read_url.to_string(), bundle));
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
                    ctx.outcome
                        .decoded_wot_packages
                        .push((read_url.to_string(), pkg));
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
/// Strictly read/decrypt/delta-collecting: never mutates wallet stores during `.await`
/// and aborts early if the active session generation changes.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn poll_sync_channels(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    ui_prefs: &UiPreferences,
    security_policy: &SecurityPolicy,
    id_ctx: &SyncIdentityContext,
    now_secs: u64,
    password: Option<&str>,
    sync_state: &mut StoredSyncState,
    errors: &mut super::SyncErrorTracker,
) -> PollOutcome {
    let mut outcome = PollOutcome {
        polled_syncs: 0,
        decrypted_slots: 0,
        decoded_card_bundles: Vec::new(),
        decoded_wot_packages: Vec::new(),
        sync_health_updates: Vec::new(),
    };

    let Ok(Some((direct_slot_fetches, candidate_read_urls))) =
        state.with_session(session, |service| {
            Ok(collect_poll_targets(
                state,
                service,
                ui_prefs,
                security_policy,
                id_ctx.my_prefix.as_deref(),
                password,
            ))
        })
    else {
        return outcome;
    };

    sync_state.last_poll_at = Some(now_secs);
    let my_x25519_sk = human_money_core::crypto::ed25519_sk_to_x25519_sk(&id_ctx.my_ed_sk);

    let mut decode_ctx = SlotDecodeContext {
        my_x25519_sk: &my_x25519_sk,
        sync_state,
        outcome: &mut outcome,
        errors,
    };

    // 1. Direct O(1) fetches
    for (read_url, slot_id, prefix_opt) in direct_slot_fetches {
        if !state.is_generation_current(session.generation) {
            break;
        }
        if state.is_origin_cooling_down(&read_url, std::time::Instant::now()) {
            decode_ctx.errors.push(format!(
                "Poll of {} deferred: server origin cooling down after HTTP 429",
                read_url
            ));
            continue;
        }
        let (base_slot_pk, slot_prefix) = match humoco_market::parse_slot_id(&slot_id) {
            Ok(parsed) => parsed,
            Err(e) => {
                decode_ctx.errors.push(format!(
                    "Skipping invalid direct slot_id '{}' for {}: {}",
                    slot_id, read_url, e
                ));
                continue;
            }
        };
        decode_ctx.outcome.polled_syncs += 1;
        let card_prefix = slot_prefix.as_deref().or(prefix_opt.as_deref());

        if ui_prefs.market_sync_enabled {
            let base_pfx_str = card_prefix.unwrap_or("");
            let base_etag_key = direct_card_full_etag_key(&read_url, &base_slot_pk, base_pfx_str);
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
                    if !e.is_local_validation() {
                        decode_ctx
                            .outcome
                            .sync_health_updates
                            .push((read_url.clone(), false));
                    }
                    decode_ctx.errors.record_sync_error(
                        state,
                        &read_url,
                        &e,
                        format!(
                            "Failed to get direct card slot {} from {}: {}",
                            slot_id, read_url, e
                        ),
                    );
                }
            }

            let delta_pfx = humoco_market::derive_delta_prefix(card_prefix);
            let delta_etag_key = direct_card_delta_etag_key(&read_url, &base_slot_pk, &delta_pfx);
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
                    decode_ctx.errors.record_sync_error(
                        state,
                        &read_url,
                        &e,
                        format!(
                            "Failed to get direct delta card slot {} from {}: {}",
                            slot_id, read_url, e
                        ),
                    );
                }
            }
        }

        // Fetch WoT slot
        let wot_etag_key = direct_wot_etag_key(&read_url, &base_slot_pk);
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
                if !e.is_local_validation() {
                    decode_ctx
                        .outcome
                        .sync_health_updates
                        .push((read_url.clone(), false));
                }
                decode_ctx.errors.record_sync_error(
                    state,
                    &read_url,
                    &e,
                    format!(
                        "Failed to get direct wot slot {} from {}: {}",
                        slot_id, read_url, e
                    ),
                );
            }
        }
    }

    // 2. Index-based fetches
    for read_url in candidate_read_urls {
        if !state.is_generation_current(session.generation) {
            break;
        }
        if state.is_origin_cooling_down(&read_url, std::time::Instant::now()) {
            decode_ctx.errors.push(format!(
                "Poll of {} deferred: server origin cooling down after HTTP 429",
                read_url
            ));
            continue;
        }
        decode_ctx.outcome.polled_syncs += 1;
        let idx_key = index_etag_key(&read_url);
        let cached_etag = decode_ctx
            .sync_state
            .etags_by_read_url
            .get(&idx_key)
            .map(|s| s.as_str());
        let cached_lm = decode_ctx
            .sync_state
            .last_modified_by_read_url
            .get(&idx_key)
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
                        .insert(idx_key.clone(), et);
                }
                if let Some(lm) = last_modified {
                    decode_ctx
                        .sync_state
                        .last_modified_by_read_url
                        .insert(idx_key, lm);
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

                // Prune ONLY index-listed slot ETags for entries no longer advertised by this read_url index;
                // direct O(1) slot ETags on the same read_url remain intact.
                prune_index_slot_etags(&mut decode_ctx.sync_state.slot_etags, &read_url, &entries);

                for entry in entries {
                    if !state.is_generation_current(session.generation) {
                        break;
                    }
                    if entry.channel != "card" && entry.channel != "wot" {
                        continue;
                    }
                    if entry.channel == "card" && !ui_prefs.market_sync_enabled {
                        continue;
                    }
                    let pfx_opt = if entry.channel == "wot" {
                        None
                    } else {
                        entry.prefix.as_deref()
                    };
                    let etag_key = if entry.channel == "wot" {
                        index_wot_etag_key(&read_url, &entry.id)
                    } else {
                        index_card_etag_key(&read_url, &entry.id, pfx_opt.unwrap_or(""))
                    };
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
                            decode_ctx.errors.record_sync_error(
                                state,
                                &read_url,
                                &e,
                                format!("Failed to get {} slot {}: {}", entry.channel, entry.id, e),
                            );
                        }
                    }
                }
            }
            Err(e) => {
                if !e.is_local_validation() {
                    decode_ctx
                        .outcome
                        .sync_health_updates
                        .push((read_url.clone(), false));
                }
                decode_ctx.errors.record_sync_error(
                    state,
                    &read_url,
                    &e,
                    format!("Failed to list index from {}: {}", read_url, e),
                );
            }
        }
    }

    outcome
}

fn purge_urls_from_sync_state(sync_state: &mut StoredSyncState, candidate_urls: &[String]) {
    let norms: Vec<String> = candidate_urls
        .iter()
        .map(|u| normalize_sync_url(u))
        .filter(|u| !u.is_empty())
        .collect();
    let matches_norm = |url: &str| {
        let nu = normalize_sync_url(url);
        !nu.is_empty() && norms.iter().any(|n| n == &nu)
    };
    let matches_index_key = |key: &str| matches_norm(key.strip_prefix("index:").unwrap_or(key));
    let matches_channel_key =
        |key: &str| key.rsplit_once(':').is_some_and(|(u, _)| matches_norm(u));
    let matches_media_blob = |key: &str| key.split_once("::").is_some_and(|(u, _)| matches_norm(u));
    let matches_rev_key = |key: &str| {
        let url_part = key.split_once("::").map(|(u, _)| u).unwrap_or(key);
        matches_norm(url_part)
    };
    let matches_slot_etag = |key: &str| {
        key.split_once(':').is_some_and(|(_, rest)| {
            norms
                .iter()
                .any(|n| rest == n || rest.starts_with(&format!("{}:", n)))
        })
    };

    sync_state
        .etags_by_read_url
        .retain(|k, _| !matches_index_key(k));
    sync_state
        .last_modified_by_read_url
        .retain(|k, _| !matches_index_key(k));
    sync_state.slot_etags.retain(|k, _| !matches_slot_etag(k));
    sync_state
        .channel_max_bytes_by_write_url
        .retain(|k, _| !matches_channel_key(k));
    sync_state
        .card_base_uploaded_at_by_write_url
        .retain(|k, _| !matches_norm(k));
    sync_state
        .card_base_size_by_write_url
        .retain(|k, _| !matches_norm(k));
    sync_state
        .card_base_versions_by_write_url
        .retain(|k, _| !matches_norm(k));
    sync_state
        .card_delta_active_by_write_url
        .retain(|k, _| !matches_norm(k));
    sync_state
        .card_last_uploaded_versions_by_write_url
        .retain(|k, _| !matches_norm(k));
    sync_state
        .last_revocation_check_at
        .retain(|k, _| !matches_rev_key(k));
    sync_state
        .uploaded_media_blobs
        .retain(|k| !matches_media_blob(k));
    sync_state
        .wot_tombstone_fp_by_write_url
        .retain(|k, _| !matches_norm(k));
}

#[allow(clippy::too_many_arguments)]
async fn probe_public_sync_capabilities(
    probe_client: &reqwest::Client,
    canonical_url: &str,
    access_pubkey: &str,
    my_prefix: Option<&str>,
    state: &AppState,
    session: &SessionToken,
    password: Option<&str>,
) -> Vec<String> {
    let mut authorized_channels = Vec::new();
    let mut channel_max_bytes_updates = Vec::new();
    for ch in ["card", "wot", "backup", "post", "lock"] {
        if !state.is_generation_current(session.generation) {
            return authorized_channels;
        }
        if state.is_origin_cooling_down(canonical_url, std::time::Instant::now()) {
            break;
        }
        let ch_prefix = if ch == "wot" || ch == "lock" {
            None
        } else {
            my_prefix
        };
        match sync_client::head_slot_capability(
            probe_client,
            canonical_url,
            access_pubkey,
            ch_prefix,
            ch,
        )
        .await
        {
            Ok(cap) => {
                if let Some(mb) = cap.max_bytes.filter(|&v| v > 0) {
                    channel_max_bytes_updates.push((canonical_url.to_string(), ch.to_string(), mb));
                }
                if cap.authorized {
                    authorized_channels.push(ch.to_string());
                }
            }
            Err(e) => {
                state.record_origin_cooldown_for_sync_error(canonical_url, &e);
            }
        }
    }
    if state.is_generation_current(session.generation) {
        save_channel_max_bytes_updates(state, session, password, &channel_max_bytes_updates);
    }
    authorized_channels
}

pub async fn probe_public_sync_impl(
    url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPublicSyncInfo, FrontendError> {
    let auth =
        state.authorize_session(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    let pw = auth.pw();
    let canonical_url = humoco_market::canonicalize_write_url(&url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&canonical_url).map_err(FrontendError::from)?;

    let probe_client = sync_client::create_sync_probe_client();
    let info = sync_client::fetch_sync_info(&probe_client, &canonical_url)
        .await
        .map_err(FrontendError::from)?;
    let norm_read_url = normalize_sync_url(&info.read_url);
    sync_client::validate_sync_read_url(&norm_read_url).map_err(|e| {
        FrontendError::from(format!(
            "Sync server returned invalid read_url in ?info: {}",
            e
        ))
    })?;
    let clean_drop_pub = canonical_drop_pub_hex(&info.drop_pub).map_err(|e| {
        FrontendError::from(format!(
            "Sync server returned invalid drop_pub in ?info: {}",
            e.message
        ))
    })?;

    let id_ctx_opt = state.with_session_required(
        &auth.token,
        "Wallet session changed before public sync probe completed",
        |service| {
            if pw.is_some() || service.is_session_active() {
                Ok(Some(SyncIdentityContext::from_service(service, pw)?))
            } else {
                Ok(None)
            }
        },
    )?;

    let (access_pubkey, authorized_channels) = if let Some(id_ctx) = id_ctx_opt {
        let (_r_sk, _r_vk, access_pubkey) =
            humoco_market::derive_public_drop_keypair(&id_ctx.master_seed, &canonical_url)
                .map_err(|e| FrontendError::from(e.to_string()))?;

        let authorized_channels = probe_public_sync_capabilities(
            &probe_client,
            &canonical_url,
            &access_pubkey,
            id_ctx.my_prefix.as_deref(),
            state,
            &auth.token,
            pw,
        )
        .await;
        (access_pubkey, authorized_channels)
    } else {
        (String::new(), Vec::new())
    };

    Ok(FrontendPublicSyncInfo {
        write_url: canonical_url,
        read_url: norm_read_url,
        drop_pub: clean_drop_pub,
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
    let norm_read_url = normalize_sync_url(&read_url);
    sync_client::validate_sync_read_url(&norm_read_url).map_err(FrontendError::from)?;
    let clean_drop_pub = canonical_drop_pub_hex(&drop_pub)?;

    let auth = state.authorize_session(crate::state::RequiredAccess::Full, password.as_deref())?;
    let pw = auth.pw();
    let id_ctx = state.with_session_required(
        &auth.token,
        "Active wallet changed before public sync could be prepared",
        |service| {
            let policy = state
                .get_or_load_security_policy(service, pw)
                .map_err(FrontendError::from)?;
            if let Some(existing) = policy
                .public_syncs
                .iter()
                .find(|d| normalize_sync_url(&d.write_url) == canonical_url)
            {
                let pinned = existing.drop_pub.trim();
                if !pinned.is_empty() && !drop_pub_matches(pinned, &clean_drop_pub) {
                    return Err(FrontendError::from(format!(
                        "Public sync key pin mismatch for {}: existing {}, got {}",
                        canonical_url, pinned, clean_drop_pub
                    )));
                }
            }
            SyncIdentityContext::from_service(service, pw)
        },
    )?;

    let (_r_sk, _r_vk, access_pubkey) =
        humoco_market::derive_public_drop_keypair(&id_ctx.master_seed, &canonical_url)
            .map_err(|e| FrontendError::from(e.to_string()))?;

    let probe_client = sync_client::create_sync_probe_client();
    let info_opt = sync_client::fetch_sync_info(&probe_client, &canonical_url)
        .await
        .ok();
    if let Some(ref info) = info_opt {
        if !info.drop_pub.trim().is_empty() {
            let live_drop_pub = canonical_drop_pub_hex(&info.drop_pub).map_err(|e| {
                FrontendError::from(format!(
                    "Public sync server at {} returned invalid drop_pub in ?info: {}",
                    canonical_url, e.message
                ))
            })?;
            if live_drop_pub != clean_drop_pub {
                return Err(FrontendError::from(format!(
                    "Public sync server key mismatch at {}: expected {}, got {}",
                    canonical_url, clean_drop_pub, live_drop_pub
                )));
            }
        }
    }
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
        &auth.token,
        pw,
    )
    .await;

    let final_enabled = {
        // Serialize SecurityPolicy + OwnMarketProfile RMW under market_op_lock, and drop it before
        // enqueue_guardian_locator_updates_if_configured (which acquires recovery_op_lock -> post_op_lock).
        let _market_guard = state.market_op_lock.lock().await;
        state.with_session_required(
            &auth.token,
            "Active wallet changed before public sync could be saved",
            |service| {
                let now_secs = sync_client::current_unix_timestamp();
                let mut stored = StoredPublicSync {
                    write_url: canonical_url.clone(),
                    read_url: norm_read_url.clone(),
                    drop_pub: clean_drop_pub.clone(),
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
                        if sync_client::verify_revocation_notice(notice, &clean_drop_pub).is_ok() {
                            apply_revocation_notice(&mut stored, notice, now_secs);
                        }
                    }
                }
                if is_endpoint_revoked(&stored.status) {
                    stored.enabled = false;
                }

                let mut policy = state
                    .get_or_load_security_policy(service, pw)
                    .map_err(FrontendError::from)?;
                if let Some(pos) = policy
                    .public_syncs
                    .iter()
                    .position(|d| normalize_sync_url(&d.write_url) == canonical_url)
                {
                    let existing = &mut policy.public_syncs[pos];
                    let pinned = existing.drop_pub.trim();
                    if !pinned.is_empty() && !drop_pub_matches(pinned, &clean_drop_pub) {
                        return Err(FrontendError::from(format!(
                            "Public sync key pin mismatch for {}: existing {}, got {}",
                            canonical_url, pinned, clean_drop_pub
                        )));
                    }
                    if crate::state::is_revocation_effective(existing, now_secs) {
                        stored.status = "revoked".to_string();
                        stored.enabled = false;
                        stored.sunset_at = existing.sunset_at;
                        stored.revocation_reason = existing.revocation_reason.clone();
                        stored.replacement = existing.replacement.clone();
                    } else if existing.status.eq_ignore_ascii_case("deprecating")
                        && info_opt.is_none()
                    {
                        stored.status = existing.status.clone();
                        stored.sunset_at = existing.sunset_at;
                        stored.revocation_reason = existing.revocation_reason.clone();
                        stored.replacement = existing.replacement.clone();
                    }
                    policy.public_syncs[pos] = stored;
                } else {
                    policy.public_syncs.push(stored);
                }

                let stored_enabled = policy
                    .public_syncs
                    .iter()
                    .find(|d| normalize_sync_url(&d.write_url) == canonical_url)
                    .is_some_and(|d| d.enabled);

                let updated_profile = super::admin::build_resigned_own_card_profile_for_policy(
                    state, service, &policy, pw,
                );

                let mut batch = EncryptedBatchCommit::new();
                batch.security_policy(policy);
                if let Some(prof) = updated_profile {
                    batch.own_market_profile(prof);
                }
                batch
                    .commit(state, service, pw)
                    .map_err(FrontendError::from)?;
                Ok(stored_enabled)
            },
        )?
    };

    if final_enabled
        && info_opt.as_ref().is_some_and(|i| i.lock.is_some())
        && state.is_generation_current(auth.generation())
    {
        let lock_probe =
            sync_client::probe_lock_server(&probe_client, &canonical_url, Some(&clean_drop_pub))
                .await;
        if !lock_probe.lock_pub.trim().is_empty() && state.is_generation_current(auth.generation())
        {
            let _ = state.mutate_internal_lifecycle_for_session(&auth.token, pw, |lc| {
                crate::services::common::upsert_verified_lock_probe(
                    &mut lc.l2_probe_cache,
                    lock_probe.clone(),
                )
            });
        }
    }

    if state.is_generation_current(auth.generation()) {
        let _ = enqueue_guardian_locator_updates_if_configured(state, &auth.token, pw).await;
    }

    Ok(FrontendPublicSyncInfo {
        write_url: canonical_url,
        read_url: norm_read_url,
        drop_pub: clean_drop_pub,
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
    let auth = state.authorize_session(crate::state::RequiredAccess::Full, password.as_deref())?;
    let pw = auth.pw();
    let canonical_url = humoco_market::canonicalize_write_url(&write_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    if delete_remote_slots {
        let id_ctx = state.with_session_required(
            &auth.token,
            "Active wallet changed before public sync deletion could be prepared",
            |service| SyncIdentityContext::from_service(service, pw),
        )?;
        let (r_sk, _, _) =
            humoco_market::derive_public_drop_keypair(&id_ctx.master_seed, &canonical_url)
                .map_err(|e| FrontendError::from(e.to_string()))?;
        let client = sync_client::create_sync_http_client();
        let now = sync_client::current_unix_timestamp();
        let acct_nonce = sync_client::generate_sync_nonce();
        let acct_signed = humoco_market::sign_delete_account_request(&r_sk, now, &acct_nonce);
        sync_client::delete_account(&client, &canonical_url, &acct_signed)
            .await
            .map_err(|err| {
                let mut fe = FrontendError::with_code(
                    "error.sync.remoteDeleteFailed",
                    format!(
                        "Remote sync deletion failed; local sync settings were preserved: {}",
                        err
                    ),
                );
                fe.details.insert("error".to_string(), err.to_string());
                fe
            })?;
    }

    {
        let _backup_guard = state.backup_op_lock.lock().await;
        let _post_guard = state.post_op_lock.lock().await;
        let _market_guard = state.market_op_lock.lock().await;

        state.with_session_required(
            &auth.token,
            "Active wallet changed before public sync could be removed",
            |service| {
                let mut removed_urls = vec![canonical_url.clone(), normalize_sync_url(&write_url)];
                let mut policy = state
                    .get_or_load_security_policy(service, pw)
                    .map_err(FrontendError::from)?;
                for d in &policy.public_syncs {
                    if normalize_sync_url(&d.write_url) == canonical_url && !d.read_url.is_empty() {
                        removed_urls.push(normalize_sync_url(&d.read_url));
                    }
                }
                policy
                    .public_syncs
                    .retain(|d| normalize_sync_url(&d.write_url) != canonical_url);
                let removed_norms: HashSet<String> = removed_urls
                    .iter()
                    .map(|u| normalize_sync_url(u))
                    .filter(|u| !u.is_empty())
                    .collect();

                let updated_profile = super::admin::build_resigned_own_card_profile_for_policy(
                    state, service, &policy, pw,
                );

                let updated_reg = if let Ok(mut reg) = state.get_or_load_wot_register(service, pw) {
                    reg.clear_drop_sent_log(Some(&canonical_url));
                    if normalize_sync_url(&write_url) != canonical_url {
                        reg.clear_drop_sent_log(Some(&write_url));
                    }
                    Some(reg)
                } else {
                    None
                };

                let updated_sync_state = crate::state::load_encrypted_json_opt::<StoredSyncState>(
                    service,
                    MARKET_SYNC_KEY,
                    pw,
                )
                .map_err(FrontendError::from)?
                .map(|mut sync_state| {
                    purge_urls_from_sync_state(&mut sync_state, &removed_urls);
                    sync_state
                });

                let updated_post_state = crate::state::load_encrypted_json_opt::<StoredPostState>(
                    service,
                    MARKET_POST_STATE_KEY,
                    pw,
                )
                .map_err(FrontendError::from)?
                .map(|mut post_state| {
                    post_state
                        .slot_etags_by_write_url
                        .retain(|k, _| !removed_norms.contains(&normalize_sync_url(k)));
                    post_state
                });

                let mut batch = EncryptedBatchCommit::new();
                batch.security_policy(policy);
                if let Some(prof) = updated_profile {
                    batch.own_market_profile(prof);
                }
                if let Some(reg) = updated_reg {
                    batch.wot(reg);
                }
                if let Some(sync_state) = updated_sync_state {
                    batch.sync_state(sync_state);
                }
                if let Some(post_state) = updated_post_state {
                    batch.post_state(post_state);
                }
                batch
                    .commit(state, service, pw)
                    .map_err(FrontendError::from)?;

                if let Some(mut backup_state) = crate::state::load_local_sidecar_json_opt::<
                    StoredBackupSyncState,
                >(service, BACKUP_SYNC_STATE_KEY, pw)
                .map_err(FrontendError::from)?
                {
                    let prev_len = backup_state.mirrors_by_write_url.len();
                    backup_state
                        .mirrors_by_write_url
                        .retain(|k, _| !removed_norms.contains(&normalize_sync_url(k)));
                    if backup_state.mirrors_by_write_url.len() != prev_len {
                        crate::state::save_local_sidecar_json(
                            service,
                            BACKUP_SYNC_STATE_KEY,
                            &backup_state,
                            pw,
                        )
                        .map_err(FrontendError::from)?;
                    }
                }
                Ok(())
            },
        )?;
    }

    if state.is_generation_current(auth.generation()) {
        let _ = enqueue_guardian_locator_updates_if_configured(state, &auth.token, pw).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn h6_prune_index_slot_etags_preserves_direct_card_and_wot_etags() {
        let mut etags = BTreeMap::new();
        let url1 = "https://sync1.example.org/sync.php";
        let url2 = "https://sync2.example.org/sync.php";

        let keep_card = index_card_etag_key(url1, "slot-1", "p1");
        let stale_card = index_card_etag_key(url1, "slot-stale", "p2");
        let keep_wot = index_wot_etag_key(url1, "wot-1");
        let stale_wot = index_wot_etag_key(url1, "wot-stale");
        let other_server_card = index_card_etag_key(url2, "slot-9", "p9");
        let direct_full = direct_card_full_etag_key(url1, "pk1", "pfx1");
        let direct_delta = direct_card_delta_etag_key(url1, "pk1", "dpfx1");
        let direct_wot = direct_wot_etag_key(url1, "hash1");

        for key in [
            &keep_card,
            &stale_card,
            &keep_wot,
            &stale_wot,
            &other_server_card,
            &direct_full,
            &direct_delta,
            &direct_wot,
        ] {
            etags.insert(key.clone(), "etag-val".to_string());
        }

        let entries = vec![
            crate::sync_client::SyncIndexEntry {
                id: "slot-1".to_string(),
                channel: "card".to_string(),
                prefix: Some("p1".to_string()),
                mtime: 100,
                size: 128,
                etag: None,
            },
            crate::sync_client::SyncIndexEntry {
                id: "wot-1".to_string(),
                channel: "wot".to_string(),
                prefix: None,
                mtime: 100,
                size: 64,
                etag: None,
            },
        ];

        prune_index_slot_etags(&mut etags, url1, &entries);

        assert!(etags.contains_key(&keep_card));
        assert!(!etags.contains_key(&stale_card));
        assert!(etags.contains_key(&keep_wot));
        assert!(!etags.contains_key(&stale_wot));
        assert!(etags.contains_key(&other_server_card));
        assert!(etags.contains_key(&direct_full));
        assert!(etags.contains_key(&direct_delta));
        assert!(etags.contains_key(&direct_wot));
    }

    #[tokio::test]
    async fn test_h13_remove_public_sync_preserves_local_state_on_remote_delete_failure() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let dir = crate::test_support::temp_test_dir("h13_remote_delete_fail");
        let pw = "StrongPassword123!";
        let svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let state = crate::test_support::create_test_state(svc);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let srv = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 4\r\n\r\nfail";
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });

        let write_url = format!("http://127.0.0.1:{}/sync.php", port);
        {
            let mut s = state.lock_service().unwrap();
            state
                .mutate_security_policy(&mut s, Some(pw), |p| {
                    p.public_syncs.push(crate::settings::StoredPublicSync {
                        write_url: write_url.clone(),
                        read_url: format!("http://127.0.0.1:{}/d", port),
                        drop_pub: crate::test_support::test_ed25519_pub_hex(0x11),
                        register_url: None,
                        access_pubkey: crate::test_support::test_ed25519_pub_hex(0x22),
                        enabled: true,
                        authorized_channels: vec!["card".into()],
                        status: "active".into(),
                        sunset_at: None,
                        revocation_reason: None,
                        replacement: None,
                    });
                })
                .unwrap();
        }

        let err = remove_public_sync_impl(write_url.clone(), true, Some(pw.into()), &state)
            .await
            .unwrap_err();
        assert_eq!(err.code, "error.sync.remoteDeleteFailed");

        let remaining = {
            let mut s = state.lock_service().unwrap();
            state.get_or_load_security_policy(&mut s, Some(pw)).unwrap()
        };
        assert_eq!(remaining.public_syncs.len(), 1);
        assert_eq!(remaining.public_syncs[0].write_url, write_url);

        let _ = srv.await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn s09_f3_f4_f5_purge_urls_and_exclude_revoked_targets() {
        let mut sync_state = StoredSyncState::default();
        let target_read = "https://sync1.example:8443/d";
        let target_write = "https://sync1.example:8443/sync.php";
        let other_read = "https://other.example/d";
        let other_write = "https://other.example/sync.php";

        sync_state
            .etags_by_read_url
            .insert(index_etag_key(target_read), "etag-1".to_string());
        sync_state
            .etags_by_read_url
            .insert(index_etag_key(other_read), "etag-2".to_string());
        sync_state
            .last_modified_by_read_url
            .insert(index_etag_key(target_read), "Mon, 01 Jan 2026".to_string());
        sync_state
            .last_modified_by_read_url
            .insert(index_etag_key(other_read), "Tue, 02 Jan 2026".to_string());
        sync_state
            .channel_max_bytes_by_write_url
            .insert(channel_max_bytes_key(target_write, "card"), 1024);
        sync_state
            .channel_max_bytes_by_write_url
            .insert(channel_max_bytes_key(other_write, "card"), 2048);
        sync_state.last_revocation_check_at.insert(
            format!(
                "{}::{}",
                normalize_sync_url(target_read),
                crate::test_support::test_ed25519_pub_hex(0x11)
            ),
            1000,
        );
        sync_state.last_revocation_check_at.insert(
            format!(
                "{}::{}",
                normalize_sync_url(other_read),
                crate::test_support::test_ed25519_pub_hex(0x22)
            ),
            2000,
        );
        sync_state
            .uploaded_media_blobs
            .insert(format!("{}/::blob1", normalize_sync_url(target_write)));

        purge_urls_from_sync_state(
            &mut sync_state,
            &[format!("{}/", target_read), target_write.to_string()],
        );

        assert!(!sync_state
            .etags_by_read_url
            .contains_key(&index_etag_key(target_read)));
        assert!(sync_state
            .etags_by_read_url
            .contains_key(&index_etag_key(other_read)));
        assert!(!sync_state
            .last_modified_by_read_url
            .contains_key(&index_etag_key(target_read)));
        assert!(sync_state
            .last_modified_by_read_url
            .contains_key(&index_etag_key(other_read)));
        assert!(!sync_state
            .channel_max_bytes_by_write_url
            .contains_key(&channel_max_bytes_key(target_write, "card")));
        assert!(sync_state
            .channel_max_bytes_by_write_url
            .contains_key(&channel_max_bytes_key(other_write, "card")));
        assert_eq!(sync_state.last_revocation_check_at.len(), 1);
        assert!(sync_state
            .last_revocation_check_at
            .keys()
            .all(|k| k.starts_with(other_read)));
        assert!(sync_state.uploaded_media_blobs.is_empty());

        let policy = SecurityPolicy {
            market_selected_syncs: vec![
                FrontendSyncDescriptor {
                    write_url: target_write.to_string(),
                    read_url: target_read.to_string(),
                    drop_pub: crate::test_support::test_ed25519_pub_hex(0x11),
                    status: "active".to_string(),
                    ..Default::default()
                },
                FrontendSyncDescriptor {
                    write_url: "https://revoked.example/sync.php".to_string(),
                    read_url: "https://revoked.example/d".to_string(),
                    drop_pub: crate::test_support::test_ed25519_pub_hex(0x22),
                    status: "revoked".to_string(),
                    ..Default::default()
                },
            ],
            public_syncs: vec![
                StoredPublicSync {
                    write_url: other_write.to_string(),
                    read_url: other_read.to_string(),
                    drop_pub: crate::test_support::test_ed25519_pub_hex(0x33),
                    access_pubkey: crate::test_support::test_ed25519_pub_hex(0x44),
                    enabled: true,
                    authorized_channels: vec!["card".to_string()],
                    status: "active".to_string(),
                    ..Default::default()
                },
                StoredPublicSync {
                    write_url: "https://pub-rev.example/sync.php".to_string(),
                    read_url: "https://pub-rev.example/d".to_string(),
                    drop_pub: crate::test_support::test_ed25519_pub_hex(0x55),
                    access_pubkey: crate::test_support::test_ed25519_pub_hex(0x66),
                    enabled: true,
                    authorized_channels: vec!["card".to_string()],
                    status: "revoked".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let targets = collect_upload_targets_for_channel(&policy, "card");
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].write_url, target_write);
        assert_eq!(targets[1].write_url, other_write);
    }

    #[tokio::test]
    async fn test_s09_1_save_public_sync_preserves_sticky_revocation() {
        use ed25519_dalek::SigningKey;
        use std::sync::{Arc, Mutex};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let dir = crate::test_support::temp_test_dir("s09_1_sticky_rev");
        let pw = "StrongPassword123!";
        let svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let state = crate::test_support::create_test_state(svc);

        let admin_sk = SigningKey::from_bytes(&[0x31u8; 32]);
        let drop_pub = hex::encode(admin_sk.verifying_key().as_bytes());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let write_url = format!("http://127.0.0.1:{}/sync.php", port);
        let read_url = format!("http://127.0.0.1:{}/d", port);

        let mode = Arc::new(Mutex::new("omit_revocation"));
        let mode_srv = Arc::clone(&mode);
        let drop_pub_srv = drop_pub.clone();
        let read_url_srv = read_url.clone();
        let admin_sk_srv = admin_sk.clone();

        let srv = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mode_req = Arc::clone(&mode_srv);
                let dp = drop_pub_srv.clone();
                let ru = read_url_srv.clone();
                let sk = admin_sk_srv.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf).await;
                    let req = String::from_utf8_lossy(&buf);
                    let first_line = req.lines().next().unwrap_or("");
                    let cur_mode = *mode_req.lock().unwrap();

                    if first_line.starts_with("GET ") && first_line.contains("?info") {
                        if cur_mode == "fail_info" {
                            let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 4\r\n\r\nfail";
                            let _ = stream.write_all(resp.as_bytes()).await;
                            return;
                        }
                        let revocation_json = if cur_mode == "postpone_future" {
                            let now = sync_client::current_unix_timestamp();
                            let notice = sync_client::sign_revocation_notice(
                                &sk,
                                now.max(1),
                                now + 86_400 * 30,
                                Some("Postponed".to_string()),
                                None,
                            )
                            .unwrap();
                            serde_json::to_value(notice).unwrap()
                        } else {
                            serde_json::Value::Null
                        };
                        let body = serde_json::json!({
                            "drop_pub": dp,
                            "read_url": ru,
                            "channels": ["card", "wot", "backup", "post"],
                            "revocation": revocation_json,
                        })
                        .to_string();
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(resp.as_bytes()).await;
                    } else {
                        let resp = "HTTP/1.1 200 OK\r\nX-Drop-Authorized: true\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes()).await;
                    }
                });
            }
        });

        let replacement = crate::models::FrontendSyncReplacementTarget {
            write_url: "https://new-sync.example.org/sync.php".to_string(),
            read_url: "https://new-sync.example.org/d".to_string(),
            drop_pub: crate::test_support::test_ed25519_pub_hex(0x44),
        };
        {
            let mut s = state.lock_service().unwrap();
            state
                .mutate_security_policy(&mut s, Some(pw), |p| {
                    p.public_syncs.push(StoredPublicSync {
                        write_url: write_url.clone(),
                        read_url: read_url.clone(),
                        drop_pub: drop_pub.clone(),
                        register_url: None,
                        access_pubkey: crate::test_support::test_ed25519_pub_hex(0x22),
                        enabled: false,
                        authorized_channels: vec!["card".into()],
                        status: "revoked".into(),
                        sunset_at: Some(1_000_000),
                        revocation_reason: Some("Compromised".into()),
                        replacement: Some(replacement.clone()),
                    });
                })
                .unwrap();
        }

        // Case 1: Compromised server omits ?info.revocation while user toggles/saves public sync
        save_public_sync_impl(
            write_url.clone(),
            read_url.clone(),
            drop_pub.clone(),
            None,
            true,
            Some(pw.into()),
            &state,
        )
        .await
        .unwrap();

        {
            let mut s = state.lock_service().unwrap();
            let pol = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
            let ps = &pol.public_syncs[0];
            assert_eq!(ps.status, "revoked");
            assert!(!ps.enabled);
            assert_eq!(ps.sunset_at, Some(1_000_000));
            assert_eq!(ps.revocation_reason.as_deref(), Some("Compromised"));
            assert_eq!(ps.replacement.as_ref(), Some(&replacement));
        }

        // Case 2: Compromised server tries to postpone effective_at into the future
        *mode.lock().unwrap() = "postpone_future";
        save_public_sync_impl(
            write_url.clone(),
            read_url.clone(),
            drop_pub.clone(),
            None,
            true,
            Some(pw.into()),
            &state,
        )
        .await
        .unwrap();

        {
            let mut s = state.lock_service().unwrap();
            let pol = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
            let ps = &pol.public_syncs[0];
            assert_eq!(ps.status, "revoked");
            assert!(!ps.enabled);
            assert_eq!(ps.sunset_at, Some(1_000_000));
            assert_eq!(ps.revocation_reason.as_deref(), Some("Compromised"));
        }

        // Case 3: Existing entry was "deprecating" with already-elapsed sunset_at and ?info fails
        *mode.lock().unwrap() = "fail_info";
        {
            let mut s = state.lock_service().unwrap();
            state
                .mutate_security_policy(&mut s, Some(pw), |p| {
                    p.public_syncs[0].status = "deprecating".into();
                    p.public_syncs[0].sunset_at = Some(1_000_000);
                    p.public_syncs[0].enabled = true;
                })
                .unwrap();
        }
        save_public_sync_impl(
            write_url,
            read_url,
            drop_pub,
            None,
            true,
            Some(pw.into()),
            &state,
        )
        .await
        .unwrap();

        {
            let mut s = state.lock_service().unwrap();
            let pol = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
            let ps = &pol.public_syncs[0];
            assert_eq!(ps.status, "revoked");
            assert!(!ps.enabled);
            assert_eq!(ps.sunset_at, Some(1_000_000));
        }

        srv.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_s09_1_save_public_sync_preserves_deprecating_on_transient_info_failure() {
        use ed25519_dalek::SigningKey;
        use std::sync::{Arc, Mutex};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let dir = crate::test_support::temp_test_dir("s09_1_deprecating");
        let pw = "StrongPassword123!";
        let svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let state = crate::test_support::create_test_state(svc);

        let admin_sk = SigningKey::from_bytes(&[0x32u8; 32]);
        let drop_pub = hex::encode(admin_sk.verifying_key().as_bytes());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let write_url = format!("http://127.0.0.1:{}/sync.php", port);
        let read_url = format!("http://127.0.0.1:{}/d", port);

        let fail_info = Arc::new(Mutex::new(true));
        let fail_info_srv = Arc::clone(&fail_info);
        let drop_pub_srv = drop_pub.clone();
        let read_url_srv = read_url.clone();

        let srv = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let fi = Arc::clone(&fail_info_srv);
                let dp = drop_pub_srv.clone();
                let ru = read_url_srv.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf).await;
                    let req = String::from_utf8_lossy(&buf);
                    let first_line = req.lines().next().unwrap_or("");
                    if first_line.starts_with("GET ") && first_line.contains("?info") {
                        if *fi.lock().unwrap() {
                            let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 4\r\n\r\nfail";
                            let _ = stream.write_all(resp.as_bytes()).await;
                        } else {
                            let body = serde_json::json!({
                                "drop_pub": dp,
                                "read_url": ru,
                                "channels": ["card", "wot", "backup", "post"],
                                "revocation": null,
                            })
                            .to_string();
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(resp.as_bytes()).await;
                        }
                    } else {
                        let resp = "HTTP/1.1 200 OK\r\nX-Drop-Authorized: true\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes()).await;
                    }
                });
            }
        });

        let future_sunset = sync_client::current_unix_timestamp() + 86_400 * 14;
        let replacement = crate::models::FrontendSyncReplacementTarget {
            write_url: "https://next-sync.example.org/sync.php".to_string(),
            read_url: "https://next-sync.example.org/d".to_string(),
            drop_pub: crate::test_support::test_ed25519_pub_hex(0x45),
        };
        {
            let mut s = state.lock_service().unwrap();
            state
                .mutate_security_policy(&mut s, Some(pw), |p| {
                    p.public_syncs.push(StoredPublicSync {
                        write_url: write_url.clone(),
                        read_url: read_url.clone(),
                        drop_pub: drop_pub.clone(),
                        register_url: None,
                        access_pubkey: crate::test_support::test_ed25519_pub_hex(0x23),
                        enabled: true,
                        authorized_channels: vec!["card".into()],
                        status: "deprecating".into(),
                        sunset_at: Some(future_sunset),
                        revocation_reason: Some("Planned move".into()),
                        replacement: Some(replacement.clone()),
                    });
                })
                .unwrap();
        }

        // Transient ?info failure must preserve active deprecation state
        save_public_sync_impl(
            write_url.clone(),
            read_url.clone(),
            drop_pub.clone(),
            None,
            true,
            Some(pw.into()),
            &state,
        )
        .await
        .unwrap();

        {
            let mut s = state.lock_service().unwrap();
            let pol = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
            let ps = &pol.public_syncs[0];
            assert_eq!(ps.status, "deprecating");
            assert_eq!(ps.sunset_at, Some(future_sunset));
            assert_eq!(ps.revocation_reason.as_deref(), Some("Planned move"));
            assert_eq!(ps.replacement.as_ref(), Some(&replacement));
        }

        // Explicit ?info without revocation before sunset_at clears deprecation
        *fail_info.lock().unwrap() = false;
        save_public_sync_impl(
            write_url,
            read_url,
            drop_pub,
            None,
            true,
            Some(pw.into()),
            &state,
        )
        .await
        .unwrap();

        {
            let mut s = state.lock_service().unwrap();
            let pol = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
            let ps = &pol.public_syncs[0];
            assert_eq!(ps.status, "active");
            assert_eq!(ps.sunset_at, None);
            assert_eq!(ps.revocation_reason, None);
            assert_eq!(ps.replacement, None);
        }

        srv.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
