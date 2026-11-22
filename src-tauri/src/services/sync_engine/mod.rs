pub mod admin;
pub mod card_sync;
pub mod channel_reader;
pub mod media_sync;
pub mod wot_sync;

#[cfg(test)]
pub(crate) use admin::sync_channel_members_impl;
pub use admin::{
    compute_admin_sync_members_with_stats, compute_channel_members_hash, delete_sync_admin_impl,
    finalize_sync_setup_impl, get_rust_sync_config_impl, get_sync_admin_status_impl,
    prepare_sync_setup_impl, revoke_own_sync_server_impl, test_sync_connection_impl,
    update_sync_admin_config_impl,
};
pub(crate) use card_sync::probe_contact_syncs_with_session;
pub use card_sync::{
    discover_available_syncs_impl, probe_contact_syncs_impl, set_selected_upload_syncs_impl,
};
pub use channel_reader::{probe_public_sync_impl, remove_public_sync_impl, save_public_sync_impl};
pub use wot_sync::preview_contact_trust_change_impl;

use std::collections::HashSet;

use crate::models::{
    FrontendAppNotice, FrontendError, FrontendMarketSyncResult, FrontendSyncOverviewStatus,
    MarketSyncStatus,
};
use crate::services::common::{normalize_sync_url, SyncIdentityContext};
use crate::settings::SYNC_MARKET_POLL_COOLDOWN_SECS;
use crate::state::{
    sync_endpoint_key, AppState, StoredSyncState, MARKET_CARD_STORE_KEY, MARKET_SYNC_KEY,
};
use crate::sync_client;

#[derive(Debug, Default)]
pub(crate) struct SyncErrorTracker {
    pub errors: Vec<String>,
    pub max_retry_after_secs: Option<u64>,
}

impl SyncErrorTracker {
    pub(crate) fn push(&mut self, msg: String) {
        self.errors.push(msg);
    }

    pub(crate) fn record_retry_after(&mut self, secs: Option<u64>) {
        if let Some(secs) = secs {
            self.max_retry_after_secs = Some(
                self.max_retry_after_secs
                    .map_or(secs, |prev| prev.max(secs)),
            );
        }
    }

    pub(crate) fn record_sync_error(
        &mut self,
        state: &AppState,
        url: &str,
        err: &sync_client::SyncClientError,
        msg: String,
    ) {
        state.record_origin_cooldown_for_sync_error(url, err);
        self.record_retry_after(err.retry_after_secs());
        self.errors.push(msg);
    }
}

pub async fn sync_market_channels_impl(
    is_wifi: Option<bool>,
    force_upload: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketSyncResult, FrontendError> {
    sync_market_channels_with_options(is_wifi, force_upload, None, password, state).await
}

pub async fn sync_market_channels_with_options(
    is_wifi: Option<bool>,
    force_upload: Option<bool>,
    respect_cooldown: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketSyncResult, FrontendError> {
    let _market_guard = state.market_op_lock.lock().await;
    let auth =
        state.authorize_session(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    let session = &auth.token;
    let pw = auth.pw();

    let (ui_prefs, mut security_policy) = state.with_session_required(
        session,
        "Active wallet session changed before market sync could start",
        |service| {
            let ui_prefs = state
                .get_or_load_ui_preferences(service, pw)
                .map_err(FrontendError::from)?;
            let security_policy = state
                .get_or_load_security_policy(service, pw)
                .map_err(FrontendError::from)?;
            Ok((ui_prefs, security_policy))
        },
    )?;

    if ui_prefs.market_poll_wifi_only && is_wifi == Some(false) {
        return Ok(FrontendMarketSyncResult {
            status: MarketSyncStatus::SkippedWifiOnly,
            uploaded_to: 0,
            polled_syncs: 0,
            decrypted_slots: 0,
            imported_cards: 0,
            new_ads: 0,
            imported_wot_lists: 0,
            errors: Vec::new(),
            retry_after_secs: None,
            revoked_own_syncs: Vec::new(),
        });
    }

    let client = sync_client::create_sync_http_client();
    let mut err_tracker = SyncErrorTracker::default();
    let mut uploaded_to = 0;
    let mut polled_syncs = 0;
    let mut decrypted_slots = 0;
    let mut imported_cards = 0;
    let mut new_ads = 0;
    let mut imported_wot_lists = 0;
    let now_secs = sync_client::current_unix_timestamp();

    // 1. Ensure own DropAdmin channel members are up-to-date on server
    admin::refresh_own_admin_members_locked(&client, state, session, pw, &mut err_tracker).await;

    // 2. Load identity context and sync state
    let Some((id_ctx, mut sync_state)) = state.with_session(session, |service| {
        let id_ctx = SyncIdentityContext::from_service(service, pw)?;
        let sync_state: StoredSyncState =
            crate::state::load_encrypted_json_or_default(service, MARKET_SYNC_KEY, pw)
                .map_err(FrontendError::from)?;
        Ok((id_ctx, sync_state))
    })?
    else {
        return Ok(FrontendMarketSyncResult {
            status: MarketSyncStatus::SessionSuperseded,
            uploaded_to: 0,
            polled_syncs: 0,
            decrypted_slots: 0,
            imported_cards: 0,
            new_ads: 0,
            imported_wot_lists: 0,
            errors: err_tracker.errors,
            retry_after_secs: err_tracker.max_retry_after_secs,
            revoked_own_syncs: Vec::new(),
        });
    };
    let baseline_sync_state = sync_state.clone();

    // 2a. Upload card channel + ad media blobs and garbage-collect orphaned media blobs
    let card_upload_targets =
        channel_reader::collect_upload_targets_for_channel(&security_policy, "card");
    if let Some(active_media_blob_ids) = card_sync::sync_card_upload_channel(
        &client,
        state,
        session,
        &ui_prefs,
        &security_policy,
        &id_ctx,
        force_upload,
        now_secs,
        pw,
        &mut sync_state,
        &mut uploaded_to,
        &mut err_tracker,
    )
    .await
    {
        if state.is_generation_current(session.generation) {
            media_sync::gc_orphaned_media_blobs(
                &client,
                state,
                session,
                &id_ctx,
                &card_upload_targets,
                &active_media_blob_ids,
                now_secs,
                &mut sync_state,
                &mut err_tracker,
            )
            .await;
        }
    }

    // 2b. Upload WoT channel
    let wot_upload_outcome = if state.is_generation_current(session.generation) {
        wot_sync::sync_wot_upload_channel(
            &client,
            state,
            session,
            &ui_prefs,
            &security_policy,
            &id_ctx,
            force_upload,
            now_secs,
            pw,
            &mut sync_state,
            &mut uploaded_to,
            &mut err_tracker,
        )
        .await
    } else {
        wot_sync::WotSyncUploadOutcome::default()
    };

    // 2c. Check revocation.json across own and contact syncs (max once per 24h per (url, drop_pub))
    let revoked_own_syncs = if state.is_generation_current(session.generation) {
        channel_reader::check_revocations(
            &client,
            state,
            session,
            &mut security_policy,
            &mut sync_state,
            now_secs,
            pw,
        )
        .await
    } else {
        Vec::new()
    };

    // 3. Download poll (strictly read/decrypt/delta-collecting; no cross-.await store writes)
    let skip_poll = respect_cooldown == Some(true)
        && force_upload != Some(true)
        && sync_state.last_poll_at.is_some_and(|last| {
            last <= now_secs && now_secs - last < SYNC_MARKET_POLL_COOLDOWN_SECS
        });

    let mut sync_health_updates: Vec<(String, bool)> = Vec::new();
    let mut decoded_card_bundles: Vec<(String, humoco_market::CardBundle)> = Vec::new();
    let mut decoded_wot_packages: Vec<(String, humoco_wot::WotSyncPackage)> = Vec::new();

    if !skip_poll && state.is_generation_current(session.generation) {
        let poll = channel_reader::poll_sync_channels(
            &client,
            state,
            session,
            &ui_prefs,
            &security_policy,
            &id_ctx,
            now_secs,
            pw,
            &mut sync_state,
            &mut err_tracker,
        )
        .await;
        polled_syncs = poll.polled_syncs;
        decrypted_slots = poll.decrypted_slots;
        sync_health_updates = poll.sync_health_updates;
        decoded_card_bundles = poll.decoded_card_bundles;
        decoded_wot_packages = poll.decoded_wot_packages;
    }

    // 4. Atomic Delta-Apply strictly gated by SessionToken: reload fresh stores on commit,
    //    compute all mutations in memory, and commit every modified blob via EncryptedBatchCommit.
    //    If the session was logged out / switched / re-logged in, discard everything cleanly.
    let max_stored_cards = ui_prefs.max_stored_cards.clamp(100, 10_000);
    let committed = state.with_session(session, |service| {
        let latest_sync_state: StoredSyncState =
            crate::state::load_encrypted_json_or_default(service, MARKET_SYNC_KEY, pw)
                .map_err(FrontendError::from)?;
        crate::state::merge_sync_state_3way(
            &baseline_sync_state,
            &mut sync_state,
            &latest_sync_state,
        );

        let mut next_card_store: Option<humoco_market::CardStore> = None;
        let mut next_address_book: Option<crate::models::FrontendAddressBook> = None;
        let mut address_book_changed = false;
        let mut next_security_policy: Option<crate::settings::SecurityPolicy> = None;
        let mut policy_changed = false;
        let mut next_wot_reg: Option<humoco_wot::TrustRegister> = None;
        let mut next_notices_store: Option<crate::models::StoredAppNotices> = None;

        let need_address_book = !decoded_card_bundles.is_empty() || !sync_health_updates.is_empty();
        if need_address_book {
            let ab: crate::models::FrontendAddressBook =
                crate::state::load_encrypted_json_or_default(
                    service,
                    crate::state::CONTACTS_DATA_NAME,
                    pw,
                )
                .map_err(FrontendError::from)?;
            next_address_book = Some(ab);
        }

        if !decoded_card_bundles.is_empty() {
            let mut fresh_store: humoco_market::CardStore =
                crate::state::load_encrypted_json_or_default(service, MARKET_CARD_STORE_KEY, pw)
                    .map_err(FrontendError::from)?;
            let ab_ref = next_address_book
                .as_mut()
                .expect("address book preloaded for card bundle commit");
            for (read_url, bundle) in decoded_card_bundles {
                let (c_cnt, a_cnt) = card_sync::import_decrypted_card_bundle(
                    bundle,
                    &read_url,
                    now_secs,
                    max_stored_cards,
                    service,
                    &mut fresh_store,
                    &mut sync_state,
                    ab_ref,
                    &mut address_book_changed,
                );
                imported_cards += c_cnt;
                new_ads += a_cnt;
            }
            next_card_store = Some(fresh_store);
        }

        let need_wot_update = !decoded_wot_packages.is_empty()
            || !wot_upload_outcome.committed_receipts.is_empty()
            || !wot_upload_outcome.cleared_drop_urls.is_empty();
        if need_wot_update {
            let mut reg = state
                .load_wot_register(service, pw)
                .map_err(FrontendError::from)?;
            let mut reg_dirty = false;
            for (read_url, pkg) in &decoded_wot_packages {
                let imported = wot_sync::import_decrypted_wot_package(
                    pkg,
                    read_url,
                    now_secs,
                    &mut reg,
                    &mut err_tracker.errors,
                );
                imported_wot_lists += imported;
                if imported > 0 {
                    reg_dirty = true;
                }
            }
            for receipt in wot_upload_outcome.committed_receipts {
                reg.commit_drop_sync_receipt(receipt, now_secs);
                reg_dirty = true;
            }
            for url in wot_upload_outcome.cleared_drop_urls {
                if reg.drop_sent_log.remove(&url).is_some() {
                    reg_dirty = true;
                }
            }
            if reg.own_list_dirty {
                state
                    .ensure_wot_list_signed(service, &mut reg)
                    .map_err(FrontendError::from)?;
                reg_dirty = true;
            }
            if reg_dirty {
                next_wot_reg = Some(reg);
            }
        }

        if !sync_health_updates.is_empty() {
            let mut policy = state
                .get_or_load_security_policy(service, pw)
                .map_err(FrontendError::from)?;
            let ab_ref = next_address_book
                .as_mut()
                .expect("address book preloaded for health updates");
            let (ab_dirty, pol_dirty) = crate::state::apply_contact_sync_health_batch(
                ab_ref,
                Some(&mut policy),
                &sync_health_updates,
                now_secs,
            );
            address_book_changed |= ab_dirty;
            if pol_dirty {
                policy_changed = true;
                next_security_policy = Some(policy);
            }
        }

        if polled_syncs > 0 || uploaded_to > 0 {
            let mut notices_store: crate::models::StoredAppNotices =
                crate::state::load_encrypted_json_or_default(
                    service,
                    crate::state::APP_NOTICES_KEY,
                    pw,
                )
                .map_err(FrontendError::from)?;
            let mut market_notices = Vec::new();
            if imported_cards > 0 || new_ads > 0 {
                market_notices.push(FrontendAppNotice {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: "market".to_string(),
                    created_at: now_secs,
                    read: false,
                    imported_payments: 0,
                    imported_receipts: 0,
                    imported_signatures: 0,
                    new_sign_requests: 0,
                    new_wot_handshakes: 0,
                    new_inquiries: 0,
                    new_inquiry_messages: 0,
                    imported_cards,
                    new_ads,
                    counterparty_name: None,
                    inquiry_id: None,
                });
            }
            crate::state::apply_app_notices_update(
                &mut notices_store,
                Some(now_secs),
                market_notices,
            );
            next_notices_store = Some(notices_store);
        }

        let mut batch = crate::state::EncryptedBatchCommit::new();
        if let Some(store) = next_card_store {
            batch.card_store(store);
        }
        if let Some(reg) = next_wot_reg {
            batch.wot(reg);
        }
        if address_book_changed {
            if let Some(ab) = next_address_book {
                batch.contacts(ab);
            }
        }
        if policy_changed {
            if let Some(pol) = next_security_policy {
                batch.security_policy(pol);
            }
        }
        if let Some(notices) = next_notices_store {
            batch.app_notices(notices);
        }
        batch.sync_state(sync_state);
        batch
            .commit(state, service, pw)
            .map_err(FrontendError::from)?;
        Ok(())
    })?;

    if committed.is_none() {
        return Ok(FrontendMarketSyncResult {
            status: MarketSyncStatus::SessionSuperseded,
            uploaded_to: 0,
            polled_syncs: 0,
            decrypted_slots: 0,
            imported_cards: 0,
            new_ads: 0,
            imported_wot_lists: 0,
            errors: Vec::new(),
            retry_after_secs: None,
            revoked_own_syncs: Vec::new(),
        });
    }

    let status = if skip_poll && uploaded_to == 0 {
        MarketSyncStatus::SkippedCooldown
    } else {
        MarketSyncStatus::Completed
    };

    Ok(FrontendMarketSyncResult {
        status,
        uploaded_to,
        polled_syncs,
        decrypted_slots,
        imported_cards,
        new_ads,
        imported_wot_lists,
        errors: err_tracker.errors,
        retry_after_secs: err_tracker.max_retry_after_secs,
        revoked_own_syncs,
    })
}

pub fn get_sync_overview_status_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncOverviewStatus, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let (security_policy, own_desc_opt, contacts, sync_state) =
        if pw.is_some() || service.is_session_active() {
            let security_policy = state
                .get_or_load_security_policy(&mut service, pw)
                .map_err(FrontendError::from)?;
            let admin_opt = admin::load_stored_sync_admin_opt(&mut service, pw)?;
            let own_desc_opt = crate::state::active_own_sync_descriptor(admin_opt.as_ref());
            let contacts = state
                .load_contacts(&mut service, pw)
                .map_err(FrontendError::from)?;
            let sync_state = state
                .load_sync_state_or_cached(&mut service, pw)
                .map_err(FrontendError::from)?;
            (security_policy, own_desc_opt, contacts, Some(sync_state))
        } else {
            // timeout == 0 graceful cache fallback
            let security_policy = state
                .read_cache_slot_for_service(&service, |c| &c.security_policy)
                .unwrap_or_default();
            let own_desc_opt = state
                .read_cache_slot_for_service(&service, |c| &c.own_sync_descriptor)
                .flatten();
            let contacts = state
                .read_cache_slot_for_service(&service, |c| &c.contacts)
                .map(|ab| ab.contacts.into_values().collect())
                .unwrap_or_default();
            let sync_state = state.read_cache_slot_for_service(&service, |c| &c.sync_state);
            (security_policy, own_desc_opt, contacts, sync_state)
        };

    let has_own_sync = own_desc_opt
        .as_ref()
        .is_some_and(|d| !d.write_url.is_empty());
    let pending_own_sync = own_desc_opt
        .as_ref()
        .is_some_and(|d| d.write_url.is_empty());
    let norm_own_read_url = own_desc_opt
        .as_ref()
        .map(|d| normalize_sync_url(&d.read_url))
        .unwrap_or_default();

    let friend_syncs = security_policy
        .market_selected_syncs
        .iter()
        .filter(|d| {
            normalize_sync_url(&d.read_url) != norm_own_read_url
                && !d.read_url.is_empty()
                && !d.drop_pub.is_empty()
                && d.fail_count == 0
                && d.status == "active"
        })
        .count();

    let public_syncs = security_policy
        .public_syncs
        .iter()
        .filter(|d| {
            d.enabled
                && !crate::services::sync_endpoints::is_endpoint_revoked(&d.status)
                && !d.authorized_channels.is_empty()
        })
        .count();

    let pending_public_syncs = security_policy
        .public_syncs
        .iter()
        .filter(|d| {
            !crate::services::sync_endpoints::is_endpoint_revoked(&d.status)
                && d.authorized_channels.is_empty()
        })
        .count();

    let mut pending_keys = HashSet::new();
    for c in &contacts {
        for ep in &c.syncs {
            if ep.status == "pending_auth" {
                let key = sync_endpoint_key(ep);
                if !key.is_empty() {
                    pending_keys.insert(key);
                }
            }
        }
    }
    for sel in &security_policy.market_selected_syncs {
        if sel.status == "pending_auth" {
            let key = sync_endpoint_key(sel);
            if !key.is_empty() {
                pending_keys.insert(key);
            }
        }
    }
    let pending_auth_count = pending_keys.len();

    let failing_count = security_policy
        .market_selected_syncs
        .iter()
        .filter(|d| d.fail_count > 0 || d.status == "unreachable")
        .count();

    let connected_count = (if has_own_sync { 1 } else { 0 }) + friend_syncs + public_syncs;
    let pending_count = (if pending_own_sync { 1 } else { 0 }) + pending_public_syncs;

    let last_sync_at =
        sync_state.and_then(|s| s.last_upload_at.into_iter().chain(s.last_poll_at).max());

    Ok(FrontendSyncOverviewStatus {
        connected_count,
        pending_count,
        friend_syncs,
        public_syncs,
        has_own_sync,
        last_sync_at,
        pending_auth_count,
        failing_count,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::models::{
        CardFieldVisibility, FrontendAdInput, FrontendContact, FrontendMemberPolicyDto,
        FrontendSaveMyMarketProfileRequest, FrontendSyncAdminUpdateRequest, FrontendSyncDescriptor,
        FrontendUserProfile,
    };
    use crate::services::common::get_wallet_master_seed;
    use crate::services::market_service::{
        get_market_view_impl, save_my_market_profile_impl, set_card_published_impl, upsert_ad_impl,
    };
    use crate::services::settings_service::apply_reach_preset_impl;
    use crate::state::{canonicalize_did, MARKET_CARD_STORE_KEY};
    use crate::test_support::*;

    #[tokio::test]
    async fn test_phase_3_acceptance_criteria_flow() {
        let base_dir = temp_test_dir("phase3_flow");
        let password = "TestPassword123!";

        // 1. Setup actors: Alice (Admin), Bob, Charlie, Reader L1, Reader L2
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let charlie_service = setup_test_app(
            &base_dir.join("charlie"),
            "Charlie",
            CHARLIE_PHRASE,
            password,
        );
        let l1_service = setup_test_app(&base_dir.join("l1"), "Reader1", READER1_PHRASE, password);
        let l2_service = setup_test_app(&base_dir.join("l2"), "Reader2", READER2_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);
        let charlie_state = create_test_state(charlie_service);
        let l1_state = create_test_state(l1_service);
        let l2_state = create_test_state(l2_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let charlie_did = {
            let s = charlie_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let l1_did = {
            let s = l1_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // 2. Alice prepares drop setup and finalize on MockSyncServer A
        let draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 2,
                hop2_enabled: true,
                bridge_min: 2,
                hop2_min: 2,
            }),
            Some(300),
            Some(256),
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        let server_a =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;

        let finalize_status = finalize_sync_setup_impl(
            server_a.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        assert_eq!(finalize_status.data_dir, draft.data_dir);
        assert_eq!(finalize_status.total_member_count, 1); // Alice is in allow list automatically

        // Test connection
        let conn_test = test_sync_connection_impl(
            server_a.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert!(conn_test.success);
        assert!(conn_test.write_ok);
        assert!(conn_test.read_ok);

        // 3. Alice authorizes Bob directly (Level 2). Bob authorizes Charlie (Level 2).
        {
            let mut s = alice_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
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
                syncs: vec![],
            };
            alice_state
                .save_contact(&mut s, bob_contact, Some(password))
                .unwrap();
        }

        {
            let mut s = bob_state.service.lock().unwrap();
            let charlie_contact = FrontendContact {
                did: charlie_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Charlie".to_string()),
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
            };
            bob_state
                .save_contact(&mut s, charlie_contact, Some(password))
                .unwrap();
        }

        // Bob signs his SD-JWT and Alice stores it in her raw_list_store
        let bob_sd_jwt = {
            let mut s = bob_state.service.lock().unwrap();
            let mut reg = bob_state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            bob_state.ensure_wot_list_signed(&mut s, &mut reg).unwrap();
            reg.own_signed_list.unwrap()
        };

        {
            let mut s = alice_state.service.lock().unwrap();
            let mut reg = alice_state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            let can_bob = canonicalize_did(&bob_did).unwrap();
            reg.raw_list_store.insert(can_bob, bob_sd_jwt);
            alice_state
                .save_wot_register(&mut s, &reg, Some(password))
                .unwrap();
        }

        // Alice updates drop admin config and pushes to Server A
        let updated_status = update_sync_admin_config_impl(
            FrontendSyncAdminUpdateRequest::default(),
            true,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        // Admin A authorizes B directly and C via B's SD-JWT trust list
        assert_eq!(updated_status.total_member_count, 3);
        assert_eq!(server_a.members.lock().unwrap().len(), 3);
        {
            let mut s = alice_state.service.lock().unwrap();
            let admin_cfg = admin::load_stored_sync_admin_opt(&mut s, Some(password))
                .unwrap()
                .unwrap();
            let (stats, _) = compute_admin_sync_members_with_stats(
                &alice_state,
                &mut s,
                &admin_cfg,
                "card",
                Some(password),
            )
            .unwrap();
            assert_eq!(stats.direct_count, 2); // Alice + Bob
            assert_eq!(stats.hop2_count, 1); // Charlie
        }

        // 4. Charlie discovers its own authorization via discover_available_syncs
        {
            let mut s = charlie_state.service.lock().unwrap();
            let alice_contact = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(1),
                desired_hops: Some(2),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![finalize_status.descriptor.clone()],
            };
            charlie_state
                .save_contact(&mut s, alice_contact, Some(password))
                .unwrap();
        }

        // Charlie probes contacts to check reachability and authorization on Server A
        let probe_results =
            probe_contact_syncs_impl(false, Some(password.to_string()), &charlie_state)
                .await
                .unwrap();
        assert!(!probe_results.is_empty(), "Charlie should probe Server A");

        let charlie_discovered =
            discover_available_syncs_impl(Some(password.to_string()), &charlie_state)
                .await
                .unwrap();

        assert_eq!(charlie_discovered.len(), 1);
        assert!(
            charlie_discovered[0].is_authorized,
            "Charlie should discover authorization on Server A!"
        );

        // 5. Start Server E for redundant public drop upload
        let server_e = MockSyncServer::start(
            "d-server-e".to_string(),
            "2222222222222222222222222222222222222222222222222222222222222222".to_string(),
        )
        .await;
        let server_e_url = format!("{}/sync.php", server_e.base_url);

        // Bob probes Server E
        let e_info =
            probe_public_sync_impl(server_e_url.clone(), Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert_eq!(e_info.access_pubkey.len(), 64);

        // Authorize Bob's access pubkey on Server E
        server_e
            .members
            .lock()
            .unwrap()
            .push(e_info.access_pubkey.clone());

        // Bob saves Server E as a public drop
        save_public_sync_impl(
            server_e_url.clone(),
            format!("{}/d-server-e", server_e.base_url),
            e_info.drop_pub.clone(),
            None,
            true,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();

        // Bob configures selected upload targets: Drop A
        let selected = set_selected_upload_syncs_impl(
            vec![finalize_status.descriptor.clone()],
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(selected.len(), 1);

        // Bob adds Reader 1 (L1) with level 2 (>= share_min_level 1)
        {
            let mut s = bob_state.service.lock().unwrap();
            let l1_contact = FrontendContact {
                did: l1_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Reader1".to_string()),
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
            };
            bob_state
                .save_contact(&mut s, l1_contact, Some(password))
                .unwrap();
        }

        // Bob creates and saves his own market profile and publishes it
        let save_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                last_name: Some("Baker".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![humoco_market::Skill {
                title: "Fresh Bread".to_string(),
                text: Some("Warm and crispy".to_string()),
                pricing: Some("5 M".to_string()),
                is_remote: false,
            }],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        };
        save_my_market_profile_impl(save_req, Some(password.to_string()), &bob_state).unwrap();
        set_card_published_impl(true, Some(password.to_string()), &bob_state)
            .await
            .unwrap();

        // Bob uploads redundantly to Drop A (selected) and Drop E (active public drop)
        let bob_sync =
            sync_market_channels_impl(None, Some(true), Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert_eq!(
            bob_sync.uploaded_to, 4,
            "Bob should upload card and wot channels to both Drop A and Drop E"
        );
        assert_eq!(server_a.slots.lock().unwrap().len(), 2);
        assert_eq!(server_e.slots.lock().unwrap().len(), 2);

        // 6. Authorized reader L1 decrypts Bob's slot and imports cards and wot list
        {
            let mut s = l1_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
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
                syncs: vec![finalize_status.descriptor.clone()],
            };
            l1_state
                .save_contact(&mut s, bob_contact, Some(password))
                .unwrap();
        }

        let l1_sync = sync_market_channels_impl(None, None, Some(password.to_string()), &l1_state)
            .await
            .unwrap();
        assert_eq!(
            l1_sync.decrypted_slots, 2,
            "Authorized reader L1 should decrypt both card and wot slots"
        );
        assert_eq!(
            l1_sync.imported_cards, 1,
            "Authorized reader L1 should import card"
        );
        assert_eq!(
            l1_sync.imported_wot_lists, 1,
            "Authorized reader L1 should import wot list"
        );

        // Verify Bob's card appears in L1's market view with Drop entry source
        let l1_market = get_market_view_impl(Some(password.to_string()), &l1_state).unwrap();
        let can_bob_did = canonicalize_did(&bob_did).unwrap();
        let bob_person = l1_market.people.iter().find(|p| p.did == can_bob_did);
        assert!(bob_person.is_some(), "Bob should be in L1 market view");
        let has_sync_source = bob_person.unwrap().sources.iter().any(|s| {
            matches!(s, crate::models::FrontendEntrySource::Sync { read_url } if *read_url == finalize_status.descriptor.read_url)
        });
        assert!(
            has_sync_source,
            "Bob's market person should have Drop entry source!"
        );

        // 7. Unauthorized reader L2 gets 0 decrypted slots
        {
            let mut s = l2_state.service.lock().unwrap();
            l2_state
                .mutate_security_policy(&mut s, Some(password), |policy| {
                    policy.market_selected_syncs = vec![finalize_status.descriptor.clone()];
                })
                .unwrap();
        }

        let l2_sync = sync_market_channels_impl(None, None, Some(password.to_string()), &l2_state)
            .await
            .unwrap();
        assert_eq!(
            l2_sync.decrypted_slots, 0,
            "Unauthorized reader L2 should decrypt 0 slots"
        );
        assert_eq!(
            l2_sync.imported_cards, 0,
            "Unauthorized reader L2 should import 0 cards"
        );

        // 8. Withdrawing trust from Bob and pushing removes Bob's stealth pubkey from members and purges slots
        let bob_stealth_pk_a = {
            let mut s = alice_state.service.lock().unwrap();
            let seed = get_wallet_master_seed(&mut s, Some(password)).unwrap();
            let (admin_sk, _, _) = humoco_market::derive_drop_admin_keypair(&seed, 0);
            let bob_vk = human_money_core::crypto::get_pubkey_from_user_id(&bob_did).unwrap();
            humoco_market::derive_member_stealth_pubkey_admin(&admin_sk, &bob_vk).unwrap()
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
                        trust_level: Some(0),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![],
                    },
                    Some(password),
                )
                .unwrap();
        }
        let _revoked_push_status = update_sync_admin_config_impl(
            FrontendSyncAdminUpdateRequest::default(),
            true,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        {
            let mut s = alice_state.service.lock().unwrap();
            let admin_after = admin::load_stored_sync_admin_opt(&mut s, Some(password))
                .unwrap()
                .unwrap();
            let (_, active_pks) = admin::compute_admin_sync_members_with_stats(
                &alice_state,
                &mut s,
                &admin_after,
                "card",
                Some(password),
            )
            .unwrap();
            assert!(!active_pks.contains(&bob_stealth_pk_a));
        }
        assert!(!server_a.members.lock().unwrap().contains(&bob_stealth_pk_a));
        assert!(
            !server_a
                .slots
                .lock()
                .unwrap()
                .contains_key(&format!("{}.card", bob_stealth_pk_a)),
            "Slot file should be purged on Server A"
        );
        assert!(
            !server_a
                .slots
                .lock()
                .unwrap()
                .contains_key(&format!("{}_pc.card", bob_stealth_pk_a)),
            "Prefixed slot file should be purged on Server A"
        );
        assert!(
            !server_a
                .slots
                .lock()
                .unwrap()
                .contains_key(&format!("{}.wot", bob_stealth_pk_a)),
            "WoT slot file should be purged on Server A"
        );
        assert!(
            !server_a
                .slots
                .lock()
                .unwrap()
                .contains_key(&format!("{}_pc.wot", bob_stealth_pk_a)),
            "Prefixed WoT slot file should be purged on Server A"
        );

        // 9. Check overview status
        let overview =
            get_sync_overview_status_impl(Some(password.to_string()), &alice_state).unwrap();
        assert!(overview.has_own_sync);
        assert_eq!(overview.connected_count, 1);
        assert_eq!(overview.pending_count, 0);
    }

    #[test]
    fn test_get_sync_overview_status_pending_vs_connected() {
        let base_dir = temp_test_dir("overview_status");
        let password = "TestPassword123!";
        let service = setup_test_app(&base_dir.join("user"), "User", ALICE_PHRASE, password);
        let state = create_test_state(service);

        // 1. Initial state: Disconnected
        let initial = get_sync_overview_status_impl(Some(password.to_string()), &state).unwrap();
        assert_eq!(initial.connected_count, 0);
        assert_eq!(initial.pending_count, 0);
        assert_eq!(initial.public_syncs, 0);

        // 2. Add public drop with empty authorized_channels (pending / unauthorized)
        {
            let mut s = state.service.lock().unwrap();
            state
                .mutate_security_policy(&mut s, Some(password), |policy| {
                    policy.public_syncs.push(crate::settings::StoredPublicSync {
                        write_url: "https://sync.example.com/sync.php".to_string(),
                        read_url: "https://sync.example.com/data".to_string(),
                        drop_pub: "a".repeat(64),
                        register_url: None,
                        access_pubkey: "b".repeat(64),
                        enabled: false,
                        authorized_channels: vec![],
                        ..Default::default()
                    });
                })
                .unwrap();
        }

        let pending_overview =
            get_sync_overview_status_impl(Some(password.to_string()), &state).unwrap();
        assert_eq!(pending_overview.connected_count, 0);
        assert_eq!(pending_overview.pending_count, 1);
        assert_eq!(pending_overview.public_syncs, 0);

        // 3. Mark public drop as authorized and enabled
        {
            let mut s = state.service.lock().unwrap();
            state
                .mutate_security_policy(&mut s, Some(password), |policy| {
                    policy.public_syncs[0].enabled = true;
                    policy.public_syncs[0].authorized_channels =
                        vec!["card".to_string(), "wot".to_string()];
                })
                .unwrap();
        }

        let connected_overview =
            get_sync_overview_status_impl(Some(password.to_string()), &state).unwrap();
        assert_eq!(connected_overview.connected_count, 1);
        assert_eq!(connected_overview.pending_count, 0);
        assert_eq!(connected_overview.public_syncs, 1);
    }

    #[tokio::test]
    async fn test_probe_contact_syncs_lifecycle_and_auto_activation() {
        let base_dir = temp_test_dir("probe_lifecycle");
        let password = "TestPassword123!";
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let bob_state = create_test_state(bob_service);

        let server_admin_pub = crate::test_support::test_ed25519_pub_hex(0x11);
        let server =
            MockSyncServer::start("alice_data".to_string(), server_admin_pub.clone()).await;

        // 1. Initially Bob has no syncs in security_policy
        {
            let mut s = bob_state.service.lock().unwrap();
            let policy = bob_state
                .load_security_policy(&mut s, Some(password))
                .unwrap();
            assert!(policy.market_selected_syncs.is_empty());
        }

        // 2. Add Alice contact with unverified sync
        let alice_did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let unverified_sync = FrontendSyncDescriptor {
            write_url: format!("{}/sync.php", server.base_url),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let alice_contact = FrontendContact {
            did: alice_did.to_string(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![unverified_sync],
        };
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .save_contact(&mut s, alice_contact, Some(password))
                .unwrap();
        }

        // 3. First probe: Bob not in server members -> becomes pending_auth, NOT auto-activated
        let probed = probe_contact_syncs_impl(false, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(probed.len(), 1);
        assert_eq!(probed[0].status, "pending_auth");
        assert_eq!(probed[0].drop_pub, server_admin_pub);
        assert!(probed[0].last_probed_at.is_some());

        // Verify security_policy still empty (no auto-activation for pending_auth)
        {
            let mut s = bob_state.service.lock().unwrap();
            let policy = bob_state
                .load_security_policy(&mut s, Some(password))
                .unwrap();
            assert!(policy.market_selected_syncs.is_empty());
        }

        // 4. Backoff check: immediate re-probe with force=false skips pending_auth
        let initial_probed_at = probed[0].last_probed_at;
        let reprobed = probe_contact_syncs_impl(false, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(reprobed[0].status, "pending_auth");
        assert_eq!(reprobed[0].last_probed_at, initial_probed_at);

        // 5. Authorize Bob on the mock drop server
        let bob_signing_key = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet_and_identity(|_w, id| id.signing_key.clone())
                .unwrap()
        };
        let stealth_pk =
            humoco_market::derive_member_stealth_pubkey(&bob_signing_key, &server_admin_pub)
                .unwrap();
        server.members.lock().unwrap().push(stealth_pk);

        // 6. Force probe: Bob is now authorized -> becomes active and auto-activates into selected syncs
        let active_probed = probe_contact_syncs_impl(true, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(active_probed.len(), 1);
        assert_eq!(active_probed[0].status, "active");

        {
            let mut s = bob_state.service.lock().unwrap();
            let policy = bob_state
                .load_security_policy(&mut s, Some(password))
                .unwrap();
            assert_eq!(policy.market_selected_syncs.len(), 1);
            assert_eq!(policy.market_selected_syncs[0].status, "active");
            assert_eq!(policy.market_selected_syncs[0].drop_pub, server_admin_pub);
        }

        // 7. Test unreachable handling with invalid endpoint
        let charlie_service = setup_test_app(
            &base_dir.join("charlie"),
            "Charlie",
            CHARLIE_PHRASE,
            password,
        );
        let charlie_did = charlie_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let unreachable_sync = FrontendSyncDescriptor {
            write_url: "http://127.0.0.1:9/sync.php".to_string(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let charlie_contact = FrontendContact {
            did: charlie_did,
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(1),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![unreachable_sync],
        };
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .save_contact(&mut s, charlie_contact, Some(password))
                .unwrap();
        }

        let all_probed = probe_contact_syncs_impl(true, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        let unreach = all_probed
            .iter()
            .find(|s| s.write_url.contains(":9/sync.php"))
            .unwrap();
        assert_eq!(unreach.status, "unreachable");
        assert_eq!(unreach.fail_count, 1);
        assert!(unreach.last_probed_at.is_some());
    }

    #[tokio::test]
    async fn test_local_validation_error_does_not_penalize_contact_sync_health() {
        let base_dir = temp_test_dir("local_val_no_health_penalty");
        let password = "TestPassword123!";
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let bob_state = create_test_state(bob_service);
        let charlie_service = setup_test_app(
            &base_dir.join("charlie"),
            "Charlie",
            CHARLIE_PHRASE,
            password,
        );
        let charlie_did = charlie_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let placeholder_sync = FrontendSyncDescriptor {
            write_url: "https://contact.example.org/sync.php".to_string(),
            read_url: "https://contact.example.org/d-test".to_string(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: Some(1_700_000_000),
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let charlie_contact = FrontendContact {
            did: charlie_did.clone(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(1),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![placeholder_sync],
        };
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .save_contact(&mut s, charlie_contact, Some(password))
                .unwrap();
            let mut ab: crate::models::FrontendAddressBook =
                crate::state::load_encrypted_json_or_default(
                    &mut s,
                    crate::state::CONTACTS_DATA_NAME,
                    Some(password),
                )
                .unwrap();
            let ep = &mut ab.contacts.get_mut(&charlie_did).unwrap().syncs[0];
            ep.write_url = "http://10.255.255.1/sync.php".to_string();
            ep.read_url = "http://10.255.255.1/d-test".to_string();
            let mut batch = crate::state::EncryptedBatchCommit::new();
            batch.contacts(ab);
            batch.commit(&bob_state, &mut s, Some(password)).unwrap();
        }

        let _ = probe_contact_syncs_impl(true, Some(password.to_string()), &bob_state)
            .await
            .unwrap();

        let ab: crate::models::FrontendAddressBook = {
            let mut s = bob_state.service.lock().unwrap();
            crate::state::load_encrypted_json_or_default(
                &mut s,
                crate::state::CONTACTS_DATA_NAME,
                Some(password),
            )
            .unwrap()
        };
        let saved_ep = &ab.contacts[&charlie_did].syncs[0];
        assert_eq!(
            saved_ep.status, "active",
            "Local SSRF/URL validation error must not mark server unreachable"
        );
        assert_eq!(saved_ep.fail_count, 0);
        assert!(saved_ep.last_probed_at.is_none());

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn test_probe_contact_syncs_healthy_sync_probes_unverified_once_without_reprobing_pending(
    ) {
        let base_dir = temp_test_dir("healthy_probe_unverified");
        let password = "TestPassword123!";
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let bob_state = create_test_state(bob_service);

        let server_admin_pub = crate::test_support::test_ed25519_pub_hex(0x22);
        let server =
            MockSyncServer::start("server_data".to_string(), server_admin_pub.clone()).await;

        // 1. Give Bob an active, healthy friend sync in settings
        let healthy_sync = FrontendSyncDescriptor {
            write_url: "https://healthy.example.com/sync.php".to_string(),
            read_url: "https://healthy.example.com/read".to_string(),
            slot_id: None,
            drop_pub: crate::test_support::test_ed25519_pub_hex(0x23),
            last_seen_at: Some(chrono::Utc::now().timestamp() as u64),
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: Some(chrono::Utc::now().timestamp() as u64),
            ..Default::default()
        };
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .mutate_security_policy(&mut s, Some(password), |policy| {
                    policy.market_selected_syncs.push(healthy_sync);
                })
                .unwrap();
        }

        // 2. Add Alice contact with an already-probed "pending_auth" sync
        let alice_did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let pending_sync = FrontendSyncDescriptor {
            write_url: "http://127.0.0.1:9/pending.php".to_string(),
            read_url: "http://127.0.0.1:9/pending_read".to_string(),
            slot_id: None,
            drop_pub: crate::test_support::test_ed25519_pub_hex(0x24),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "pending_auth".to_string(),
            last_probed_at: Some(1000),
            ..Default::default()
        };

        // 3. Add Charlie contact with a newly added "unverified" sync (last_probed_at == None, read_url == "")
        let charlie_service = setup_test_app(
            &base_dir.join("charlie"),
            "Charlie",
            CHARLIE_PHRASE,
            password,
        );
        let charlie_did = charlie_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let unverified_sync = FrontendSyncDescriptor {
            write_url: format!("{}/sync.php", server.base_url),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        let alice_contact = FrontendContact {
            did: alice_did.to_string(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![pending_sync],
        };
        let charlie_contact = FrontendContact {
            did: charlie_did.to_string(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![unverified_sync],
        };
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .save_contact(&mut s, alice_contact, Some(password))
                .unwrap();
            bob_state
                .save_contact(&mut s, charlie_contact, Some(password))
                .unwrap();
        }

        // 4. Probe with force=false.
        // Even though Bob already has a healthy sync, Charlie's unverified sync MUST be probed once!
        // Meanwhile, Alice's already-probed pending_auth sync must NOT be re-probed (retaining last_probed_at == 1000).
        let probed = probe_contact_syncs_impl(false, Some(password.to_string()), &bob_state)
            .await
            .unwrap();

        let alice_probed = probed
            .iter()
            .find(|s| s.write_url.contains("pending.php"))
            .unwrap();
        assert_eq!(alice_probed.status, "pending_auth");
        assert_eq!(
            alice_probed.last_probed_at,
            Some(1000),
            "Already probed pending sync should not be re-probed when has_healthy_sync"
        );

        let charlie_probed = probed
            .iter()
            .find(|s| s.write_url.contains(&server.base_url))
            .unwrap();
        assert_ne!(
            charlie_probed.status, "unverified",
            "Newly added unverified sync must be probed"
        );
        assert_eq!(
            charlie_probed.drop_pub, server_admin_pub,
            "Drop pub must be populated from probe info"
        );
        assert!(
            !charlie_probed.read_url.is_empty(),
            "Read URL must be populated from probe info"
        );
        assert!(
            charlie_probed.last_probed_at.is_some(),
            "last_probed_at must be populated"
        );
    }

    #[test]
    fn test_preview_contact_trust_change() {
        let base_dir = temp_test_dir("trust_preview");
        let password = "TestPassword123!";
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        // Prepare drop setup on Alice with direct_min 2
        let _draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 2,
                hop2_enabled: true,
                bridge_min: 2,
                hop2_min: 2,
            }),
            Some(300),
            Some(256),
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        // 1. Initially Bob has no trust level (0)
        let preview_upgrade = preview_contact_trust_change_impl(
            bob_did.clone(),
            2,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert!(!preview_upgrade.can_read_my_market_before);
        assert!(preview_upgrade.can_read_my_market_after);
        assert!(!preview_upgrade.can_write_my_sync_before);
        assert!(preview_upgrade.can_write_my_sync_after);
        assert!(!preview_upgrade.can_send_me_post_before);
        assert!(preview_upgrade.can_send_me_post_after);

        // 2. Save Bob with level 2
        {
            let mut s = alice_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
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
                syncs: vec![],
            };
            alice_state
                .save_contact(&mut s, bob_contact, Some(password))
                .unwrap();
        }

        // Preview downgrade to level 0
        let preview_downgrade = preview_contact_trust_change_impl(
            bob_did.clone(),
            0,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert!(preview_downgrade.can_read_my_market_before);
        assert!(!preview_downgrade.can_read_my_market_after);
        assert!(preview_downgrade.can_read_my_wot_before);
        assert!(!preview_downgrade.can_read_my_wot_after);
        assert!(preview_downgrade.can_write_my_sync_before);
        assert!(!preview_downgrade.can_write_my_sync_after);
        assert!(preview_downgrade.can_send_me_post_before);
        assert!(!preview_downgrade.can_send_me_post_after);
    }

    #[tokio::test]
    async fn test_wot_channel_sync_flow_and_idempotency() {
        let base_dir = temp_test_dir("wot_sync_flow");
        let password = "TestPassword123!";

        // 1. Setup actors: Alice (Hoster/Admin), Bob (Direct Friend, Level 2), Carol (Hop-2 from Alice via Bob, Level 2), David (Outsider)
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let carol_service =
            setup_test_app(&base_dir.join("carol"), "Carol", CHARLIE_PHRASE, password);
        let david_service =
            setup_test_app(&base_dir.join("david"), "David", READER1_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);
        let carol_state = create_test_state(carol_service);
        let david_state = create_test_state(david_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let carol_did = {
            let s = carol_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // 2. Alice sets up her drop
        let draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 2,
                hop2_enabled: true,
                bridge_min: 2,
                hop2_min: 2,
            }),
            Some(300),
            Some(256),
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;

        let finalize_status = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        // Configure Alice's drop as target drop for Alice, Bob, and Carol
        for st in [&alice_state, &bob_state, &carol_state] {
            let mut s = st.service.lock().unwrap();
            st.mutate_security_policy(&mut s, Some(password), |policy| {
                policy.market_selected_syncs = vec![finalize_status.descriptor.clone()];
            })
            .unwrap();
        }

        // 3. Establish Trust Network:
        // Alice rates Bob as Level 2 (Direct, class 1)
        {
            let mut s = alice_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
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
                syncs: vec![],
            };
            alice_state
                .save_contact(&mut s, bob_contact, Some(password))
                .unwrap();
        }

        // Bob rates Alice as Level 2 (so Bob recognizes Alice) and Carol as Level 2
        {
            let mut s = bob_state.service.lock().unwrap();
            let alice_contact = FrontendContact {
                did: alice_did.clone(),
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
                syncs: vec![],
            };
            bob_state
                .save_contact(&mut s, alice_contact, Some(password))
                .unwrap();

            let carol_contact = FrontendContact {
                did: carol_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Carol".to_string()),
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
            };
            bob_state
                .save_contact(&mut s, carol_contact, Some(password))
                .unwrap();
        }

        // Carol rates Bob as Level 2 and Alice as Level 1
        {
            let mut s = carol_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
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
                syncs: vec![],
            };
            carol_state
                .save_contact(&mut s, bob_contact, Some(password))
                .unwrap();

            let alice_contact = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(1),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            carol_state
                .save_contact(&mut s, alice_contact, Some(password))
                .unwrap();
        }

        // Alice learns Bob's list so Carol is in Alice's graph as reachable Hop-2:
        {
            let can_alice = canonicalize_did(&alice_did).unwrap();
            let can_bob = canonicalize_did(&bob_did).unwrap();

            let mut bob_s = bob_state.service.lock().unwrap();
            let mut bob_reg = bob_state
                .get_or_load_wot_register(&mut bob_s, Some(password))
                .unwrap();
            bob_state
                .ensure_wot_list_signed(&mut bob_s, &mut bob_reg)
                .unwrap();
            let bob_sync_pkg = bob_reg
                .prepare_sync_package(Some(&can_alice), None, true)
                .unwrap()
                .0;

            let mut alice_s = alice_state.service.lock().unwrap();
            let mut alice_reg = alice_state
                .get_or_load_wot_register(&mut alice_s, Some(password))
                .unwrap();
            let now = sync_client::current_unix_timestamp();
            let stats = alice_reg
                .import_sync_package(&bob_sync_pkg, Some(&can_bob), now)
                .unwrap();
            assert!(stats.accepted_total() >= 1);
            alice_state
                .save_wot_register(&mut alice_s, &alice_reg, Some(password))
                .unwrap();
        }

        // Verify Alice's authorized drop readers: Bob (class 1) and Carol (class 2)
        {
            let mut alice_s = alice_state.service.lock().unwrap();
            let alice_reg = alice_state
                .get_or_load_wot_register(&mut alice_s, Some(password))
                .unwrap();
            let readers = alice_reg.authorized_drop_readers();
            let can_bob = canonicalize_did(&bob_did).unwrap();
            let can_carol = canonicalize_did(&carol_did).unwrap();
            assert!(
                readers.iter().any(|(d, c)| d == &can_bob && *c == 1),
                "Bob must be in class 1"
            );
            assert!(
                readers.iter().any(|(d, c)| d == &can_carol && *c == 2),
                "Carol must be in class 2"
            );
        }

        // 4. Alice Syncs (Uploads "wot" channel to drop server)
        let alice_sync_res =
            sync_market_channels_impl(None, Some(true), Some(password.to_string()), &alice_state)
                .await
                .unwrap();
        assert!(
            alice_sync_res.uploaded_to >= 1,
            "Alice must upload to drop server"
        );
        assert!(
            alice_sync_res.errors.is_empty(),
            "Alice sync should have 0 errors: {:?}",
            alice_sync_res.errors
        );

        // Verify slot exists on mock drop server and has multi-class magic
        let slot_keys: Vec<String> = server.slots.lock().unwrap().keys().cloned().collect();
        let wot_slot_key = slot_keys
            .iter()
            .find(|k| k.ends_with(".wot"))
            .expect("Wot slot must exist on server");
        let wot_slot_bytes = server
            .slots
            .lock()
            .unwrap()
            .get(wot_slot_key)
            .unwrap()
            .clone();
        assert_eq!(&wot_slot_bytes[0..8], humoco_market::SLOT_MULTI_MAGIC);

        // 5. Bob (Class 1) Syncs (Downloads Alice's list)
        let bob_sync_res =
            sync_market_channels_impl(None, None, Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert!(
            bob_sync_res.imported_wot_lists >= 1,
            "Bob must import Alice's wot list: {:?}",
            bob_sync_res
        );
        assert!(
            bob_sync_res.errors.is_empty(),
            "Bob sync should have 0 errors: {:?}",
            bob_sync_res.errors
        );

        // Verify Bob's register now has Alice's list
        {
            let mut bob_s = bob_state.service.lock().unwrap();
            let bob_reg = bob_state
                .get_or_load_wot_register(&mut bob_s, Some(password))
                .unwrap();
            let can_alice = canonicalize_did(&alice_did).unwrap();
            assert!(
                bob_reg.raw_list_store.contains_key(&can_alice),
                "Bob must have Alice's raw list"
            );
        }

        // Bob second sync before any new uploads -> strict idempotency (0 new imports)
        let bob_idempotent_sync =
            sync_market_channels_impl(None, None, Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert_eq!(
            bob_idempotent_sync.imported_wot_lists, 0,
            "Bob second sync before new uploads must be 0 imports"
        );

        // 6. Carol (Class 2) Syncs (Downloads Alice's list with Selective Disclosure)
        let carol_sync_res =
            sync_market_channels_impl(None, None, Some(password.to_string()), &carol_state)
                .await
                .unwrap();
        assert!(
            carol_sync_res.imported_wot_lists >= 1,
            "Carol must import Alice's wot list"
        );
        assert!(
            carol_sync_res.errors.is_empty(),
            "Carol sync should have 0 errors: {:?}",
            carol_sync_res.errors
        );

        // Verify Carol's register now has Alice's list
        {
            let mut carol_s = carol_state.service.lock().unwrap();
            let carol_reg = carol_state
                .get_or_load_wot_register(&mut carol_s, Some(password))
                .unwrap();
            let can_alice = canonicalize_did(&alice_did).unwrap();
            assert!(
                carol_reg.raw_list_store.contains_key(&can_alice),
                "Carol must have Alice's raw list"
            );
        }

        // Carol second sync -> strict idempotency (0 new imports)
        let carol_idempotent_sync =
            sync_market_channels_impl(None, None, Some(password.to_string()), &carol_state)
                .await
                .unwrap();
        assert_eq!(
            carol_idempotent_sync.imported_wot_lists, 0,
            "Carol second sync must be 0 imports"
        );

        // 7. Alice second sync -> no new upload needed
        let alice_second_sync =
            sync_market_channels_impl(None, None, Some(password.to_string()), &alice_state)
                .await
                .unwrap();
        assert_eq!(
            alice_second_sync.uploaded_to, 0,
            "Alice second sync without changes must be no-op upload"
        );

        // Bob now syncs to pick up Carol's newly uploaded list
        let bob_third_sync =
            sync_market_channels_impl(None, None, Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert_eq!(
            bob_third_sync.imported_wot_lists, 1,
            "Bob should import Carol's list"
        );

        // Bob fourth sync -> again fully idempotent
        let bob_fourth_sync =
            sync_market_channels_impl(None, None, Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert_eq!(
            bob_fourth_sync.imported_wot_lists, 0,
            "Bob fourth sync must be 0 imports"
        );

        // 8. Outsider (David) cannot decrypt Alice's wot slot
        {
            let david_s = david_state.service.lock().unwrap();
            let david_sk = david_s
                .with_wallet_and_identity(|_w, id| id.signing_key.clone())
                .unwrap();
            let david_x_sk = human_money_core::crypto::ed25519_sk_to_x25519_sk(&david_sk);
            let decrypted =
                humoco_market::open_multi_class_slot_blob(&wot_slot_bytes, &david_x_sk).unwrap();
            assert_eq!(
                decrypted, None,
                "Outsider cannot decrypt any subpackage in multi-class slot blob"
            );
        }
    }

    #[tokio::test]
    async fn test_push_channel_members_hash_check_and_trust_change() {
        let base_dir = temp_test_dir("members_hash_check");
        let password = "TestPassword123!";

        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // 1. Prepare drop setup and finalize
        let draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 2,
                hop2_enabled: true,
                bridge_min: 2,
                hop2_min: 2,
            }),
            Some(300),
            Some(256),
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;

        let finalize_status = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        assert_eq!(finalize_status.data_dir, draft.data_dir);

        // Finalize pushes the 5 channels (card, wot, post, backup, lock)
        let initial_puts = *server.put_members_count.lock().unwrap();
        assert_eq!(
            initial_puts, 5,
            "Initial finalize should push all 5 channels"
        );

        // 2. Call sync_channel_members_impl immediately without any changes
        let pushed_again = sync_channel_members_impl(Some(password), &alice_state)
            .await
            .unwrap();
        assert_eq!(
            pushed_again, 0,
            "No channels should be pushed when members hash has not changed"
        );
        assert_eq!(
            *server.put_members_count.lock().unwrap(),
            initial_puts,
            "Server should receive zero new PUT requests"
        );

        // 3. Add Bob as a trusted contact (Level 2)
        let bob_contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".into(),
            notes: None,
            trust_level: Some(2),
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };

        {
            let mut service = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(&mut service, bob_contact.clone(), Some(password))
                .unwrap();
        }

        // 4. Call sync_channel_members_impl after trust level update
        let pushed_after_add = sync_channel_members_impl(Some(password), &alice_state)
            .await
            .unwrap();
        assert!(
            pushed_after_add > 0,
            "Channels with modified members should be pushed"
        );
        let puts_after_add = *server.put_members_count.lock().unwrap();
        assert!(
            puts_after_add > initial_puts,
            "Server should have received new PUT requests"
        );

        // 5. Calling again immediately should skip
        let pushed_noop = sync_channel_members_impl(Some(password), &alice_state)
            .await
            .unwrap();
        assert_eq!(
            pushed_noop, 0,
            "Subsequent call without changes must be skipped"
        );
        assert_eq!(*server.put_members_count.lock().unwrap(), puts_after_add);

        // 6. Change Bob's trust level from 2 to 1 (below direct_min=2 for drop access)
        let mut bob_untrusted = bob_contact.clone();
        bob_untrusted.trust_level = Some(1);
        {
            let mut service = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(&mut service, bob_untrusted, Some(password))
                .unwrap();
        }

        let pushed_after_downgrade = sync_channel_members_impl(Some(password), &alice_state)
            .await
            .unwrap();
        assert!(
            pushed_after_downgrade > 0,
            "Trust change must trigger channel member re-upload"
        );
        let puts_after_downgrade = *server.put_members_count.lock().unwrap();
        assert!(
            puts_after_downgrade > puts_after_add,
            "Server must receive updated members PUT"
        );
    }

    fn build_signed_test_card_envelope(
        seed_byte: u8,
        v: u64,
        valid_until: u64,
        item_count: usize,
        hops: u8,
    ) -> humoco_market::CardEnvelope {
        use ed25519_dalek::SigningKey;
        use rand::rngs::StdRng;
        use rand::{RngCore, SeedableRng};

        let sk = SigningKey::from_bytes(&[seed_byte; 32]);
        let did =
            human_money_core::crypto::create_user_id(&sk.verifying_key(), Some("tx")).unwrap();
        let mut rng = StdRng::seed_from_u64((seed_byte as u64) * 1000 + v);
        let mut items = Vec::with_capacity(item_count + 1);
        items.push((
            humoco_market::CardItem::Name {
                first_name: Some(format!("Maker-{}", seed_byte)),
                last_name: Some(format!("V{}", v)),
            },
            1,
        ));
        for idx in 0..item_count {
            let mut entropy = [0u8; 24];
            rng.fill_bytes(&mut entropy);
            let title = format!("Craft-{}-{}", idx, hex::encode(&entropy[..6]));
            let desc = format!("Local work batch {}", hex::encode(&entropy[6..24]));
            items.push((
                humoco_market::CardItem::Skill(humoco_market::Skill {
                    title,
                    text: Some(desc),
                    pricing: Some("10 M/h".to_string()),
                    is_remote: idx % 2 == 0,
                }),
                1,
            ));
        }

        let card = humoco_market::sign_sd_card(
            humoco_market::CardPayloadInput {
                did,
                v,
                valid_until,
                max_hops: 3,
                ad_count: 0,
                allow_inquiries: true,
                drops: vec![],
            },
            items,
            &sk,
        )
        .unwrap();
        let wire =
            humoco_market::render_wire(&card, hops, humoco_market::WireFilter::Full).unwrap();
        humoco_market::CardEnvelope {
            card_wire: wire,
            hops,
        }
    }

    #[tokio::test]
    async fn test_base_and_delta_card_bundle_and_compaction_on_new_reader() {
        let base_dir = temp_test_dir("base_delta_compaction");
        let password = "TestPassword123!";

        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let r1_service = setup_test_app(&base_dir.join("r1"), "Reader1", READER1_PHRASE, password);
        let r2_service = setup_test_app(&base_dir.join("r2"), "Reader2", READER2_PHRASE, password);

        let bob_state = create_test_state(bob_service);
        let r1_state = create_test_state(r1_service);
        let r2_state = create_test_state(r2_service);

        let r1_did = r1_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let r2_did = r2_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let draft = prepare_sync_setup_impl(
            None,
            None,
            None,
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .unwrap();
        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let finalize_status = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();

        set_selected_upload_syncs_impl(
            vec![finalize_status.descriptor.clone()],
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();

        // Bob trusts Reader 1 initially and disables WoT share so we assert pure card upload counts cleanly
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .mutate_ui_preferences(&mut s, Some(password), |prefs| {
                    prefs.wot_share_via_sync = false;
                })
                .unwrap();
            bob_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: r1_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("Reader1".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".into(),
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
        }
        sync_channel_members_impl(Some(password), &bob_state)
            .await
            .unwrap();

        // Populate Bob's CardStore with 18 rich cards (~8-10 KB compressed Base)
        let now_secs = sync_client::current_unix_timestamp();
        let valid_until = now_secs + 365 * 86_400;
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut store = humoco_market::CardStore::default();
            for seed in 20u8..38u8 {
                let env = build_signed_test_card_envelope(seed, now_secs, valid_until, 5, 1);
                humoco_market::accept_envelope(&mut store, &env, now_secs, 2000, None, &|_| false);
            }
            let bytes = serde_json::to_vec(&store).unwrap();
            s.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, Some(password))
                .unwrap();
        }

        // 1. Cold upload creates Base card slot (no delta yet)
        let first_up =
            sync_market_channels_impl(None, Some(true), Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert_eq!(first_up.uploaded_to, 1);

        let keys_after_base: Vec<String> = server.slots.lock().unwrap().keys().cloned().collect();
        assert_eq!(keys_after_base.len(), 1);
        let base_key = keys_after_base[0].clone();
        assert!(!base_key.contains("delta"));
        let base_bytes_before = server.slots.lock().unwrap().get(&base_key).unwrap().clone();
        assert!(
            base_bytes_before.len() > 4096,
            "Base should be large enough for <25% delta"
        );

        // 2. Update 1 small card -> Delta upload ONLY (Base bytes untouched!)
        let updated_v = now_secs + 120;
        let updated_env = build_signed_test_card_envelope(20, updated_v, valid_until, 0, 1);
        let updated_did = humoco_market::verify_sd_card(&updated_env.card_wire)
            .unwrap()
            .payload
            .did;
        {
            let mut s = bob_state.service.lock().unwrap();
            let raw = s
                .load_encrypted_data(MARKET_CARD_STORE_KEY, Some(password))
                .unwrap();
            let mut store: humoco_market::CardStore = serde_json::from_slice(&raw).unwrap();
            humoco_market::accept_envelope(
                &mut store,
                &updated_env,
                updated_v,
                2000,
                None,
                &|_| false,
            );
            let bytes = serde_json::to_vec(&store).unwrap();
            s.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, Some(password))
                .unwrap();
        }

        let delta_up =
            sync_market_channels_impl(None, None, Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert_eq!(delta_up.uploaded_to, 1);
        let keys_after_delta: Vec<String> = server.slots.lock().unwrap().keys().cloned().collect();
        assert_eq!(
            keys_after_delta.len(),
            2,
            "Server must hold Base + Delta slots"
        );
        assert!(keys_after_delta.iter().any(|k| k.contains("-delta.card")));
        assert_eq!(
            server
                .slots
                .lock()
                .unwrap()
                .get(&base_key)
                .unwrap()
                .as_slice(),
            base_bytes_before.as_slice(),
            "Base slot must stay unchanged on Delta upload"
        );

        // 3. Reader 1 syncs and gets both Base and Delta (including updated_v), and subsequent background poll hits 20m cooldown
        {
            let mut s = r1_state.service.lock().unwrap();
            r1_state
                .mutate_security_policy(&mut s, Some(password), |policy| {
                    policy.market_selected_syncs = vec![finalize_status.descriptor.clone()];
                })
                .unwrap();
        }
        let r1_res = sync_market_channels_impl(None, None, Some(password.to_string()), &r1_state)
            .await
            .unwrap();
        assert_eq!(
            r1_res.decrypted_slots, 2,
            "Reader 1 must decrypt Base and Delta slots"
        );
        assert_eq!(r1_res.imported_cards, 19);
        {
            let mut s = r1_state.service.lock().unwrap();
            let raw = s
                .load_encrypted_data(MARKET_CARD_STORE_KEY, Some(password))
                .unwrap();
            let r1_store: humoco_market::CardStore = serde_json::from_slice(&raw).unwrap();
            assert_eq!(r1_store.cards.len(), 18);
            let imported = r1_store
                .cards
                .values()
                .find(|c| c.card.payload.did == updated_did)
                .unwrap();
            assert_eq!(imported.card.payload.v, updated_v);
        }

        let r1_cooldown = sync_market_channels_with_options(
            None,
            None,
            Some(true),
            Some(password.to_string()),
            &r1_state,
        )
        .await
        .unwrap();
        assert_eq!(
            r1_cooldown.polled_syncs, 0,
            "Background poll inside 20m cooldown must skip download polling"
        );

        // 4. Bob authorizes Reader 2 -> readers_hash changes -> Compaction (new Base + DELETE Delta)
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: r2_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("Reader2".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".into(),
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
        }
        sync_channel_members_impl(Some(password), &bob_state)
            .await
            .unwrap();

        let compact_up =
            sync_market_channels_impl(None, Some(true), Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert_eq!(compact_up.uploaded_to, 1);
        let keys_after_compact: Vec<String> =
            server.slots.lock().unwrap().keys().cloned().collect();
        assert_eq!(
            keys_after_compact.len(),
            1,
            "Compaction must delete Delta slot after Base PUT (keys={:?}, errors={:?})",
            keys_after_compact,
            compact_up.errors
        );
        assert!(!keys_after_compact[0].contains("delta"));

        // Reader 2 decrypts consolidated Base and receives all 18 cards incl. updated_v
        {
            let mut s = r2_state.service.lock().unwrap();
            r2_state
                .mutate_security_policy(&mut s, Some(password), |policy| {
                    policy.market_selected_syncs = vec![finalize_status.descriptor.clone()];
                })
                .unwrap();
        }
        let r2_res = sync_market_channels_impl(None, None, Some(password.to_string()), &r2_state)
            .await
            .unwrap();
        assert_eq!(r2_res.decrypted_slots, 1);
        assert_eq!(r2_res.imported_cards, 18);
        {
            let mut s = r2_state.service.lock().unwrap();
            let raw = s
                .load_encrypted_data(MARKET_CARD_STORE_KEY, Some(password))
                .unwrap();
            let r2_store: humoco_market::CardStore = serde_json::from_slice(&raw).unwrap();
            let imported = r2_store
                .cards
                .values()
                .find(|c| c.card.payload.did == updated_did)
                .unwrap();
            assert_eq!(imported.card.payload.v, updated_v);
        }
    }

    #[tokio::test]
    async fn test_quiet_friend_without_own_card_forwards_stored_cards() {
        let base_dir = temp_test_dir("quiet_friend_forward");
        let password = "TestPassword123!";

        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let bob_did = bob_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let draft = prepare_sync_setup_impl(
            None,
            None,
            None,
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let finalize_status = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        set_selected_upload_syncs_impl(
            vec![finalize_status.descriptor.clone()],
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        // Alice trusts Bob (Level 2), has NO published own market profile, and holds Carol's stored card
        let now_secs = sync_client::current_unix_timestamp();
        let third_party_env =
            build_signed_test_card_envelope(77, now_secs, now_secs + 365 * 86_400, 1, 1);
        let third_party_did = humoco_market::verify_sd_card(&third_party_env.card_wire)
            .unwrap()
            .payload
            .did;

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
                        added_at: "2026-01-01T00:00:00Z".into(),
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

            let mut store = humoco_market::CardStore::default();
            humoco_market::accept_envelope(
                &mut store,
                &third_party_env,
                now_secs,
                2000,
                None,
                &|_| false,
            );
            let bytes = serde_json::to_vec(&store).unwrap();
            s.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, Some(password))
                .unwrap();
        }
        sync_channel_members_impl(Some(password), &alice_state)
            .await
            .unwrap();

        // Quiet Alice uploads stored third-party card even without own card
        let alice_sync =
            sync_market_channels_impl(None, Some(true), Some(password.to_string()), &alice_state)
                .await
                .unwrap();
        assert!(
            alice_sync.uploaded_to >= 1,
            "Quiet friend must still upload forwarded card bundle"
        );

        // Bob syncs and receives the forwarded card with hops == 2
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .mutate_security_policy(&mut s, Some(password), |policy| {
                    policy.market_selected_syncs = vec![finalize_status.descriptor.clone()];
                })
                .unwrap();
        }
        let bob_sync =
            sync_market_channels_impl(None, None, Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        assert!(
            bob_sync.imported_cards >= 1,
            "Newcomer Bob must receive forwarded card from quiet friend"
        );
        {
            let mut s = bob_state.service.lock().unwrap();
            let raw = s
                .load_encrypted_data(MARKET_CARD_STORE_KEY, Some(password))
                .unwrap();
            let bob_store: humoco_market::CardStore = serde_json::from_slice(&raw).unwrap();
            let imported = bob_store
                .cards
                .values()
                .find(|c| c.card.payload.did == third_party_did)
                .expect("Forwarded card must be in Bob's CardStore");
            assert_eq!(imported.hops, 2);
        }
    }

    #[tokio::test]
    async fn test_sync_admin_status_requires_finalized_live_server() {
        let base_dir = temp_test_dir("admin_status_finalized");
        let password = "pw";
        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let state = create_test_state(alice_service);

        // Before any setup, admin status is None
        assert!(
            get_sync_admin_status_impl(Some(password.to_string()), &state)
                .await
                .unwrap()
                .is_none()
        );

        // Preparing setup (copying config.toml / generating ZIP) must NOT mark service as active
        let draft =
            prepare_sync_setup_impl(None, None, None, None, Some(password.to_string()), &state)
                .unwrap();
        let _toml = get_rust_sync_config_impl(Some(password.to_string()), &state).unwrap();
        let zip_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &draft.zip_base64,
        )
        .expect("prepare_sync_setup_impl must produce valid base64 ZIP bytes");
        assert!(
            zip_bytes.starts_with(b"PK\x03\x04"),
            "decoded setup artifact must be a PKZIP archive"
        );
        assert!(
            get_sync_admin_status_impl(Some(password.to_string()), &state)
                .await
                .unwrap()
                .is_none(),
            "Prepared draft with empty write_url must still return None"
        );

        // Finalizing against an unreachable server must fail and keep status as None
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_port = listener.local_addr().unwrap().port();
        drop(listener);
        let dead_url = format!("http://127.0.0.1:{}/sync.php", closed_port);

        let err = finalize_sync_setup_impl(dead_url, Some(password.to_string()), &state)
            .await
            .expect_err("Finalizing against unreachable server must fail");
        assert!(
            !err.message.is_empty(),
            "Unreachable server must return a clear error"
        );
        assert!(
            get_sync_admin_status_impl(Some(password.to_string()), &state)
                .await
                .unwrap()
                .is_none(),
            "Failed finalize must not persist active admin status"
        );

        // Finalizing against a live, valid server succeeds and activates admin status
        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let finalized =
            finalize_sync_setup_impl(server.base_url.clone(), Some(password.to_string()), &state)
                .await
                .unwrap();
        let expected_write_url = format!("{}/sync.php", server.base_url);
        assert_eq!(finalized.descriptor.write_url, expected_write_url);

        let status = get_sync_admin_status_impl(Some(password.to_string()), &state)
            .await
            .unwrap()
            .expect("Finalized service must return Some(FrontendSyncAdminStatus)");
        assert_eq!(status.descriptor.write_url, expected_write_url);
    }

    #[tokio::test]
    async fn test_probe_distinguishes_reachable_head_error_from_offline_and_validates_admin_read_url(
    ) {
        let base_dir = temp_test_dir("probe_head_error_vs_offline");
        let password = "pw";

        // Custom mini HTTP server where ?info returns 200 OK, PUT ?members returns 204,
        // HEAD returns 405 Method Not Allowed, and ?info read_url points to non-loopback HTTP (invalid).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_url = format!("http://127.0.0.1:{}/sync.php", port);

        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);
        let draft = prepare_sync_setup_impl(
            None,
            None,
            None,
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        let admin_pub = draft.admin_pubkey.clone();
        let valid_local_read = format!("http://127.0.0.1:{}/{}", port, draft.data_dir);

        let override_bad_read_url = std::sync::Arc::new(std::sync::Mutex::new(true));
        let bad_flag = std::sync::Arc::clone(&override_bad_read_url);
        let admin_pub_srv = admin_pub.clone();
        let valid_read_srv = valid_local_read.clone();

        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            while let Ok((mut stream, _)) = listener.accept().await {
                let bad_read = *bad_flag.lock().unwrap();
                let ap = admin_pub_srv.clone();
                let vr = valid_read_srv.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 16_384];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req.lines().next().unwrap_or("");
                    let mut parts = first_line.split_whitespace();
                    let method = parts.next().unwrap_or("");
                    let path = parts.next().unwrap_or("");

                    let resp = if method == "GET" && path.contains("info") {
                        let read_u = if bad_read {
                            "http://misconfigured.example.org/d-bad".to_string()
                        } else {
                            vr
                        };
                        let body = serde_json::json!({
                            "drop_pub": ap,
                            "read_url": read_u,
                            "register_url": null,
                            "channels": ["card", "wot", "backup", "post"]
                        })
                        .to_string();
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        )
                    } else if method == "PUT" && path.contains("members") {
                        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n".to_string()
                    } else if method == "HEAD" {
                        "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n".to_string()
                    } else {
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
                    };
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        // 1. Admin finalize & connection test MUST reject invalid info.read_url ("http://misconfigured.example.org/...")
        let conn_res =
            test_sync_connection_impl(server_url.clone(), Some(password.to_string()), &alice_state)
                .await
                .unwrap();
        assert!(!conn_res.success);
        assert!(conn_res.write_ok);
        assert!(!conn_res.read_ok);

        let fin_err =
            finalize_sync_setup_impl(server_url.clone(), Some(password.to_string()), &alice_state)
                .await
                .expect_err("finalize must reject invalid read_url reported by ?info");
        assert!(fin_err.message.contains("read_url"));

        // 2. Fix read_url on server; Bob probes while HEAD returns 405 Method Not Allowed.
        // Because ?info succeeds, Bob MUST get "pending_auth" (never "unreachable" / offline!).
        *override_bad_read_url.lock().unwrap() = false;

        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let bob_state = create_test_state(bob_service);
        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // Also give Bob a healthy sync and set last_probed_at = Some(500) on an "unverified" contact sync
        // to prove recovered unverified endpoints are still probed when force=false.
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .mutate_security_policy(&mut s, Some(password), |policy| {
                    policy.market_selected_syncs.push(FrontendSyncDescriptor {
                        write_url: "https://healthy.example.org/sync.php".to_string(),
                        read_url: "https://healthy.example.org/d-ok".to_string(),
                        drop_pub: crate::test_support::test_ed25519_pub_hex(0x33),
                        status: "active".to_string(),
                        ..Default::default()
                    });
                })
                .unwrap();

            let contact = FrontendContact {
                did: alice_did,
                profile: FrontendUserProfile::default(),
                tags: vec![],
                added_at: chrono::Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(1),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: server_url.clone(),
                    read_url: String::new(),
                    drop_pub: String::new(),
                    status: "unverified".to_string(),
                    last_probed_at: Some(500),
                    ..Default::default()
                }],
            };
            bob_state
                .save_contact(&mut s, contact, Some(password))
                .unwrap();
        }

        let probed = probe_contact_syncs_impl(false, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(probed.len(), 1);
        assert_eq!(
            probed[0].status, "pending_auth",
            "Reachable server (?info 200) with failing HEAD (405) must report pending_auth, never unreachable"
        );
        assert_eq!(probed[0].fail_count, 0);
        assert_eq!(probed[0].read_url, valid_local_read);
        assert!(probed[0].last_seen_at.is_some());
    }

    #[tokio::test]
    async fn test_sync_admin_quotas_setup_status_and_remote_update() {
        let base_dir = temp_test_dir("sync_admin_quotas");
        let password = "TestPassword123!";

        let alice_service =
            setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        // 1. User quota < 10 MB must be rejected
        let too_small_err = prepare_sync_setup_impl(
            None,
            Some(300),
            Some(2048),
            Some(9),
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap_err();
        assert!(too_small_err.message.contains("10 MB"));

        // 2. Defaults match 2048 MB total / 300 members / 25 MB per user
        let draft = prepare_sync_setup_impl(
            None,
            None,
            None,
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert_eq!(draft.max_members, 300);
        assert_eq!(draft.max_total_storage_mb, 2048);
        assert_eq!(draft.max_user_storage_mb, 25);
        let toml = &draft.rust_config_toml;
        assert!(toml.contains("max_total_storage_mb = 2048"));
        assert!(toml.contains("max_admin_members = 300"));
        assert!(toml.contains("max_admin_user_storage_mb = 25"));

        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let fin = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(fin.max_members, 300);
        assert_eq!(fin.max_total_storage_mb, 2048);
        assert_eq!(fin.max_user_storage_mb, 25);

        // 3. Push live admin quota update via PUT ?admin_config
        let updated = update_sync_admin_config_impl(
            FrontendSyncAdminUpdateRequest {
                max_members: Some(450),
                max_total_storage_mb: Some(4096),
                max_user_storage_mb: Some(40),
                ..Default::default()
            },
            true,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(updated.max_members, 450);
        assert_eq!(updated.max_total_storage_mb, 4096);
        assert_eq!(updated.max_user_storage_mb, 40);
        assert_eq!(*server.admin_config.lock().unwrap(), (4096, 450, 40));

        // 4. Subsequent GET ?status syncs live limits back into local state
        *server.admin_config.lock().unwrap() = (8192, 600, 50);
        let synced = get_sync_admin_status_impl(Some(password.to_string()), &alice_state)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(synced.max_total_storage_mb, 8192);
        assert_eq!(synced.max_members, 600);
        assert_eq!(synced.max_user_storage_mb, 50);
    }

    #[tokio::test]
    async fn test_reach_shrink_unpublish_trust_withdrawal_and_media_gc() {
        use base64::Engine;

        let base_dir = temp_test_dir("reach_shrink_tombstone_flow");
        let password = "TestPassword123!";

        let alice_state = create_test_state(setup_test_app(
            &base_dir.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let eve_state = create_test_state(setup_test_app(
            &base_dir.join("eve"),
            "Eve",
            READER1_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &base_dir.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));
        let carol_state = create_test_state(setup_test_app(
            &base_dir.join("carol"),
            "Carol",
            CHARLIE_PHRASE,
            password,
        ));

        let get_did = |st: &AppState| -> String {
            let s = st.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let alice_did = get_did(&alice_state);
        let eve_did = get_did(&eve_state);
        let bob_did = get_did(&bob_state);
        let carol_did = get_did(&carol_state);
        let can_alice = canonicalize_did(&alice_did).unwrap();

        let draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 1,
                hop2_enabled: true,
                bridge_min: 1,
                hop2_min: 1,
            }),
            Some(300),
            None,
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let fin = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        for st in [&alice_state, &eve_state, &bob_state, &carol_state] {
            let mut s = st.service.lock().unwrap();
            st.mutate_security_policy(&mut s, Some(password), |policy| {
                policy.market_selected_syncs = vec![fin.descriptor.clone()];
            })
            .unwrap();
            st.mutate_ui_preferences(&mut s, Some(password), |prefs| {
                prefs.wot_share_via_sync = true;
            })
            .unwrap();
        }

        let mk_contact = |did: String, name: &str, level: u8| FrontendContact {
            did,
            profile: FrontendUserProfile {
                first_name: Some(name.to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(level),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };

        // Set "far" reach preset (max_hops=3) before signing initial lists
        apply_reach_preset_impl("far".to_string(), Some(password.to_string()), &alice_state)
            .unwrap();
        apply_reach_preset_impl("far".to_string(), Some(password.to_string()), &bob_state).unwrap();
        apply_reach_preset_impl("far".to_string(), Some(password.to_string()), &carol_state)
            .unwrap();

        // Bob trusts Carol (2) and Carol trusts Bob (2); Bob exports his initial package BEFORE
        // Alice/Bob mutually lock at unreciprocated_hops=1, mirroring direct handshake flow.
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .save_contact(
                    &mut s,
                    mk_contact(carol_did.clone(), "Carol", 2),
                    Some(password),
                )
                .unwrap();
        }
        {
            let mut s = carol_state.service.lock().unwrap();
            carol_state
                .save_contact(
                    &mut s,
                    mk_contact(bob_did.clone(), "Bob", 2),
                    Some(password),
                )
                .unwrap();
        }
        let can_bob = canonicalize_did(&bob_did).unwrap();
        let bob_init_pkg = {
            let mut bs = bob_state.service.lock().unwrap();
            let mut bob_reg = bob_state
                .get_or_load_wot_register(&mut bs, Some(password))
                .unwrap();
            bob_state
                .ensure_wot_list_signed(&mut bs, &mut bob_reg)
                .unwrap();
            bob_reg
                .prepare_sync_package(Some(&can_alice), None, true)
                .unwrap()
                .0
        };

        // Alice trusts Eve (Level 1) first, then imports Bob's package immediately before trusting Bob (Level 2),
        // then exports her package so Bob sees Bob@3 before adding Alice (same handshake as test_04).
        let alice_init_pkg = {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    mk_contact(eve_did.clone(), "Eve", 1),
                    Some(password),
                )
                .unwrap();

            let mut alice_reg = alice_state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            let now = sync_client::current_unix_timestamp();
            alice_reg
                .import_sync_package(&bob_init_pkg, Some(&can_bob), now)
                .unwrap();
            alice_state
                .save_wot_register(&mut s, &alice_reg, Some(password))
                .unwrap();

            alice_state
                .save_contact(
                    &mut s,
                    mk_contact(bob_did.clone(), "Bob", 2),
                    Some(password),
                )
                .unwrap();

            let mut reg_after = alice_state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            alice_state
                .ensure_wot_list_signed(&mut s, &mut reg_after)
                .unwrap();
            alice_state
                .save_wot_register(&mut s, &reg_after, Some(password))
                .unwrap();
            reg_after
                .prepare_sync_package(Some(&can_bob), None, true)
                .unwrap()
                .0
        };
        {
            let mut s = eve_state.service.lock().unwrap();
            eve_state
                .save_contact(
                    &mut s,
                    mk_contact(alice_did.clone(), "Alice", 1),
                    Some(password),
                )
                .unwrap();
        }
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut bob_reg = bob_state
                .get_or_load_wot_register(&mut s, Some(password))
                .unwrap();
            let now = sync_client::current_unix_timestamp();
            bob_reg
                .import_sync_package(&alice_init_pkg, Some(&can_alice), now)
                .unwrap();
            bob_state
                .save_wot_register(&mut s, &bob_reg, Some(password))
                .unwrap();
            bob_state
                .save_contact(
                    &mut s,
                    mk_contact(alice_did.clone(), "Alice", 2),
                    Some(password),
                )
                .unwrap();
        }

        // Alice publishes profile + ad with WEBP media
        save_my_market_profile_impl(
            FrontendSaveMyMarketProfileRequest {
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                visibility: CardFieldVisibility::default(),
                skills: vec![],
                max_hops: 3,
                validity_years: 1,
                allow_inquiries: true,
                published: Some(true),
            },
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        let mut webp_bytes = Vec::with_capacity(30);
        webp_bytes.extend_from_slice(b"RIFF");
        webp_bytes.extend_from_slice(&(22u32).to_le_bytes());
        webp_bytes.extend_from_slice(b"WEBP");
        webp_bytes.extend_from_slice(b"VP8 ");
        webp_bytes.extend_from_slice(&(10u32).to_le_bytes());
        webp_bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x9d, 0x01, 0x2a]);
        webp_bytes.extend_from_slice(&120u16.to_le_bytes());
        webp_bytes.extend_from_slice(&120u16.to_le_bytes());
        let webp_uri = format!(
            "data:image/webp;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&webp_bytes)
        );
        upsert_ad_impl(
            FrontendAdInput {
                id: None,
                kind: humoco_market::AdKind::Offer,
                title: "Wildflower Honey".to_string(),
                text: Some("Fresh harvest".to_string()),
                pricing: Some("5 M".to_string()),
                duration_days: 90,
                allow_inquiries: true,
                is_remote: false,
                image_webp_data_uri: Some(Some(webp_uri)),
                event_at: None,
                event_all_day: false,
                published: Some(true),
            },
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        // Bob also publishes a card so Bob forwards Hop-2 Alice card & WoT list to Carol
        apply_reach_preset_impl("far".to_string(), Some(password.to_string()), &bob_state).unwrap();
        save_my_market_profile_impl(
            FrontendSaveMyMarketProfileRequest {
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                visibility: CardFieldVisibility::default(),
                skills: vec![],
                max_hops: 3,
                validity_years: 1,
                allow_inquiries: true,
                published: Some(true),
            },
            Some(password.to_string()),
            &bob_state,
        )
        .unwrap();

        // Sync round (a): Alice uploads -> Bob syncs & forwards -> Alice learns Bob's list & authorizes Carol -> Eve & Carol sync
        let _ = sync_market_channels_impl(
            Some(true),
            Some(true),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert!(
            server
                .slots
                .lock()
                .unwrap()
                .keys()
                .any(|k| k.ends_with(".media")),
            "Published ad image blob must be uploaded to server"
        );

        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        let _ = sync_market_channels_impl(
            Some(true),
            Some(true),
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        // Alice downloads Bob's WoT list so Carol is known at Hop 2 for WoT tombstones
        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        // Second sync for Alice to push member authorization for Carol if needed
        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &eve_state,
        )
        .await
        .unwrap();
        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &carol_state,
        )
        .await
        .unwrap();

        let eve_view_far = get_market_view_impl(Some(password.to_string()), &eve_state).unwrap();
        assert!(eve_view_far.people.iter().any(|p| p.did == can_alice));
        assert!(eve_view_far
            .board
            .iter()
            .any(|b| b.ad.title == "Wildflower Honey"));

        let bob_view_far = get_market_view_impl(Some(password.to_string()), &bob_state).unwrap();
        assert!(bob_view_far.people.iter().any(|p| p.did == can_alice));

        let carol_view_far =
            get_market_view_impl(Some(password.to_string()), &carol_state).unwrap();
        assert!(
            carol_view_far.people.iter().any(|p| p.did == can_alice),
            "Hop-2 Carol must initially see Alice at preset 'far'"
        );
        {
            let mut cs = carol_state.service.lock().unwrap();
            let carol_reg = carol_state
                .get_or_load_wot_register(&mut cs, Some(password))
                .unwrap();
            let alice_list = carol_reg.raw_list_store.get(&can_alice).unwrap();
            assert!(!alice_list.disclosures.is_empty());
        }

        // (b) Alice lowers reach to "near" (max_hops=1, share_min_level=2, own_list_max_hops=1)
        apply_reach_preset_impl("near".to_string(), Some(password.to_string()), &alice_state)
            .unwrap();
        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        // Eve (Level 1 < share_min_level=2) receives Class-2 <jws>~ tombstone -> hides Alice
        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &eve_state,
        )
        .await
        .unwrap();
        let eve_view_near = get_market_view_impl(Some(password.to_string()), &eve_state).unwrap();
        assert!(!eve_view_near.people.iter().any(|p| p.did == can_alice));
        assert!(!eve_view_near.board.iter().any(|b| b.owner_did == can_alice));

        // Bob syncs (gets Alice's shrunk max_hops=1 card & WoT), forwards <jws>~ on next upload, Carol downloads
        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        let _ = sync_market_channels_impl(
            Some(true),
            Some(true),
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &carol_state,
        )
        .await
        .unwrap();

        let carol_view_near =
            get_market_view_impl(Some(password.to_string()), &carol_state).unwrap();
        assert!(
            !carol_view_near.people.iter().any(|p| p.did == can_alice),
            "Hop-2 Carol must hide Alice after receiving shrunk <jws>~ card tombstone"
        );
        {
            let mut cs = carol_state.service.lock().unwrap();
            let carol_reg = carol_state
                .get_or_load_wot_register(&mut cs, Some(password))
                .unwrap();
            let alice_list = carol_reg.raw_list_store.get(&can_alice).unwrap();
            assert!(
                alice_list.disclosures.is_empty(),
                "Hop-2 Carol must receive empty <jws>~ WoT tombstone from Alice"
            );
        }

        // (c) Alice unpublishes her card -> orphaned media blob GC deletes .media slot & Bob hides Alice
        set_card_published_impl(false, Some(password.to_string()), &alice_state)
            .await
            .unwrap();
        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert!(
            !server
                .slots
                .lock()
                .unwrap()
                .keys()
                .any(|k| k.ends_with(".media")),
            "Orphaned media blob must be deleted from server when unpublished"
        );

        let _ = sync_market_channels_impl(
            Some(true),
            Some(false),
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        let bob_view_unpub = get_market_view_impl(Some(password.to_string()), &bob_state).unwrap();
        assert!(!bob_view_unpub.people.iter().any(|p| p.did == can_alice));

        // (d) Bob withdraws trust from Alice (level=0) -> immediate local CardStore & raw_list_store prune
        {
            let mut bs = bob_state.service.lock().unwrap();
            bob_state
                .save_contact(
                    &mut bs,
                    mk_contact(alice_did.clone(), "Alice", 0),
                    Some(password),
                )
                .unwrap();
            let store_bytes = bs
                .load_encrypted_data(MARKET_CARD_STORE_KEY, Some(password))
                .unwrap();
            let store: humoco_market::CardStore = serde_json::from_slice(&store_bytes).unwrap();
            assert!(!store.cards.values().any(|sc| {
                canonicalize_did(&sc.card.payload.did).ok().as_deref() == Some(can_alice.as_str())
            }));
            let bob_reg = bob_state
                .get_or_load_wot_register(&mut bs, Some(password))
                .unwrap();
            assert!(!bob_reg.raw_list_store.contains_key(&can_alice));
        }
    }

    fn make_signed_card(
        did: &str,
        sk: &ed25519_dalek::SigningKey,
        v: u64,
        hops: u8,
    ) -> humoco_market::CardEnvelope {
        let valid_until = sync_client::current_unix_timestamp() + 86_400;
        let card = humoco_market::sign_sd_card(
            humoco_market::CardPayloadInput {
                did: did.to_string(),
                v,
                valid_until,
                max_hops: 3,
                ad_count: 0,
                allow_inquiries: true,
                drops: vec![],
            },
            vec![(
                humoco_market::CardItem::Name {
                    first_name: Some(format!("Card-V{}", v)),
                    last_name: None,
                },
                1,
            )],
            sk,
        )
        .unwrap();
        let wire =
            humoco_market::render_wire(&card, hops, humoco_market::WireFilter::Full).unwrap();
        humoco_market::CardEnvelope {
            card_wire: wire,
            hops,
        }
    }

    #[tokio::test]
    async fn test_k1_sync_aborts_on_logout_and_profile_switch_without_overwriting_profile_b() {
        use crate::test_support::{
            temp_test_dir, ALICE_PHRASE, BOB_PHRASE, CHARLIE_PHRASE, READER1_PHRASE,
        };
        use human_money_core::app_service::AppService;
        use std::sync::Arc;

        let base_dir = temp_test_dir("k1_cross_profile_sync");
        let password = "TestPassword123!";
        let wallet_root = base_dir.join("wallet_root");

        // Create both Alice and Bob inside the same AppService wallet root.
        let mut shared_svc = AppService::new(&wallet_root).unwrap();
        shared_svc
            .create_profile(
                "Alice",
                ALICE_PHRASE,
                None,
                Some("pc"),
                password,
                human_money_core::MnemonicLanguage::English,
                "device-k1-test".into(),
                true,
            )
            .unwrap();
        shared_svc.logout();
        shared_svc
            .create_profile(
                "Bob",
                BOB_PHRASE,
                None,
                Some("pc"),
                password,
                human_money_core::MnemonicLanguage::English,
                "device-k1-test".into(),
                true,
            )
            .unwrap();
        let profiles = shared_svc.list_profiles().unwrap();
        let alice_folder = profiles
            .iter()
            .find(|p| p.profile_name == "Alice")
            .unwrap()
            .folder_name
            .clone();
        let bob_folder = profiles
            .iter()
            .find(|p| p.profile_name == "Bob")
            .unwrap()
            .folder_name
            .clone();
        shared_svc.logout();

        let state = Arc::new(AppState::new(shared_svc));

        // External publisher Carol + baseline Reader1 card belonging solely to Bob
        let carol_service =
            setup_test_app(&base_dir.join("carol"), "Carol", CHARLIE_PHRASE, password);
        let carol_state = create_test_state(carol_service);
        let (carol_did, carol_sk, carol_vk) = {
            let s = carol_state.service.lock().unwrap();
            s.with_wallet_and_identity(|_, id| {
                (id.user_id.clone(), id.signing_key.clone(), id.public_key)
            })
            .unwrap()
        };

        let r1_service = setup_test_app(&base_dir.join("r1"), "Reader1", READER1_PHRASE, password);
        let (r1_did, r1_sk) = r1_service
            .with_wallet_and_identity(|_, id| (id.user_id.clone(), id.signing_key.clone()))
            .unwrap();
        let r1_env = make_signed_card(&r1_did, &r1_sk, 1, 1);

        // Prepare Carol's server and push Carol's card for Alice (Bob is NOT a member/reader)
        let draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 1,
                hop2_enabled: false,
                bridge_min: 1,
                hop2_min: 1,
            }),
            Some(300),
            None,
            None,
            Some(password.to_string()),
            &carol_state,
        )
        .unwrap();
        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let fin = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &carol_state,
        )
        .await
        .unwrap();

        // Log into Alice, record her credentials, trust Carol, configure polling from Carol's server
        let (alice_did, alice_vk) = {
            let mut s = state.service.lock().unwrap();
            s.login(&alice_folder, password, false, "device-k1-test".into())
                .unwrap();
            state.initialize_profile_session(&mut s, password).unwrap();
            let (did, vk) = s
                .with_wallet_and_identity(|_, id| (id.user_id.clone(), id.public_key))
                .unwrap();
            state
                .mutate_security_policy(&mut s, Some(password), |p| {
                    p.market_selected_syncs = vec![fin.descriptor.clone()];
                })
                .unwrap();
            let c = FrontendContact {
                did: carol_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Carol".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![fin.descriptor.clone()],
            };
            state.save_contact(&mut s, c, Some(password)).unwrap();
            (did, vk)
        };

        // Initialize Bob's profile with pre-existing CardStore (Reader1) + contact "BobOnly"
        crate::commands::auth::logout_impl(&state);
        {
            let mut s = state.service.lock().unwrap();
            s.login(&bob_folder, password, false, "device-k1-test".into())
                .unwrap();
            state.initialize_profile_session(&mut s, password).unwrap();
            let mut bob_store = humoco_market::CardStore::default();
            let out = humoco_market::accept_envelope(
                &mut bob_store,
                &r1_env,
                sync_client::current_unix_timestamp(),
                2000,
                None,
                &|_| false,
            );
            assert!(matches!(out, humoco_market::AcceptOutcome::Accepted { .. }));
            crate::state::save_encrypted_json(
                &mut s,
                MARKET_CARD_STORE_KEY,
                &bob_store,
                Some(password),
            )
            .unwrap();
            let bob_contact = FrontendContact {
                did: r1_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("BobOnly".to_string()),
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
            };
            state
                .save_contact(&mut s, bob_contact, Some(password))
                .unwrap();
        }
        crate::commands::auth::logout_impl(&state);

        // Log back into Alice and prepare Carol's slot readable by Alice
        {
            let mut s = state.service.lock().unwrap();
            s.login(&alice_folder, password, false, "device-k1-test".into())
                .unwrap();
            state.initialize_profile_session(&mut s, password).unwrap();
        }
        {
            let mut cs = carol_state.service.lock().unwrap();
            let c_contact = FrontendContact {
                did: alice_did.clone(),
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
                syncs: vec![],
            };
            carol_state
                .save_contact(&mut cs, c_contact, Some(password))
                .unwrap();
        }
        let carol_env = make_signed_card(&carol_did, &carol_sk, 2, 1);
        let bundle =
            humoco_market::sign_card_bundle(vec![carol_env], &carol_did, &carol_sk).unwrap();
        let sealed =
            humoco_market::seal_slot_blob(&bundle, &[alice_vk, carol_vk], 512 * 1024).unwrap();
        let stealth_carol =
            humoco_market::derive_member_stealth_pubkey(&carol_sk, &fin.descriptor.drop_pub)
                .unwrap();
        server.members.lock().unwrap().push(stealth_carol.clone());
        server
            .slots
            .lock()
            .unwrap()
            .insert(format!("{}.card", stealth_carol), sealed);

        // Pause Alice's poll at index.json -> switch to Bob mid-flight -> release!
        let (hit_rx, release_tx) = server.arm_pause_gate("index.json");
        let state_bg = Arc::clone(&state);
        let sync_handle = tokio::spawn(async move {
            sync_market_channels_impl(
                Some(true),
                Some(false),
                Some(password.to_string()),
                &state_bg,
            )
            .await
        });

        hit_rx.await.unwrap();
        crate::commands::auth::logout_impl(&state);
        {
            let mut s = state.service.lock().unwrap();
            s.login(&bob_folder, password, false, "device-k1-test".into())
                .unwrap();
            state.initialize_profile_session(&mut s, password).unwrap();
        }
        let _ = release_tx.send(true);
        let res = sync_handle.await.unwrap().unwrap();
        assert_eq!(res.imported_cards, 0);

        // Verify Bob's stores are completely untouched (no CardStore overwrite, no sync state pollution)
        {
            let mut s = state.service.lock().unwrap();
            let bob_store: humoco_market::CardStore = crate::state::load_encrypted_json_or_default(
                &mut s,
                MARKET_CARD_STORE_KEY,
                Some(password),
            )
            .unwrap();
            assert!(
                bob_store.cards.contains_key(&r1_did),
                "Bob's pre-existing Reader1 card must NOT be overwritten by Alice's sync"
            );
            assert!(
                !bob_store.cards.contains_key(&carol_did),
                "Alice's polled Carol card must NOT leak into Bob's wallet"
            );
            let bob_sync: StoredSyncState = crate::state::load_encrypted_json_or_default(
                &mut s,
                MARKET_SYNC_KEY,
                Some(password),
            )
            .unwrap();
            assert_eq!(
                bob_sync.last_poll_at, None,
                "Alice's StoredSyncState.last_poll_at must NOT be written into Bob's profile"
            );
            let bob_contacts = state.load_contacts(&mut s, Some(password)).unwrap();
            assert!(bob_contacts.iter().any(|c| c.did == r1_did));
        }

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn test_k1_sync_aborts_on_same_profile_logout_and_relogin() {
        use crate::test_support::{temp_test_dir, ALICE_PHRASE, CHARLIE_PHRASE};
        use std::sync::Arc;

        let base_dir = temp_test_dir("k1_same_profile_relogin");
        let password = "TestPassword123!";
        let alice_svc = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_folder = alice_svc.list_profiles().unwrap()[0].folder_name.clone();
        let state = Arc::new(AppState::new(alice_svc));

        let carol_svc = setup_test_app(&base_dir.join("carol"), "Carol", CHARLIE_PHRASE, password);
        let carol_state = create_test_state(carol_svc);
        let (carol_did, carol_sk, carol_vk) = {
            let s = carol_state.service.lock().unwrap();
            s.with_wallet_and_identity(|_, id| {
                (id.user_id.clone(), id.signing_key.clone(), id.public_key)
            })
            .unwrap()
        };

        let draft = prepare_sync_setup_impl(
            None,
            Some(300),
            None,
            None,
            Some(password.to_string()),
            &carol_state,
        )
        .unwrap();
        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let fin = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &carol_state,
        )
        .await
        .unwrap();

        let alice_vk = {
            let mut s = state.service.lock().unwrap();
            state
                .mutate_security_policy(&mut s, Some(password), |p| {
                    p.market_selected_syncs = vec![fin.descriptor.clone()];
                })
                .unwrap();
            state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: carol_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("Carol".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".to_string(),
                        notes: None,
                        trust_level: Some(2),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![fin.descriptor.clone()],
                    },
                    Some(password),
                )
                .unwrap();
            s.with_wallet_and_identity(|_, id| id.public_key).unwrap()
        };

        let carol_env = make_signed_card(&carol_did, &carol_sk, 2, 1);
        let bundle =
            humoco_market::sign_card_bundle(vec![carol_env], &carol_did, &carol_sk).unwrap();
        let sealed =
            humoco_market::seal_slot_blob(&bundle, &[alice_vk, carol_vk], 512 * 1024).unwrap();
        let stealth_carol =
            humoco_market::derive_member_stealth_pubkey(&carol_sk, &fin.descriptor.drop_pub)
                .unwrap();
        server.members.lock().unwrap().push(stealth_carol.clone());
        server
            .slots
            .lock()
            .unwrap()
            .insert(format!("{}.card", stealth_carol), sealed);

        let (hit_rx, release_tx) = server.arm_pause_gate("index.json");
        let state_bg = Arc::clone(&state);
        let sync_handle = tokio::spawn(async move {
            sync_market_channels_impl(
                Some(true),
                Some(false),
                Some(password.to_string()),
                &state_bg,
            )
            .await
        });

        hit_rx.await.unwrap();
        crate::commands::auth::logout_impl(&state);
        {
            let mut s = state.service.lock().unwrap();
            s.login(&alice_folder, password, false, "device-drops-test".into())
                .unwrap();
            state.initialize_profile_session(&mut s, password).unwrap();
        }
        let _ = release_tx.send(true);
        let res = sync_handle.await.unwrap().unwrap();
        assert_eq!(res.imported_cards, 0);

        {
            let mut s = state.service.lock().unwrap();
            let store: humoco_market::CardStore = crate::state::load_encrypted_json_or_default(
                &mut s,
                MARKET_CARD_STORE_KEY,
                Some(password),
            )
            .unwrap();
            assert!(
                !store.cards.contains_key(&carol_did),
                "Same-profile logout + re-login must bump session_generation and discard old poll"
            );
            let sync_st: StoredSyncState = crate::state::load_encrypted_json_or_default(
                &mut s,
                MARKET_SYNC_KEY,
                Some(password),
            )
            .unwrap();
            assert_eq!(sync_st.last_poll_at, None);
        }

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn test_h1_market_poll_preserves_concurrent_qr_import_and_deletion_tombstone() {
        use crate::services::market_service::{
            delete_foreign_contact_card_impl, import_contact_card_impl,
        };
        use crate::test_support::{
            temp_test_dir, ALICE_PHRASE, BOB_PHRASE, CHARLIE_PHRASE, READER1_PHRASE,
        };
        use std::sync::Arc;

        let base_dir = temp_test_dir("h1_market_poll_concurrency");
        let password = "TestPassword123!";
        let bob_state = Arc::new(create_test_state(setup_test_app(
            &base_dir.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        )));
        let carol_state = create_test_state(setup_test_app(
            &base_dir.join("carol"),
            "Carol",
            CHARLIE_PHRASE,
            password,
        ));
        let dave_svc = setup_test_app(&base_dir.join("dave"), "Dave", READER1_PHRASE, password);
        let _alice_svc = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);

        let (carol_did, carol_sk, carol_vk) = {
            let s = carol_state.service.lock().unwrap();
            s.with_wallet_and_identity(|_, id| {
                (id.user_id.clone(), id.signing_key.clone(), id.public_key)
            })
            .unwrap()
        };
        let (dave_did, dave_sk) = dave_svc
            .with_wallet_and_identity(|_, id| (id.user_id.clone(), id.signing_key.clone()))
            .unwrap();

        let draft = prepare_sync_setup_impl(
            None,
            Some(300),
            None,
            None,
            Some(password.to_string()),
            &carol_state,
        )
        .unwrap();
        let server =
            MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let fin = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &carol_state,
        )
        .await
        .unwrap();

        let bob_vk = {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .mutate_security_policy(&mut s, Some(password), |p| {
                    p.market_selected_syncs = vec![fin.descriptor.clone()];
                })
                .unwrap();
            bob_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: carol_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("Carol".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".to_string(),
                        notes: None,
                        trust_level: Some(2),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![fin.descriptor.clone()],
                    },
                    Some(password),
                )
                .unwrap();
            // Pre-seed Carol v=1 locally before poll begins (simulates Bob already having Carol's card)
            let mut initial_store = humoco_market::CardStore::default();
            let carol_env = make_signed_card(&carol_did, &carol_sk, 1, 1);
            let out = humoco_market::accept_envelope(
                &mut initial_store,
                &carol_env,
                sync_client::current_unix_timestamp(),
                2000,
                None,
                &|_| false,
            );
            assert!(matches!(out, humoco_market::AcceptOutcome::Accepted { .. }));
            crate::state::save_encrypted_json(
                &mut s,
                MARKET_CARD_STORE_KEY,
                &initial_store,
                Some(password),
            )
            .unwrap();
            s.with_wallet_and_identity(|_, id| id.public_key).unwrap()
        };

        // Put Carol's v=1 card onto the server so the poll downloads it
        let carol_env_srv = make_signed_card(&carol_did, &carol_sk, 1, 1);
        let srv_bundle =
            humoco_market::sign_card_bundle(vec![carol_env_srv], &carol_did, &carol_sk).unwrap();
        let sealed =
            humoco_market::seal_slot_blob(&srv_bundle, &[bob_vk, carol_vk], 512 * 1024).unwrap();
        let stealth_carol =
            humoco_market::derive_member_stealth_pubkey(&carol_sk, &fin.descriptor.drop_pub)
                .unwrap();
        server.members.lock().unwrap().push(stealth_carol.clone());
        server
            .slots
            .lock()
            .unwrap()
            .insert(format!("{}.card", stealth_carol), sealed);

        let (hit_rx, release_tx) = server.arm_pause_gate("index.json");
        let bob_bg = Arc::clone(&bob_state);
        let poll_handle = tokio::spawn(async move {
            sync_market_channels_impl(Some(true), Some(false), Some(password.to_string()), &bob_bg)
                .await
        });

        hit_rx.await.unwrap();

        // Concurrently during poll .await: QR-import Dave AND delete Carol
        let dave_env = make_signed_card(&dave_did, &dave_sk, 1, 1);
        let dave_json = serde_json::to_string(&dave_env).unwrap();
        import_contact_card_impl(dave_json, Some(password.to_string()), &bob_state).unwrap();
        delete_foreign_contact_card_impl(carol_did.clone(), Some(password.to_string()), &bob_state)
            .unwrap();

        let _ = release_tx.send(true);
        let _ = poll_handle.await.unwrap().unwrap();

        let mut s = bob_state.service.lock().unwrap();
        let final_store: humoco_market::CardStore = crate::state::load_encrypted_json_or_default(
            &mut s,
            MARKET_CARD_STORE_KEY,
            Some(password),
        )
        .unwrap();
        assert!(
            final_store.cards.contains_key(&dave_did),
            "Concurrently QR-imported card for Dave must survive slow sync poll"
        );
        assert!(
            !final_store.cards.contains_key(&carol_did),
            "Deleted card for Carol (v=1) must NOT be resurrected by concurrent sync poll"
        );
        assert_eq!(final_store.deleted_card_versions.get(&carol_did), Some(&1));

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn f4_sync_market_channels_reports_tagged_wifi_skip_status() {
        let dir = temp_test_dir("f4_market_sync_status");
        let password = "pw-f4";
        let state = create_test_state(setup_test_app(
            &dir.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        {
            let mut s = state.service.lock().unwrap();
            let mut prefs = state
                .get_or_load_ui_preferences(&mut s, Some(password))
                .unwrap();
            prefs.market_poll_wifi_only = true;
            state
                .save_ui_preferences(&mut s, prefs, Some(password))
                .unwrap();
        }
        let res =
            sync_market_channels_impl(Some(false), Some(false), Some(password.to_string()), &state)
                .await
                .unwrap();
        assert_eq!(res.status, MarketSyncStatus::SkippedWifiOnly);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn s09_f1_probe_pin_mismatch_foreign_slot_and_public_sync_pin() {
        let dir = temp_test_dir("s09_f1_pins");
        let pw = "StrongPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir.join("alice"),
            "Alice",
            ALICE_PHRASE,
            pw,
        ));
        let bob_svc = setup_test_app(&dir.join("bob"), "Bob", BOB_PHRASE, pw);
        let (bob_did, bob_sk) = bob_svc
            .with_wallet_and_identity(|w, id| (w.get_user_id().to_string(), id.signing_key.clone()))
            .unwrap();

        let real_pub = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[0x11u8; 32])
                .verifying_key()
                .as_bytes(),
        );
        let rogue_pub = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[0x22u8; 32])
                .verifying_key()
                .as_bytes(),
        );

        // 1. Rogue server presenting wrong drop_pub must NOT overwrite pinned drop_pub/read_url or clear "revoked".
        let rogue_srv = MockSyncServer::start("d-rogue".to_string(), rogue_pub.clone()).await;
        let orig_read = "https://original.example.org/d-real".to_string();
        let bob_foreign_slot =
            humoco_market::derive_member_stealth_pubkey(&bob_sk, &real_pub).unwrap();
        let bob_contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: format!("{}/sync.php", rogue_srv.base_url),
                read_url: orig_read.clone(),
                slot_id: Some(bob_foreign_slot.clone()),
                drop_pub: real_pub.clone(),
                status: "revoked".to_string(),
                sunset_at: Some(12345),
                ..Default::default()
            }],
        };
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(&mut s, bob_contact, Some(pw))
                .unwrap();
        }
        let probed = probe_contact_syncs_impl(true, Some(pw.to_string()), &alice_state)
            .await
            .unwrap();
        assert_eq!(probed.len(), 1);
        assert_eq!(probed[0].drop_pub, real_pub);
        assert_eq!(probed[0].read_url, orig_read);
        assert_eq!(probed[0].status, "revoked");

        // 2. Real server where only Bob's foreign slot_id is authorized, NOT Alice:
        //    Alice must probe her OWN stealth key -> pending_auth, never active;
        //    once Alice is authorized, auto-activated market_selected_syncs[0].slot_id must be None.
        let good_srv = MockSyncServer::start("d-good".to_string(), real_pub.clone()).await;
        good_srv
            .members
            .lock()
            .unwrap()
            .push(bob_foreign_slot.clone());
        {
            let mut s = alice_state.service.lock().unwrap();
            let mut contacts = alice_state.load_contacts(&mut s, Some(pw)).unwrap();
            contacts[0].syncs = vec![FrontendSyncDescriptor {
                write_url: format!("{}/sync.php", good_srv.base_url),
                read_url: String::new(),
                slot_id: Some(bob_foreign_slot),
                drop_pub: real_pub.clone(),
                status: "unverified".to_string(),
                ..Default::default()
            }];
            alice_state
                .save_contact(&mut s, contacts.remove(0), Some(pw))
                .unwrap();
        }
        let p2 = probe_contact_syncs_impl(true, Some(pw.to_string()), &alice_state)
            .await
            .unwrap();
        assert_eq!(p2[0].status, "pending_auth");

        let alice_stealth = {
            let s = alice_state.service.lock().unwrap();
            let ask = s
                .with_wallet_and_identity(|_, id| id.signing_key.clone())
                .unwrap();
            humoco_market::derive_member_stealth_pubkey(&ask, &real_pub).unwrap()
        };
        good_srv.members.lock().unwrap().push(alice_stealth);
        let p3 = probe_contact_syncs_impl(true, Some(pw.to_string()), &alice_state)
            .await
            .unwrap();
        assert_eq!(p3[0].status, "active");
        {
            let mut s = alice_state.service.lock().unwrap();
            let pol = alice_state
                .get_or_load_security_policy(&mut s, Some(pw))
                .unwrap();
            assert_eq!(pol.market_selected_syncs.len(), 1);
            assert_eq!(pol.market_selected_syncs[0].slot_id, None);
        }

        // 3. save_public_sync_impl must fail closed on drop_pub key pin mismatch.
        let pub_w = format!("{}/sync.php", good_srv.base_url);
        let pub_r = format!("{}/d-good", good_srv.base_url);
        save_public_sync_impl(
            pub_w.clone(),
            pub_r.clone(),
            real_pub,
            None,
            true,
            Some(pw.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        let pin_err = save_public_sync_impl(
            pub_w,
            pub_r,
            rogue_pub,
            None,
            true,
            Some(pw.to_string()),
            &alice_state,
        )
        .await
        .unwrap_err();
        assert!(pin_err.message.contains("pin mismatch"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn s09_f2_admin_verify_drop_pub_before_push_and_reject_zero_revocation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = temp_test_dir("s09_f2_admin_checks");
        let pw = "StrongPassword123!";
        let state = create_test_state(setup_test_app(&dir, "Alice", ALICE_PHRASE, pw));
        let _draft =
            prepare_sync_setup_impl(None, None, None, None, Some(pw.to_string()), &state).unwrap();

        let rev_err =
            revoke_own_sync_server_impl(Some(0), None, None, Some(pw.to_string()), &state)
                .await
                .unwrap_err();
        assert!(rev_err.message.contains("greater than 0"));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let push_hits = Arc::new(AtomicUsize::new(0));
        let push_hits_srv = Arc::clone(&push_hits);
        let wrong_pub = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[0x77u8; 32])
                .verifying_key()
                .as_bytes(),
        );
        let read_url = format!("http://127.0.0.1:{}/d-wrong", port);

        let srv = tokio::spawn(async move {
            for _ in 0..2 {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if req.contains("members") {
                    push_hits_srv.fetch_add(1, Ordering::SeqCst);
                }
                let body = serde_json::json!({
                    "drop_pub": wrong_pub,
                    "read_url": read_url,
                    "channels": ["card", "wot", "backup", "post"]
                })
                .to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });

        let url = format!("http://127.0.0.1:{}/sync.php", port);
        let test_res = test_sync_connection_impl(url.clone(), Some(pw.to_string()), &state)
            .await
            .unwrap();
        assert!(!test_res.success);
        assert!(!test_res.write_ok);
        assert_eq!(
            push_hits.load(Ordering::SeqCst),
            0,
            "S09-F2: must never push signed members to a server whose ?info drop_pub mismatches"
        );

        let fin_err = finalize_sync_setup_impl(url, Some(pw.to_string()), &state)
            .await
            .unwrap_err();
        assert!(fin_err.message.contains("mismatch"));

        let _ = srv.await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn s09_f6_f8_known_offender_timeout_zero_and_stale_cache() {
        use human_money_core::models::conflict::{
            ConflictRole, ProofOfDoubleSpend, ProofStoreEntry,
        };

        let dir = temp_test_dir("s09_f6_f8_offender_t0");
        let pw = "StrongPassword123!";
        let wallet_dir = dir.join("alice");
        let alice_state = create_test_state(setup_test_app(&wallet_dir, "Alice", ALICE_PHRASE, pw));
        let bob_did = setup_test_app(&dir.join("bob"), "Bob", BOB_PHRASE, pw)
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let alice_did = alice_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        {
            let mut s = alice_state.service.lock().unwrap();
            let can_bob = crate::state::canonicalize_did(&bob_did).unwrap();
            let mut reg = alice_state
                .get_or_load_wot_register(&mut s, Some(pw))
                .unwrap();
            reg.own_contacts.insert(
                can_bob,
                humoco_wot::OwnTrustEntry {
                    level: 3,
                    desired_hops: 1,
                    updated_at: 100,
                },
            );
            alice_state
                .save_wot_register(&mut s, &reg, Some(pw))
                .unwrap();

            let (wallet, _) = s.get_unlocked_mut_for_test();
            wallet.proof_store.proofs.insert(
                "proof-s09-offender".to_string(),
                ProofStoreEntry {
                    proof: ProofOfDoubleSpend {
                        proof_id: "proof-s09-offender".to_string(),
                        offender_id: bob_did.clone(),
                        suspected_identity: None,
                        fork_point_prev_hash: "00".repeat(32),
                        conflicting_transactions: Vec::new(),
                        fingerprints: Vec::new(),
                        victim_witnesses: Vec::new(),
                        deletable_at: "2099-01-01T00:00:00Z".to_string(),
                        reporter_id: alice_did,
                        report_timestamp: "2026-01-01T00:00:00Z".to_string(),
                        reporter_signature: "sig".to_string(),
                        affected_voucher_name: None,
                        voucher_standard_uuid: None,
                        resolutions: None,
                        non_redeemable_test_voucher: false,
                    },
                    local_override: false,
                    local_note: None,
                    conflict_role: ConflictRole::default(),
                },
            );

            alice_state
                .mutate_security_policy(&mut s, Some(pw), |p| {
                    p.session_timeout_seconds = 0;
                    p.public_syncs.push(crate::settings::StoredPublicSync {
                        write_url: "https://revoked-pub.example/sync.php".into(),
                        read_url: "https://revoked-pub.example/d".into(),
                        drop_pub: crate::test_support::test_ed25519_pub_hex(0x11),
                        access_pubkey: crate::test_support::test_ed25519_pub_hex(0x22),
                        enabled: true,
                        authorized_channels: vec!["card".into()],
                        status: "revoked".into(),
                        ..Default::default()
                    });
                })
                .unwrap();
        }

        let impact = preview_contact_trust_change_impl(
            bob_did.clone(),
            3,
            Some(pw.to_string()),
            &alice_state,
        )
        .unwrap();
        assert!(!impact.can_read_my_market_before);
        assert!(!impact.can_read_my_market_after);
        assert!(!impact.can_write_my_sync_before);
        assert!(!impact.can_write_my_sync_after);

        // Reload wallet under timeout == 0 (!is_session_active()) and exercise Unlocked readers without password.
        let mut reloaded_svc = human_money_core::app_service::AppService::new(&wallet_dir).unwrap();
        let folder = reloaded_svc.list_profiles().unwrap()[0].folder_name.clone();
        reloaded_svc
            .login(&folder, pw, false, "device-drops-test".to_string())
            .unwrap();
        let reloaded_state =
            create_test_state(human_money_core::app_service::AppService::new(&wallet_dir).unwrap());
        reloaded_state
            .initialize_profile_session(&mut reloaded_svc, pw)
            .unwrap();
        *reloaded_state.service.lock().unwrap() = reloaded_svc;
        assert!(!reloaded_state.service.lock().unwrap().is_session_active());

        let overview = get_sync_overview_status_impl(None, &reloaded_state).unwrap();
        assert_eq!(overview.public_syncs, 0);
        let discovered = discover_available_syncs_impl(None, &reloaded_state)
            .await
            .unwrap();
        assert!(discovered.is_empty());
        assert!(get_sync_admin_status_impl(None, &reloaded_state)
            .await
            .unwrap()
            .is_none());
        let impact_t0 =
            preview_contact_trust_change_impl(bob_did, 3, None, &reloaded_state).unwrap();
        assert!(!impact_t0.can_read_my_market_after);

        // Advance generation while leaving cached slot intact to prove get_cached_store rejects stale generation.
        assert!(reloaded_state.get_cached_security_policy().is_ok());
        reloaded_state
            .session_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(reloaded_state.get_cached_security_policy().is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn s02_x1_sync_bundle_import_clamps_forwarded_hops_and_rejects_tampered_writers() {
        let base_dir = temp_test_dir("s02_x1_writer_binding");
        let password = "TestPassword123!";
        let alice_svc = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_svc = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let carol_svc = setup_test_app(&base_dir.join("carol"), "Carol", CHARLIE_PHRASE, password);

        let (alice_did, alice_sk, alice_vk) = alice_svc
            .with_wallet_and_identity(|_, id| {
                (id.user_id.clone(), id.signing_key.clone(), id.public_key)
            })
            .unwrap();
        let (bob_did, bob_sk) = bob_svc
            .with_wallet_and_identity(|_, id| (id.user_id.clone(), id.signing_key.clone()))
            .unwrap();
        let (carol_did, carol_sk) = carol_svc
            .with_wallet_and_identity(|_, id| (id.user_id.clone(), id.signing_key.clone()))
            .unwrap();
        let alice_x_sk = human_money_core::services::crypto::dh::ed25519_sk_to_x25519_sk(&alice_sk);

        let now = sync_client::current_unix_timestamp();

        // 1. Bob forwards Carol's card (max_hops=3) claiming hops=1 inside Bob's signed CardBundle.
        //    open_slot_blob verifies Bob's writer attestation, and import_decrypted_card_bundle
        //    passes Some(bob_did) into accept_envelope_with_writer -> clamped to effective_hops = 2.
        let carol_env_hop3 = make_signed_card(&carol_did, &carol_sk, 1, 1);
        let forwarded_bundle =
            humoco_market::sign_card_bundle(vec![carol_env_hop3.clone()], &bob_did, &bob_sk)
                .unwrap();
        let sealed_forwarded =
            humoco_market::seal_slot_blob(&forwarded_bundle, &[alice_vk], 512 * 1024).unwrap();
        let opened_forwarded = humoco_market::open_slot_blob(&sealed_forwarded, &alice_x_sk)
            .unwrap()
            .expect("Alice must decrypt Bob's valid signed SlotBlob");
        assert_eq!(opened_forwarded.writer_did, bob_did);

        let mut store = humoco_market::CardStore::default();
        let mut sync_state = StoredSyncState::default();
        let mut address_book = crate::models::FrontendAddressBook::default();
        let mut contacts_changed = false;
        let (accepted_cnt, _) = super::card_sync::import_decrypted_card_bundle(
            opened_forwarded,
            "https://bob.example/d",
            now,
            100,
            &alice_svc,
            &mut store,
            &mut sync_state,
            &mut address_book,
            &mut contacts_changed,
        );
        assert_eq!(accepted_cnt, 1);
        assert_eq!(
            store.cards.get(&carol_did).map(|s| s.hops),
            Some(2),
            "Forwarded third-party card claiming hops=1 inside Bob's CardBundle must clamp to 2"
        );

        // 2. Carol's card with max_hops=1 forwarded by Bob claiming hops=1 MUST be rejected when clamped to 2.
        let carol_hop1_card = humoco_market::sign_sd_card(
            humoco_market::CardPayloadInput {
                did: carol_did.clone(),
                v: 2,
                valid_until: now + 86_400,
                max_hops: 1,
                ad_count: 0,
                allow_inquiries: false,
                drops: vec![],
            },
            vec![(
                humoco_market::CardItem::Name {
                    first_name: Some("CarolDirectOnly".to_string()),
                    last_name: None,
                },
                1,
            )],
            &carol_sk,
        )
        .unwrap();
        let carol_hop1_env = humoco_market::CardEnvelope {
            hops: 1,
            card_wire: humoco_market::render_wire(
                &carol_hop1_card,
                1,
                humoco_market::WireFilter::Full,
            )
            .unwrap(),
        };
        let bob_hop1_forward_bundle =
            humoco_market::sign_card_bundle(vec![carol_hop1_env], &bob_did, &bob_sk).unwrap();
        let sealed_hop1 =
            humoco_market::seal_slot_blob(&bob_hop1_forward_bundle, &[alice_vk], 512 * 1024)
                .unwrap();
        let opened_hop1 = humoco_market::open_slot_blob(&sealed_hop1, &alice_x_sk)
            .unwrap()
            .unwrap();
        let mut store_hop1 = humoco_market::CardStore::default();
        let (rejected_cnt, _) = super::card_sync::import_decrypted_card_bundle(
            opened_hop1,
            "https://bob.example/d",
            now,
            100,
            &alice_svc,
            &mut store_hop1,
            &mut StoredSyncState::default(),
            &mut crate::models::FrontendAddressBook::default(),
            &mut false,
        );
        assert_eq!(rejected_cnt, 0);
        assert!(!store_hop1.cards.contains_key(&carol_did));

        // 3. Tampered CardBundle inside raw SlotPayload MUST fail closed in open_slot_blob AND direct import.
        let mut tampered_spoof_did = forwarded_bundle.clone();
        tampered_spoof_did.writer_did = alice_did.clone();
        let tampered_payload = serde_json::to_vec(&tampered_spoof_did).unwrap();
        let tampered_sealed =
            humoco_market::seal_slot_payload(&tampered_payload, &[alice_vk], 512 * 1024).unwrap();
        assert!(
            humoco_market::open_slot_blob(&tampered_sealed, &alice_x_sk).is_err(),
            "open_slot_blob must reject CardBundle with spoofed writer_did"
        );
        let mut store_tampered = humoco_market::CardStore::default();
        let (tampered_cnt, _) = super::card_sync::import_decrypted_card_bundle(
            tampered_spoof_did,
            "https://evil.example/d",
            now,
            100,
            &alice_svc,
            &mut store_tampered,
            &mut StoredSyncState::default(),
            &mut crate::models::FrontendAddressBook::default(),
            &mut false,
        );
        assert_eq!(tampered_cnt, 0);
        assert!(store_tampered.cards.is_empty());

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
