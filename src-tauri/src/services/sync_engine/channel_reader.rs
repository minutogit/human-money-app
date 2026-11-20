use std::collections::HashSet;

use crate::models::{
    FrontendAddressBook, FrontendError, FrontendPublicSyncInfo, FrontendSyncDescriptor,
};
use crate::services::common::{normalize_sync_url, DropChannelSigner, SyncIdentityContext};
use crate::services::market_service::sync_own_card_drops_on_settings_change;
use crate::services::recovery_service::enqueue_guardian_locator_updates_if_configured;
use crate::session::SessionToken;
use crate::settings::{SecurityPolicy, StoredPublicSync, UiPreferences, MAX_POLLED_SYNCS};
use crate::state::{
    apply_revocation_notice, apply_revocation_notice_to_address_book, canonicalize_did,
    clear_revocation, AppState, StoredBackupSyncState, StoredPostState, StoredSyncState,
    BACKUP_SYNC_STATE_KEY, CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY, MARKET_POST_STATE_KEY,
    MARKET_SYNC_KEY,
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
            if let Err(e) =
                crate::state::save_encrypted_json(service, MARKET_SYNC_KEY, &sync_state, password)
            {
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
        .cloned()
        .map(Into::into)
        .collect();

    for pd in &security_policy.public_syncs {
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

    for (rev_read_url, rev_drop_pub) in revocation_candidates {
        if !state.is_generation_current(session.generation) {
            break;
        }
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
                let committed = state.with_session(session, |service| {
                    if let Ok(mut fresh_policy) =
                        state.get_or_load_security_policy(service, password)
                    {
                        let mut policy_changed = false;
                        for d in &mut fresh_policy.market_selected_syncs {
                            if normalize_sync_url(&d.read_url) == norm_target
                                && d.drop_pub.trim().eq_ignore_ascii_case(&notice.drop_pub)
                            {
                                apply_revocation_notice(d, &notice, now_secs);
                                revoked_own_syncs.push(d.clone());
                                policy_changed = true;
                            }
                        }
                        for ps in &mut fresh_policy.public_syncs {
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
                                policy_changed = true;
                            }
                        }
                        if policy_changed
                            && state
                                .save_security_policy(service, fresh_policy.clone(), password)
                                .is_ok()
                        {
                            *security_policy = fresh_policy;
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
                                &rev_read_url,
                                &notice,
                                now_secs,
                            ) {
                                if let Err(e) = crate::state::save_encrypted_json(
                                    service,
                                    CONTACTS_DATA_NAME,
                                    &ab,
                                    password,
                                ) {
                                    log::warn!(
                                        "Failed to save address book after revocation notice: {}",
                                        e
                                    );
                                } else {
                                    state.set_cached_contacts_for_service(service, Some(ab));
                                }
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
                    Ok(())
                });
                if !matches!(committed, Ok(Some(()))) {
                    break;
                }
            }
            Ok(None) => {
                // Server returned 404 (no revocation / revocation cleared): clear stale local state.
                let norm_target = normalize_sync_url(&rev_read_url);
                let committed = state.with_session(session, |service| {
                    if let Ok(mut fresh_policy) =
                        state.get_or_load_security_policy(service, password)
                    {
                        let mut policy_changed = false;
                        for d in &mut fresh_policy.market_selected_syncs {
                            if normalize_sync_url(&d.read_url) == norm_target
                                && d.drop_pub.trim().eq_ignore_ascii_case(&rev_drop_pub)
                                && clear_revocation(d, "active")
                            {
                                policy_changed = true;
                            }
                        }
                        for ps in &mut fresh_policy.public_syncs {
                            if normalize_sync_url(&ps.read_url) == norm_target
                                && ps.drop_pub.trim().eq_ignore_ascii_case(&rev_drop_pub)
                            {
                                let fallback = if ps.authorized_channels.is_empty() {
                                    "pending_auth"
                                } else {
                                    "active"
                                };
                                if clear_revocation(ps, fallback) {
                                    policy_changed = true;
                                }
                            }
                        }
                        if policy_changed
                            && state
                                .save_security_policy(service, fresh_policy.clone(), password)
                                .is_ok()
                        {
                            *security_policy = fresh_policy;
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
                                        && ep.drop_pub.trim().eq_ignore_ascii_case(&rev_drop_pub)
                                        && clear_revocation(ep, "unverified")
                                    {
                                        ab_changed = true;
                                    }
                                }
                            }
                            if ab_changed {
                                if let Err(e) = crate::state::save_encrypted_json(
                                    service,
                                    CONTACTS_DATA_NAME,
                                    &ab,
                                    password,
                                ) {
                                    log::warn!(
                                        "Failed to save address book after clearing revocation: {}",
                                        e
                                    );
                                } else {
                                    state.set_cached_contacts_for_service(service, Some(ab));
                                }
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
                    Ok(())
                });
                if !matches!(committed, Ok(Some(()))) {
                    break;
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
                    if !d.read_url.is_empty() {
                        if let Some(ref sid) = d.slot_id {
                            direct_fetches.push((
                                d.read_url.clone(),
                                sid.clone(),
                                c_prefix.map(str::to_string),
                            ));
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
            if lvl >= ui_prefs.share_min_level {
                let sc_prefix =
                    human_money_core::crypto::get_prefix_from_user_id(&sc.card.payload.did)
                        .map(str::to_string);
                for d in &sc.card.payload.drops {
                    if !d.read_url.is_empty() {
                        if let Some(ref sid) = d.slot_id {
                            direct_fetches.push((
                                d.read_url.clone(),
                                sid.clone(),
                                sc_prefix.clone(),
                            ));
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
    my_x25519_sk: &'a x25519_dalek::StaticSecret,
    sync_state: &'a mut StoredSyncState,
    outcome: &'a mut PollOutcome,
    errors: &'a mut Vec<String>,
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
    errors: &mut Vec<String>,
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
        decode_ctx.outcome.polled_syncs += 1;
        let (base_slot_pk, slot_prefix) =
            humoco_market::parse_slot_id(&slot_id).unwrap_or_else(|_| (slot_id.clone(), None));
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
                    decode_ctx.errors.push(format!(
                        "Failed to get direct delta card slot {} from {}: {}",
                        slot_id, read_url, e
                    ));
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
        if !state.is_generation_current(session.generation) {
            break;
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
    let matches_channel_key = |key: &str| norms.iter().any(|n| key.starts_with(&format!("{}:", n)));
    let matches_media_blob = |key: &str| key.split_once("::").is_some_and(|(u, _)| matches_norm(u));
    let matches_slot_etag = |key: &str| {
        key.split_once(':').is_some_and(|(_, rest)| {
            norms
                .iter()
                .any(|n| rest == n || rest.starts_with(&format!("{}:", n)))
        })
    };

    sync_state.etags_by_read_url.retain(|k, _| !matches_norm(k));
    sync_state
        .last_modified_by_read_url
        .retain(|k, _| !matches_norm(k));
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
        .retain(|k, _| !matches_norm(k));
    sync_state
        .uploaded_media_blobs
        .retain(|k| !matches_media_blob(k));
    sync_state
        .wot_tombstone_fp_by_write_url
        .retain(|k, _| !matches_norm(k));
}

#[cfg(test)]
pub async fn delete_own_sync_slots_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<usize, FrontendError> {
    let (session, id_ctx, drops) = {
        let mut service =
            state.authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;
        let session = state.session_token_for_service(&service)?;
        let id_ctx = SyncIdentityContext::from_service(&service, password.as_deref())?;
        let security_policy = state
            .get_or_load_security_policy(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        (session, id_ctx, security_policy.market_selected_syncs)
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
    let own_urls: Vec<String> = drops
        .iter()
        .flat_map(|d| [d.write_url.clone(), d.read_url.clone()])
        .filter(|u| !u.is_empty())
        .collect();
    let _backup_guard = state.backup_op_lock.lock().await;
    state.with_session_required(
        &session,
        "Active wallet changed before sync state could be reset",
        |service| {
            if let Ok(mut reg) = state.get_or_load_wot_register(service, password.as_deref()) {
                reg.clear_drop_sent_log(None);
                let _ = state.save_wot_register(service, &reg, password.as_deref());
            }

            if let Some(mut sync_state) = crate::state::load_encrypted_json_opt::<StoredSyncState>(
                service,
                MARKET_SYNC_KEY,
                password.as_deref(),
            )
            .map_err(FrontendError::from)?
            {
                sync_state.last_upload_at = None;
                sync_state.last_poll_at = None;
                sync_state.last_readers_hash = None;
                purge_urls_from_sync_state(&mut sync_state, &own_urls);
                crate::state::save_encrypted_json(
                    service,
                    MARKET_SYNC_KEY,
                    &sync_state,
                    password.as_deref(),
                )
                .map_err(FrontendError::from)?;
            }

            if let Some(mut post_state) = crate::state::load_encrypted_json_opt::<StoredPostState>(
                service,
                MARKET_POST_STATE_KEY,
                password.as_deref(),
            )
            .map_err(FrontendError::from)?
            {
                post_state.slot_etags_by_write_url.clear();
                crate::state::save_encrypted_json(
                    service,
                    MARKET_POST_STATE_KEY,
                    &post_state,
                    password.as_deref(),
                )
                .map_err(FrontendError::from)?;
            }

            crate::state::save_local_sidecar_json(
                service,
                BACKUP_SYNC_STATE_KEY,
                &StoredBackupSyncState::default(),
                password.as_deref(),
            )
            .map_err(FrontendError::from)?;
            Ok(())
        },
    )?;

    Ok(deleted_count)
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
        let ch_prefix = if ch == "wot" || ch == "lock" {
            None
        } else {
            my_prefix
        };
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
    save_channel_max_bytes_updates(state, session, password, &channel_max_bytes_updates);
    authorized_channels
}

pub async fn probe_public_sync_impl(
    url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPublicSyncInfo, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::Unlocked,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let canonical_url = humoco_market::canonicalize_write_url(&url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&canonical_url).map_err(FrontendError::from)?;

    let probe_client = sync_client::create_sync_probe_client();
    let info = sync_client::fetch_sync_info(&probe_client, &canonical_url)
        .await
        .map_err(FrontendError::from)?;

    let id_ctx_opt = state.with_session_required(
        &session,
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
            &session,
            pw,
        )
        .await;
        (access_pubkey, authorized_channels)
    } else {
        (String::new(), Vec::new())
    };

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
    state.authorize(crate::state::RequiredAccess::Full, password.as_deref())?;
    let canonical_url = humoco_market::canonicalize_write_url(&write_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&canonical_url).map_err(FrontendError::from)?;
    sync_client::validate_sync_read_url(&read_url).map_err(FrontendError::from)?;
    humoco_market::parse_ed25519_pubkey_hex64(&drop_pub)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let session = state
        .authorize_and_begin_session(crate::state::RequiredAccess::Full, password.as_deref())?;
    let id_ctx = state.with_session_required(
        &session,
        "Active wallet changed before public sync could be prepared",
        |service| SyncIdentityContext::from_service(service, password.as_deref()),
    )?;

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
        &session,
        password.as_deref(),
    )
    .await;

    state.with_session_required(
        &session,
        "Active wallet changed before public sync could be saved",
        |service| {
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

            let updated_policy = state
                .mutate_security_policy(service, password.as_deref(), |policy| {
                    if let Some(pos) = policy
                        .public_syncs
                        .iter()
                        .position(|d| d.write_url == canonical_url)
                    {
                        policy.public_syncs[pos] = stored;
                    } else {
                        policy.public_syncs.push(stored);
                    }
                })
                .map_err(FrontendError::from)?;

            sync_own_card_drops_on_settings_change(
                state,
                service,
                &updated_policy.market_selected_syncs,
                password.as_deref(),
            );
            Ok(())
        },
    )?;

    if enabled && info_opt.as_ref().is_some_and(|i| i.lock.is_some()) {
        let lock_probe =
            sync_client::probe_lock_server(&probe_client, &canonical_url, Some(&drop_pub)).await;
        if !lock_probe.lock_pub.trim().is_empty() {
            let _ =
                state.mutate_internal_lifecycle_for_session(&session, password.as_deref(), |lc| {
                    crate::services::common::upsert_verified_lock_probe(
                        &mut lc.l2_probe_cache,
                        lock_probe.clone(),
                    )
                });
        }
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
    let session = state
        .authorize_and_begin_session(crate::state::RequiredAccess::Full, password.as_deref())?;
    let canonical_url = humoco_market::canonicalize_write_url(&write_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    if delete_remote_slots {
        let id_ctx = state.with_session_required(
            &session,
            "Active wallet changed before public sync deletion could be prepared",
            |service| SyncIdentityContext::from_service(service, password.as_deref()),
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
                fe.details.insert("error".to_string(), err);
                fe
            })?;
    }

    let _backup_guard = state.backup_op_lock.lock().await;
    state.with_session_required(
        &session,
        "Active wallet changed before public sync could be removed",
        |service| {
            let mut removed_urls = vec![canonical_url.clone(), write_url.clone()];
            let updated_policy = state
                .mutate_security_policy(service, password.as_deref(), |policy| {
                    for d in &policy.public_syncs {
                        if d.write_url == canonical_url && !d.read_url.is_empty() {
                            removed_urls.push(d.read_url.clone());
                        }
                    }
                    policy.public_syncs.retain(|d| d.write_url != canonical_url);
                })
                .map_err(FrontendError::from)?;

            sync_own_card_drops_on_settings_change(
                state,
                service,
                &updated_policy.market_selected_syncs,
                password.as_deref(),
            );

            if let Ok(mut reg) = state.get_or_load_wot_register(service, password.as_deref()) {
                reg.clear_drop_sent_log(Some(&canonical_url));
                let _ = state.save_wot_register(service, &reg, password.as_deref());
            }

            if let Some(mut sync_state) = crate::state::load_encrypted_json_opt::<StoredSyncState>(
                service,
                MARKET_SYNC_KEY,
                password.as_deref(),
            )
            .map_err(FrontendError::from)?
            {
                purge_urls_from_sync_state(&mut sync_state, &removed_urls);
                crate::state::save_encrypted_json(
                    service,
                    MARKET_SYNC_KEY,
                    &sync_state,
                    password.as_deref(),
                )
                .map_err(FrontendError::from)?;
            }

            if let Some(mut post_state) = crate::state::load_encrypted_json_opt::<StoredPostState>(
                service,
                MARKET_POST_STATE_KEY,
                password.as_deref(),
            )
            .map_err(FrontendError::from)?
            {
                post_state.slot_etags_by_write_url.remove(&canonical_url);
                post_state.slot_etags_by_write_url.remove(&write_url);
                crate::state::save_encrypted_json(
                    service,
                    MARKET_POST_STATE_KEY,
                    &post_state,
                    password.as_deref(),
                )
                .map_err(FrontendError::from)?;
            }

            if let Some(mut backup_state) = crate::state::load_local_sidecar_json_opt::<
                StoredBackupSyncState,
            >(
                service, BACKUP_SYNC_STATE_KEY, password.as_deref()
            )
            .map_err(FrontendError::from)?
            {
                let removed = backup_state
                    .mirrors_by_write_url
                    .remove(&canonical_url)
                    .is_some()
                    || backup_state
                        .mirrors_by_write_url
                        .remove(&write_url)
                        .is_some();
                if removed {
                    crate::state::save_local_sidecar_json(
                        service,
                        BACKUP_SYNC_STATE_KEY,
                        &backup_state,
                        password.as_deref(),
                    )
                    .map_err(FrontendError::from)?;
                }
            }
            Ok(())
        },
    )?;

    let _ = enqueue_guardian_locator_updates_if_configured(state, password.clone()).await;

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
                        drop_pub: "11".repeat(32),
                        register_url: None,
                        access_pubkey: "22".repeat(32),
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
}
