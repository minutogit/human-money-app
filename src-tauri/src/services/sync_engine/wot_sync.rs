use std::collections::{BTreeSet, HashSet};

use human_money_core::models::conflict::TrustStatus;
use sha2::{Digest, Sha256};

use crate::models::{FrontendAddressBook, FrontendError, FrontendTrustChangeImpact};
use crate::services::common::{normalize_sync_url, DropChannelSigner, SyncIdentityContext};
use crate::session::SessionToken;
use crate::settings::{SecurityPolicy, UiPreferences, SYNC_WOT_MAX_BYTES};
use crate::state::{canonicalize_did, AppState, StoredSyncState, CONTACTS_DATA_NAME};
use crate::sync_client::{self, PutSlotOutcome};

use super::channel_reader::{
    channel_max_bytes_key, collect_upload_targets_for_channel, ensure_cached_channel_max_bytes,
};

#[derive(Default)]
pub(crate) struct WotSyncUploadOutcome {
    pub committed_receipts: Vec<humoco_wot::DropSyncReceipt>,
    pub cleared_drop_urls: Vec<String>,
}

struct WotUploadContext {
    reg_opt: Option<humoco_wot::TrustRegister>,
    zero_book_dids: Vec<String>,
    offenders: HashSet<String>,
}

struct WotTombstoneBundle {
    pkg_bytes: Option<Vec<u8>>,
    fingerprint: Option<String>,
    reader_vks: Vec<ed25519_dalek::VerifyingKey>,
}

pub(crate) fn import_decrypted_wot_package(
    pkg: &humoco_wot::WotSyncPackage,
    read_url: &str,
    now_secs: u64,
    reg: &mut humoco_wot::TrustRegister,
    errors: &mut Vec<String>,
) -> usize {
    match reg.import_sync_package(pkg, None, now_secs) {
        Ok(stats) => stats.accepted_total() + stats.merged,
        Err(e) => {
            errors.push(format!(
                "Failed to import wot package from {}: {}",
                read_url, e
            ));
            0
        }
    }
}

fn load_wot_upload_context(
    state: &AppState,
    session: &SessionToken,
    ui_prefs: &UiPreferences,
    wot_upload_targets: &[humoco_market::DropDescriptor],
    password: Option<&str>,
) -> Result<WotUploadContext, FrontendError> {
    if !ui_prefs.wot_share_via_sync && wot_upload_targets.is_empty() {
        return Ok(WotUploadContext {
            reg_opt: None,
            zero_book_dids: Vec::new(),
            offenders: HashSet::new(),
        });
    }

    state.with_session_required(
        session,
        "Wallet session changed during WoT upload preparation",
        |service| {
            let reg_opt = Some(
                state
                    .get_or_load_wot_register(service, password)
                    .map_err(FrontendError::from)?,
            );
            let address_book: FrontendAddressBook =
                crate::state::load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)
                    .map_err(FrontendError::from)?;

            let zero_book_dids: Vec<String> = address_book
                .contacts
                .values()
                .filter(|c| c.trust_level.unwrap_or(0) == 0)
                .filter_map(|c| canonicalize_did(&c.did).ok())
                .collect();

            let offenders: HashSet<String> = if ui_prefs.wot_share_via_sync {
                if let Some(ref r) = reg_opt {
                    let mut candidate_dids: HashSet<String> =
                        r.own_contacts.keys().cloned().collect();
                    for d in &zero_book_dids {
                        candidate_dids.insert(d.clone());
                    }
                    for did in r.get_or_build_graph_cache().did_to_id.keys() {
                        candidate_dids.insert(did.clone());
                    }
                    candidate_dids
                        .into_iter()
                        .filter(|did| {
                            service
                                .with_wallet(|w| {
                                    matches!(w.check_reputation(did), TrustStatus::KnownOffender(_))
                                })
                                .unwrap_or(false)
                        })
                        .collect()
                } else {
                    HashSet::new()
                }
            } else {
                HashSet::new()
            };

            Ok(WotUploadContext {
                reg_opt,
                zero_book_dids,
                offenders,
            })
        },
    )
}

fn build_wot_tombstone_bundle(
    reg: &mut humoco_wot::TrustRegister,
    zero_book_dids: &[String],
    filtered_dids: &HashSet<&str>,
    is_offender_set: &HashSet<String>,
    now_secs: u64,
) -> WotTombstoneBundle {
    let mut direct_tomb_dids: BTreeSet<String> = BTreeSet::new();
    for (did, entry) in &reg.own_contacts {
        if entry.level == 0
            && !filtered_dids.contains(did.as_str())
            && !is_offender_set.contains(did)
        {
            direct_tomb_dids.insert(did.clone());
        }
    }
    for did in zero_book_dids {
        if did != &reg.self_did
            && !filtered_dids.contains(did.as_str())
            && !is_offender_set.contains(did)
        {
            direct_tomb_dids.insert(did.clone());
        }
    }

    let mut graph_tomb_dids: BTreeSet<String> = BTreeSet::new();
    if reg.own_signed_list.is_some() {
        let own_list = reg.own_signed_list.clone().unwrap();
        let eff_shrink = if reg.shrink_until > now_secs {
            reg.shrink_hops.min(3)
        } else {
            0
        };
        let tomb_max_hops = eff_shrink.max(own_list.max_hops.min(3));
        if tomb_max_hops >= 1 {
            let self_did = reg.self_did.clone();
            let graph = reg.get_or_build_graph_cache();
            let distances = graph.shortest_distances_from_self();
            for (did, &id) in &graph.did_to_id {
                if did == &self_did
                    || filtered_dids.contains(did.as_str())
                    || is_offender_set.contains(did)
                    || direct_tomb_dids.contains(did)
                {
                    continue;
                }
                if distances
                    .get(&id)
                    .is_some_and(|&dist| (1..=tomb_max_hops).contains(&dist))
                {
                    graph_tomb_dids.insert(did.clone());
                }
            }
        }
    }

    let mut tombstone_dids: Vec<String> = direct_tomb_dids.into_iter().collect();
    tombstone_dids.extend(graph_tomb_dids);
    tombstone_dids.truncate(humoco_market::MAX_SLOT_RECIPIENTS);

    let mut reader_vks: Vec<ed25519_dalek::VerifyingKey> = Vec::with_capacity(tombstone_dids.len());
    for did in &tombstone_dids {
        if let Ok(vk) = human_money_core::crypto::get_pubkey_from_user_id(did) {
            reader_vks.push(vk);
        }
    }

    let (pkg_bytes, fingerprint) = if !reader_vks.is_empty() {
        if let Some(ref own_list) = reg.own_signed_list {
            let pkg = humoco_wot::WotSyncPackage {
                format_version: 1,
                lists: vec![format!("{}~", own_list.jws_compact)],
            };
            match serde_json::to_vec(&pkg) {
                Ok(bytes) => {
                    let mut hasher = Sha256::new();
                    hasher.update(own_list.jws_compact.as_bytes());
                    for d in &tombstone_dids {
                        hasher.update(b"\0");
                        hasher.update(d.as_bytes());
                    }
                    (Some(bytes), Some(hex::encode(hasher.finalize())))
                }
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    WotTombstoneBundle {
        pkg_bytes,
        fingerprint,
        reader_vks,
    }
}

struct WotUploadState<'a> {
    client: &'a reqwest::Client,
    state: &'a AppState,
    session: &'a SessionToken,
    id_ctx: &'a SyncIdentityContext,
    now_secs: u64,
    force_upload: bool,
    sync_state: &'a mut StoredSyncState,
    uploaded_to: &'a mut usize,
    errors: &'a mut Vec<String>,
    committed_receipts: Vec<humoco_wot::DropSyncReceipt>,
    cleared_drop_urls: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
async fn upload_wot_subpackages_to_target(
    ctx: &mut WotUploadState<'_>,
    reg: &mut humoco_wot::TrustRegister,
    drop_desc: &humoco_market::DropDescriptor,
    readers_refs: &[(&str, u8)],
    tombstone: &WotTombstoneBundle,
    total_overhead: usize,
    tomb_overhead: usize,
    active_classes: usize,
) -> bool {
    if ctx
        .state
        .is_origin_cooling_down(&drop_desc.write_url, std::time::Instant::now())
    {
        ctx.errors.push(format!(
            "WoT upload to {} deferred: server origin cooling down after HTTP 429",
            drop_desc.write_url
        ));
        return false;
    }
    let mut effective_wot_max_bytes = ensure_cached_channel_max_bytes(
        ctx.client,
        ctx.sync_state,
        ctx.id_ctx,
        drop_desc,
        None,
        "wot",
        SYNC_WOT_MAX_BYTES,
    )
    .await;
    if !ctx.state.is_generation_current(ctx.session.generation) {
        ctx.errors
            .push("Active wallet session changed before WoT upload completed".to_string());
        return false;
    }

    let norm_write_url = normalize_sync_url(&drop_desc.write_url);
    let tomb_changed = ctx
        .sync_state
        .wot_tombstone_fp_by_write_url
        .get(&norm_write_url)
        != tombstone.fingerprint.as_ref();
    let force_wot = ctx.force_upload || tomb_changed;

    let mut reg_modified = false;
    for attempt in 0..2 {
        if !ctx.state.is_generation_current(ctx.session.generation) {
            ctx.errors
                .push("Active wallet session changed during WoT upload retry".to_string());
            break;
        }
        let initial_class_budget = effective_wot_max_bytes
            .saturating_sub(total_overhead)
            .saturating_sub(tomb_overhead)
            / active_classes;
        let class_budget = if attempt == 0 {
            initial_class_budget
        } else {
            (initial_class_budget / 2)
                .max(1024)
                .min(initial_class_budget)
        };

        match reg.prepare_drop_sync_subpackages(
            &drop_desc.write_url,
            Some(readers_refs),
            Some(class_budget),
            force_wot,
        ) {
            Ok(Some((subpackages, receipt))) => {
                if subpackages.is_empty() && tombstone.pkg_bytes.is_none() {
                    break;
                }

                let mut class_payloads = Vec::with_capacity(subpackages.len() + 1);
                for sub in &subpackages {
                    match serde_json::to_vec(&sub.package) {
                        Ok(pkg_bytes) => {
                            let mut reader_vks = Vec::with_capacity(sub.reader_dids.len());
                            for did in &sub.reader_dids {
                                if let Ok(vk) =
                                    human_money_core::crypto::get_pubkey_from_user_id(did)
                                {
                                    reader_vks.push(vk);
                                }
                            }
                            class_payloads.push((pkg_bytes, reader_vks));
                        }
                        Err(e) => {
                            ctx.errors
                                .push(format!("Failed to serialize WoT subpackage: {}", e));
                        }
                    }
                }
                if !tombstone.reader_vks.is_empty() {
                    if let Some(ref tomb_bytes) = tombstone.pkg_bytes {
                        class_payloads.push((tomb_bytes.clone(), tombstone.reader_vks.clone()));
                    }
                }

                let class_slices: Vec<(&[u8], &[ed25519_dalek::VerifyingKey])> = class_payloads
                    .iter()
                    .map(|(b, v)| (b.as_slice(), v.as_slice()))
                    .filter(|(_, v)| !v.is_empty())
                    .collect();

                if class_slices.is_empty() {
                    break;
                }

                match humoco_market::seal_multi_class_slot_blob(
                    &class_slices,
                    effective_wot_max_bytes,
                ) {
                    Ok(blob) => {
                        let signer = DropChannelSigner {
                            is_public: false,
                            master_seed: &ctx.id_ctx.master_seed,
                            my_ed_sk: &ctx.id_ctx.my_ed_sk,
                            drop_desc,
                        };
                        let mut retry_after_413 = false;
                        match signer.sign_request(
                            "PUT",
                            "wot",
                            None,
                            ctx.now_secs,
                            &sync_client::generate_sync_nonce(),
                            &blob,
                        ) {
                            Ok(signed) => {
                                match sync_client::put_slot(
                                    ctx.client,
                                    &drop_desc.write_url,
                                    &signed,
                                    "wot",
                                    &blob,
                                    None,
                                )
                                .await
                                {
                                    Ok(PutSlotOutcome::Saved { .. }) => {
                                        *ctx.uploaded_to += 1;
                                        ctx.sync_state.last_upload_at = Some(ctx.now_secs);
                                        ctx.committed_receipts.push(receipt.clone());
                                        reg.commit_drop_sync_receipt(receipt, ctx.now_secs);
                                        reg_modified = true;
                                        match tombstone.fingerprint {
                                            Some(ref fp) => {
                                                ctx.sync_state
                                                    .wot_tombstone_fp_by_write_url
                                                    .insert(norm_write_url.clone(), fp.clone());
                                            }
                                            None => {
                                                ctx.sync_state
                                                    .wot_tombstone_fp_by_write_url
                                                    .remove(&norm_write_url);
                                            }
                                        }
                                    }
                                    Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                        ctx.errors.push(format!(
                                            "Upload wot to {}: Precondition failed (ETag)",
                                            drop_desc.write_url
                                        ));
                                    }
                                    Err(e) => {
                                        ctx.state.record_origin_cooldown_if_429(
                                            &drop_desc.write_url,
                                            &e,
                                        );
                                        if e.contains("HTTP 413") {
                                            ctx.sync_state.channel_max_bytes_by_write_url.remove(
                                                &channel_max_bytes_key(&drop_desc.write_url, "wot"),
                                            );
                                            if attempt == 0
                                                && !ctx.state.is_origin_cooling_down(
                                                    &drop_desc.write_url,
                                                    std::time::Instant::now(),
                                                )
                                            {
                                                let prev_effective = effective_wot_max_bytes;
                                                let reprobed = ensure_cached_channel_max_bytes(
                                                    ctx.client,
                                                    ctx.sync_state,
                                                    ctx.id_ctx,
                                                    drop_desc,
                                                    None,
                                                    "wot",
                                                    SYNC_WOT_MAX_BYTES,
                                                )
                                                .await;
                                                effective_wot_max_bytes =
                                                    if reprobed < prev_effective {
                                                        reprobed
                                                    } else {
                                                        (prev_effective / 2).max(1024)
                                                    };
                                                retry_after_413 = true;
                                            }
                                        }
                                        if !retry_after_413 {
                                            ctx.errors.push(format!(
                                                "Upload wot to {}: {}",
                                                drop_desc.write_url, e
                                            ));
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                ctx.errors.push(format!(
                                    "Signing error for wot on {}: {}",
                                    drop_desc.write_url, e
                                ));
                            }
                        }
                        if retry_after_413 {
                            continue;
                        }
                        break;
                    }
                    Err(humoco_market::MarketError::PayloadTooLarge(_)) if attempt == 0 => {
                        continue;
                    }
                    Err(e) => {
                        ctx.errors
                            .push(format!("Failed to seal multi-class wot slot blob: {}", e));
                        break;
                    }
                }
            }
            Ok(None) => {
                // Slot on transport_id is already up-to-date.
                break;
            }
            Err(e) => {
                ctx.errors.push(format!(
                    "Failed to prepare drop sync subpackages for {}: {}",
                    drop_desc.write_url, e
                ));
                break;
            }
        }
    }

    reg_modified
}

async fn upload_wot_tombstones_only(
    ctx: &mut WotUploadState<'_>,
    reg: &mut humoco_wot::TrustRegister,
    wot_upload_targets: &[humoco_market::DropDescriptor],
    tomb_bytes: &[u8],
    tomb_fp: &str,
    tombstone_reader_vks: &[ed25519_dalek::VerifyingKey],
) -> bool {
    let mut reg_modified = false;
    for drop_desc in wot_upload_targets {
        if !ctx.state.is_generation_current(ctx.session.generation) {
            break;
        }
        let norm_write_url = normalize_sync_url(&drop_desc.write_url);
        if norm_write_url.is_empty() {
            continue;
        }
        if ctx
            .state
            .is_origin_cooling_down(&drop_desc.write_url, std::time::Instant::now())
        {
            ctx.errors.push(format!(
                "WoT tombstone upload to {} deferred: server origin cooling down after HTTP 429",
                drop_desc.write_url
            ));
            continue;
        }
        let tomb_changed = ctx
            .sync_state
            .wot_tombstone_fp_by_write_url
            .get(&norm_write_url)
            .map(|s| s.as_str())
            != Some(tomb_fp);
        if !ctx.force_upload
            && !tomb_changed
            && !reg.drop_sent_log.contains_key(&drop_desc.write_url)
        {
            continue;
        }

        let effective_wot_max_bytes = ensure_cached_channel_max_bytes(
            ctx.client,
            ctx.sync_state,
            ctx.id_ctx,
            drop_desc,
            None,
            "wot",
            SYNC_WOT_MAX_BYTES,
        )
        .await;
        if !ctx.state.is_generation_current(ctx.session.generation) {
            break;
        }

        match humoco_market::seal_slot_payload(
            tomb_bytes,
            tombstone_reader_vks,
            effective_wot_max_bytes,
        ) {
            Ok(blob) => {
                let signer = DropChannelSigner {
                    is_public: false,
                    master_seed: &ctx.id_ctx.master_seed,
                    my_ed_sk: &ctx.id_ctx.my_ed_sk,
                    drop_desc,
                };
                match signer.sign_request(
                    "PUT",
                    "wot",
                    None,
                    ctx.now_secs,
                    &sync_client::generate_sync_nonce(),
                    &blob,
                ) {
                    Ok(signed) => {
                        match sync_client::put_slot(
                            ctx.client,
                            &drop_desc.write_url,
                            &signed,
                            "wot",
                            &blob,
                            None,
                        )
                        .await
                        {
                            Ok(PutSlotOutcome::Saved { .. }) => {
                                *ctx.uploaded_to += 1;
                                ctx.sync_state.last_upload_at = Some(ctx.now_secs);
                                if reg.drop_sent_log.remove(&drop_desc.write_url).is_some() {
                                    ctx.cleared_drop_urls.push(drop_desc.write_url.clone());
                                    reg_modified = true;
                                }
                                ctx.sync_state
                                    .wot_tombstone_fp_by_write_url
                                    .insert(norm_write_url, tomb_fp.to_string());
                            }
                            Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                ctx.errors.push(format!(
                                    "Upload wot tombstone to {}: Precondition failed (ETag)",
                                    drop_desc.write_url
                                ));
                            }
                            Err(e) => {
                                if e.contains("HTTP 413") {
                                    ctx.sync_state.channel_max_bytes_by_write_url.remove(
                                        &channel_max_bytes_key(&drop_desc.write_url, "wot"),
                                    );
                                }
                                ctx.state
                                    .record_origin_cooldown_if_429(&drop_desc.write_url, &e);
                                ctx.errors.push(format!(
                                    "Upload wot tombstone to {}: {}",
                                    drop_desc.write_url, e
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        ctx.errors.push(format!(
                            "Signing error for wot tombstone on {}: {}",
                            drop_desc.write_url, e
                        ));
                    }
                }
            }
            Err(e) => {
                ctx.errors
                    .push(format!("Failed to seal wot tombstone blob: {}", e));
            }
        }
    }
    reg_modified
}

async fn delete_unshared_wot_slots(
    ctx: &mut WotUploadState<'_>,
    reg: &mut humoco_wot::TrustRegister,
    wot_upload_targets: &[humoco_market::DropDescriptor],
) -> bool {
    let mut reg_modified = false;
    for drop_desc in wot_upload_targets {
        if !ctx.state.is_generation_current(ctx.session.generation) {
            break;
        }
        let norm_write_url = normalize_sync_url(&drop_desc.write_url);
        if norm_write_url.is_empty() {
            continue;
        }
        if ctx
            .state
            .is_origin_cooling_down(&drop_desc.write_url, std::time::Instant::now())
        {
            ctx.errors.push(format!(
                "WoT slot deletion on {} deferred: server origin cooling down after HTTP 429",
                drop_desc.write_url
            ));
            continue;
        }
        if !ctx.force_upload
            && !reg.drop_sent_log.contains_key(&drop_desc.write_url)
            && !ctx
                .sync_state
                .wot_tombstone_fp_by_write_url
                .contains_key(&norm_write_url)
        {
            continue;
        }

        let signer = DropChannelSigner {
            is_public: false,
            master_seed: &ctx.id_ctx.master_seed,
            my_ed_sk: &ctx.id_ctx.my_ed_sk,
            drop_desc,
        };
        match signer.sign_request(
            "DELETE",
            "wot",
            None,
            ctx.now_secs,
            &sync_client::generate_sync_nonce(),
            b"",
        ) {
            Ok(signed) => {
                match sync_client::delete_slot(ctx.client, &drop_desc.write_url, &signed, "wot")
                    .await
                {
                    Ok(_) => {
                        if reg.drop_sent_log.remove(&drop_desc.write_url).is_some() {
                            ctx.cleared_drop_urls.push(drop_desc.write_url.clone());
                            reg_modified = true;
                        }
                        ctx.sync_state
                            .wot_tombstone_fp_by_write_url
                            .remove(&norm_write_url);
                    }
                    Err(e) => {
                        ctx.state
                            .record_origin_cooldown_if_429(&drop_desc.write_url, &e);
                        ctx.errors.push(format!(
                            "Failed to delete unshared WoT slot on {}: {}",
                            drop_desc.write_url, e
                        ));
                    }
                }
            }
            Err(e) => {
                ctx.errors.push(format!(
                    "Failed to sign WoT slot DELETE for {}: {}",
                    drop_desc.write_url, e
                ));
            }
        }
    }
    reg_modified
}

/// Uploads encrypted selective-disclosure WoT packages or revokes stale WoT slots across sync targets.
/// Returns pending `TrustRegister` mutations (`committed_receipts`, `cleared_drop_urls`) for atomic persistence.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync_wot_upload_channel(
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
    errors: &mut Vec<String>,
) -> WotSyncUploadOutcome {
    let wot_upload_targets = collect_upload_targets_for_channel(security_policy, "wot");
    let ctx_data =
        match load_wot_upload_context(state, session, ui_prefs, &wot_upload_targets, password) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("WoT upload context failed: {}", e.message));
                return WotSyncUploadOutcome::default();
            }
        };

    let Some(mut reg) = ctx_data.reg_opt else {
        return WotSyncUploadOutcome::default();
    };

    let mut delete_unshared = !ui_prefs.wot_share_via_sync;

    let mut upload_state = WotUploadState {
        client,
        state,
        session,
        id_ctx,
        now_secs,
        force_upload: force_upload == Some(true),
        sync_state,
        uploaded_to,
        errors,
        committed_receipts: Vec::new(),
        cleared_drop_urls: Vec::new(),
    };

    if ui_prefs.wot_share_via_sync {
        let needs_signing =
            reg.own_list_dirty || (reg.own_signed_list.is_none() && !reg.own_contacts.is_empty());
        if needs_signing {
            let signed_res = state.with_session(session, |service| {
                state
                    .ensure_wot_list_signed(service, &mut reg)
                    .map_err(FrontendError::from)?;
                state
                    .save_wot_register(service, &reg, password)
                    .map_err(FrontendError::from)?;
                Ok(())
            });
            match signed_res {
                Ok(Some(())) => {}
                Ok(None) => return WotSyncUploadOutcome::default(),
                Err(e) => {
                    upload_state
                        .errors
                        .push(format!("Failed to sign or save WoT list: {}", e.message));
                    return WotSyncUploadOutcome::default();
                }
            }
        }

        let auth_readers = reg.authorized_drop_readers();
        let mut class_buckets: [Vec<(humoco_wot::Did, u8)>; 3] =
            [Vec::new(), Vec::new(), Vec::new()];
        for (did, cls) in auth_readers {
            if !ctx_data.offenders.contains(&did) && (1..=3).contains(&cls) {
                class_buckets[(cls - 1) as usize].push((did, cls));
            }
        }
        let mut filtered_readers: Vec<(humoco_wot::Did, u8)> = Vec::new();
        for bucket in &mut class_buckets {
            bucket.sort_by(|(did_a, _), (did_b, _)| {
                let a_direct = reg.own_contacts.contains_key(did_a);
                let b_direct = reg.own_contacts.contains_key(did_b);
                b_direct.cmp(&a_direct).then_with(|| did_a.cmp(did_b))
            });
            bucket.truncate(humoco_market::MAX_SLOT_RECIPIENTS);
            filtered_readers.extend(bucket.iter().cloned());
        }
        let filtered_dids: HashSet<&str> = filtered_readers
            .iter()
            .map(|(did, _)| did.as_str())
            .collect();

        let tombstone = build_wot_tombstone_bundle(
            &mut reg,
            &ctx_data.zero_book_dids,
            &filtered_dids,
            &ctx_data.offenders,
            now_secs,
        );

        if !filtered_readers.is_empty() {
            let readers_refs: Vec<(&str, u8)> = filtered_readers
                .iter()
                .map(|(did, class)| (did.as_str(), *class))
                .collect();

            let active_classes = (1u8..=3)
                .filter(|&d| filtered_readers.iter().any(|(_, cls)| *cls == d))
                .count()
                .max(1);
            let mut total_overhead: usize = 32;
            for d in 1u8..=3 {
                let cnt = filtered_readers.iter().filter(|(_, cls)| *cls == d).count();
                if cnt > 0 {
                    total_overhead += humoco_market::slot_overhead_bytes(cnt).unwrap_or(8_250);
                }
            }
            let tomb_overhead = if let Some(ref tb) = tombstone.pkg_bytes {
                humoco_market::slot_overhead_bytes(tombstone.reader_vks.len())
                    .unwrap_or(8_250)
                    .saturating_add(tb.len())
                    .saturating_add(32)
            } else {
                0
            };

            for drop_desc in &wot_upload_targets {
                if !state.is_generation_current(session.generation) {
                    return WotSyncUploadOutcome::default();
                }
                if drop_desc.write_url.is_empty() {
                    continue;
                }
                let _ = upload_wot_subpackages_to_target(
                    &mut upload_state,
                    &mut reg,
                    drop_desc,
                    &readers_refs,
                    &tombstone,
                    total_overhead,
                    tomb_overhead,
                    active_classes,
                )
                .await;
            }
        } else if !tombstone.reader_vks.is_empty() {
            if let (Some(ref tomb_bytes), Some(ref tomb_fp)) =
                (&tombstone.pkg_bytes, &tombstone.fingerprint)
            {
                let _ = upload_wot_tombstones_only(
                    &mut upload_state,
                    &mut reg,
                    &wot_upload_targets,
                    tomb_bytes,
                    tomb_fp,
                    &tombstone.reader_vks,
                )
                .await;
            } else {
                delete_unshared = true;
            }
        } else {
            delete_unshared = true;
        }
    }

    if delete_unshared {
        let _ = delete_unshared_wot_slots(&mut upload_state, &mut reg, &wot_upload_targets).await;
    }

    WotSyncUploadOutcome {
        committed_receipts: upload_state.committed_receipts,
        cleared_drop_urls: upload_state.cleared_drop_urls,
    }
}

pub fn preview_contact_trust_change_impl(
    did: String,
    new_level: u8,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendTrustChangeImpact, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let (ui_prefs, reg, admin_opt) = if pw.is_some() || service.is_session_active() {
        let ui_prefs = state
            .get_or_load_ui_preferences(&mut service, pw)
            .map_err(FrontendError::from)?;
        let reg = state.get_or_load_wot_register(&mut service, pw).ok();
        let admin_opt = super::admin::load_stored_sync_admin_opt(&mut service, pw)?;
        (ui_prefs, reg, admin_opt)
    } else {
        // timeout == 0 graceful cache fallback
        let ui_prefs = state
            .read_cache_slot_for_service(&service, |c| &c.ui_preferences)
            .unwrap_or_default();
        let reg = state.read_cache_slot_for_service(&service, |c| &c.wot);
        (ui_prefs, reg, None)
    };

    let can_target = canonicalize_did(&did).unwrap_or_else(|_| did.clone());

    let own_can_did = service
        .with_wallet(|w| canonicalize_did(w.get_user_id()).ok())
        .ok()
        .flatten();
    if own_can_did.as_deref() == Some(can_target.as_str()) {
        return Ok(FrontendTrustChangeImpact {
            can_read_my_market_before: false,
            can_read_my_market_after: false,
            can_read_my_wot_before: false,
            can_read_my_wot_after: false,
            can_write_my_sync_before: true,
            can_write_my_sync_after: true,
            can_send_me_post_before: true,
            can_send_me_post_after: true,
        });
    }

    let current_level = reg
        .as_ref()
        .and_then(|r| r.own_contacts.get(&can_target).map(|e| e.level));

    let is_offender = service
        .with_wallet(|w| {
            matches!(
                w.check_reputation(&can_target),
                TrustStatus::KnownOffender(_)
            )
        })
        .unwrap_or(false);

    let can_read_my_market_before =
        current_level.unwrap_or(0) >= ui_prefs.share_min_level && !is_offender;
    let can_read_my_market_after = new_level >= ui_prefs.share_min_level && !is_offender;

    let can_read_my_wot_before =
        ui_prefs.wot_share_via_sync && current_level.unwrap_or(0) >= 1 && !is_offender;
    let can_read_my_wot_after = ui_prefs.wot_share_via_sync && new_level >= 1 && !is_offender;

    let (can_write_my_sync_before, can_write_my_sync_after) = if is_offender {
        (false, false)
    } else if let Some(admin) = admin_opt {
        if admin
            .allow
            .iter()
            .any(|d| canonicalize_did(d).ok().as_deref() == Some(&can_target))
        {
            (true, true)
        } else {
            let policy = admin.policies.get("card").cloned().unwrap_or_default();
            (
                current_level.is_some_and(|lvl| lvl >= policy.direct_min),
                new_level >= policy.direct_min,
            )
        }
    } else {
        (false, false)
    };

    let can_send_me_post_before =
        ui_prefs.post_accept_from_contacts && current_level.unwrap_or(0) >= 1;
    let can_send_me_post_after = ui_prefs.post_accept_from_contacts && new_level >= 1;

    Ok(FrontendTrustChangeImpact {
        can_read_my_market_before,
        can_read_my_market_after,
        can_read_my_wot_before,
        can_read_my_wot_after,
        can_write_my_sync_before,
        can_write_my_sync_after,
        can_send_me_post_before,
        can_send_me_post_after,
    })
}
