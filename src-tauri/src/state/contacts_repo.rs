// src-tauri/src/state/contacts_repo.rs
use super::{
    app_state_err, canonicalize_did, load_encrypted_json_opt, load_encrypted_json_or_default,
    save_encrypted_json, AppState, EncryptedBatchCommit, StoredSyncAdmin, CONTACTS_DATA_NAME,
    MARKET_SYNC_ADMIN_KEY,
};
use crate::models::{FrontendAddressBook, FrontendContact, SyncRevocationFields};
use crate::services::sync_endpoints::{
    drop_pub_matches, endpoints_overlap, is_endpoint_revoked, normalize_sync_url,
    sanitize_untrusted_frontend_sync_descriptors, sanitize_untrusted_sync_descriptors,
    validate_contact_sync_descriptors,
};
use crate::settings::SecurityPolicy;
use chrono::Utc;
use human_money_core::app_service::{AppFacadeError, AppService};
use log::{error, info};

impl AppState {
    // 3. Contacts Helpers
    pub fn load_contacts(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<Vec<FrontendContact>, AppFacadeError> {
        info!("Fetching all contacts from address book from disk...");
        let address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password).map_err(|e| {
                error!("Failed to load address book: {:?}", e);
                e
            })?;

        self.set_cached_contacts_for_service(service, Some(address_book.clone()));

        let wot_opt = self.cache_wot();
        let mut contacts: Vec<FrontendContact> = address_book.contacts.into_values().collect();
        for c in contacts.iter_mut() {
            enrich_contact_with_wot(c, wot_opt.as_ref());
            c.syncs.truncate(2);
        }
        contacts.sort_by(|a, b| {
            let name_a = a.profile.first_name.as_deref().unwrap_or("");
            let name_b = b.profile.first_name.as_deref().unwrap_or("");
            name_a.to_lowercase().cmp(&name_b.to_lowercase())
        });
        Ok(contacts)
    }

    pub fn save_contact(
        &self,
        service: &mut AppService,
        contact: FrontendContact,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        self.save_contact_with_options(service, contact, password, false)
    }

    pub fn save_contact_with_options(
        &self,
        service: &mut AppService,
        contact: FrontendContact,
        password: Option<&str>,
        preserve_manual_card: bool,
    ) -> Result<(), AppFacadeError> {
        self.save_contact_with_companions(
            service,
            contact,
            password,
            preserve_manual_card,
            |_service, _reg, _batch| Ok(()),
        )
    }

    pub(crate) fn save_contact_with_companions(
        &self,
        service: &mut AppService,
        mut contact: FrontendContact,
        password: Option<&str>,
        preserve_manual_card: bool,
        before_commit: impl FnOnce(
            &mut AppService,
            &mut Option<humoco_wot::TrustRegister>,
            &mut EncryptedBatchCommit,
        ) -> Result<(), AppFacadeError>,
    ) -> Result<(), AppFacadeError> {
        let validated_did = human_money_core::ValidatedUserId::parse(&contact.did)
            .map_err(|e| AppFacadeError::ValidationFailed(format!("Invalid contact DID: {}", e)))?;
        contact.did = validated_did.into_string();
        validate_contact_sync_descriptors(&contact.syncs)
            .map_err(|e| AppFacadeError::ValidationFailed(e.message))?;
        let mut address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)?;

        let can_did = canonicalize_did(&contact.did)
            .map_err(|e| AppFacadeError::ValidationFailed(format!("Invalid contact DID: {}", e)))?;

        let own_ids: Option<(String, String)> = service
            .with_wallet(|w| {
                let uid = w.get_user_id().trim().to_string();
                let can = canonicalize_did(&uid).unwrap_or_else(|_| uid.clone());
                (uid, can)
            })
            .ok();

        if own_ids
            .as_ref()
            .is_some_and(|(own_uid, _)| contact.did == *own_uid)
        {
            return Err(AppFacadeError::ValidationFailed(
                "Cannot add your own active profile as a contact".to_string(),
            ));
        }

        let is_own_root = own_ids
            .as_ref()
            .is_some_and(|(_, own_can)| *own_can == can_did);

        if is_own_root && contact.trust_level.is_some() {
            return Err(AppFacadeError::ValidationFailed(
                "Self trust forbidden: cannot assign a trust level to your own subaccount"
                    .to_string(),
            ));
        }

        // Keep distinct prefixes separate for both own subaccounts and external contacts;
        // each prefix can carry its own sync endpoints while sharing root WoT trust.
        let keys_to_remove: Vec<String> = address_book
            .contacts
            .keys()
            .filter(|k| *k != &contact.did && k.trim() == contact.did)
            .cloned()
            .collect();

        let is_existing =
            address_book.contacts.contains_key(&contact.did) || !keys_to_remove.is_empty();

        let mut old_contact_syncs: Vec<crate::models::FrontendSyncDescriptor> = address_book
            .contacts
            .get(&contact.did)
            .map(|c| c.syncs.clone())
            .unwrap_or_default();

        let mut existing_profile: Option<crate::models::FrontendUserProfile> = address_book
            .contacts
            .get(&contact.did)
            .map(|c| c.profile.clone());

        if let Some(old) = address_book.contacts.get(&contact.did) {
            if contact.added_at.is_empty() {
                contact.added_at = old.added_at.clone();
            }
            if !contact.has_peer_list && old.has_peer_list {
                contact.has_peer_list = true;
                contact.peer_list_updated_at =
                    contact.peer_list_updated_at.or(old.peer_list_updated_at);
            }
        }

        for k in keys_to_remove {
            if let Some(old) = address_book.contacts.remove(&k) {
                if old_contact_syncs.is_empty() {
                    old_contact_syncs = old.syncs;
                }
                if contact.added_at.is_empty() {
                    contact.added_at = old.added_at;
                }
                if !contact.has_peer_list && old.has_peer_list {
                    contact.has_peer_list = true;
                    contact.peer_list_updated_at =
                        contact.peer_list_updated_at.or(old.peer_list_updated_at);
                }
                if existing_profile.is_none() {
                    existing_profile = Some(old.profile);
                }
            }
        }

        if let Some(ref old_prof) = existing_profile {
            if contact.profile.protocol_version.is_none() {
                contact.profile.protocol_version = old_prof.protocol_version.clone();
            }
            if contact.profile.id.is_none() {
                contact.profile.id = old_prof.id.clone();
            }
            if contact.profile.community.is_none() {
                contact.profile.community = old_prof.community.clone();
            }
            if contact.profile.address.is_none() {
                contact.profile.address = old_prof.address.clone();
            }
            if contact.profile.gender.is_none() {
                contact.profile.gender = old_prof.gender.clone();
            }
            if contact.profile.email.is_none() {
                contact.profile.email = old_prof.email.clone();
            }
            if contact.profile.phone.is_none() {
                contact.profile.phone = old_prof.phone.clone();
            }
            if contact.profile.coordinates.is_none() {
                contact.profile.coordinates = old_prof.coordinates.clone();
            }
            if contact.profile.url.is_none() {
                contact.profile.url = old_prof.url.clone();
            }
            if contact.profile.service_offer.is_none() {
                contact.profile.service_offer = old_prof.service_offer.clone();
            }
            if contact.profile.needs.is_none() {
                contact.profile.needs = old_prof.needs.clone();
            }
            if contact.profile.picture_url.is_none() {
                contact.profile.picture_url = old_prof.picture_url.clone();
            }
        }

        // Authoritative sync endpoints:
        // For existing contacts, contact.syncs passed from caller/UI is authoritative.
        // We do NOT revive old sync endpoints.
        // For new contacts, if contact.syncs is empty, populate from CardStore if a card exists.
        if is_existing {
            contact.syncs = merge_contact_syncs(&[], &contact.syncs);
            contact.syncs.truncate(2);
        } else {
            let card_syncs: Vec<crate::models::FrontendSyncDescriptor> =
                match self.load_card_store_or_cached(service, password) {
                    Ok(store) => store
                        .cards
                        .get(&contact.did)
                        .map(|sc| {
                            sanitize_untrusted_sync_descriptors(
                                &sc.card.payload.drops,
                                humoco_market::MAX_CARD_DROPS,
                            )
                        })
                        .unwrap_or_default(),
                    Err(e) => {
                        log::warn!("Failed to load CardStore during save_contact: {}", e);
                        Vec::new()
                    }
                };

            let mut all_syncs = Vec::new();
            all_syncs.extend(contact.syncs);
            all_syncs.extend(card_syncs);
            contact.syncs = merge_contact_syncs(&[], &all_syncs);
            contact.syncs.truncate(2);
        }

        // Synchronize TrustRegister if wallet is unlocked (skip for own subaccounts: self-trust is forbidden)
        let mut updated_wot_reg: Option<humoco_wot::TrustRegister> = None;
        if is_own_root {
            contact.trust_level = None;
            contact.desired_hops = None;
        } else if service.with_wallet(|_| ()).is_ok() {
            let mut reg = self.get_or_load_wot_register(service, password)?;
            let now_secs = Utc::now().timestamp().max(0) as u64;
            let prev_level = reg.own_contacts.get(&can_did).map(|e| e.level).unwrap_or(0);
            if !is_existing && contact.trust_level.is_none() {
                if let Some(existing_entry) = reg.own_contacts.get(&can_did) {
                    contact.trust_level = Some(existing_entry.level);
                    contact.desired_hops = Some(existing_entry.desired_hops);
                }
            }
            let new_eff_level = contact.trust_level.unwrap_or(0);
            let mut wot_modified = false;

            if let Some(level) = contact.trust_level {
                if level > 3 {
                    return Err(AppFacadeError::ValidationFailed(format!(
                        "Invalid trust level: {}",
                        level
                    )));
                }
                let hops = if level == 0 {
                    0
                } else {
                    reg.policy.own_list_max_hops
                };
                reg.set_contact_trust(&can_did, level, hops, now_secs)
                    .map_err(|e| AppFacadeError::ValidationFailed(e.to_string()))?;
                if level == 0 {
                    self.prune_local_data_on_trust_withdrawal(
                        service,
                        &mut reg,
                        Some(&can_did),
                        password,
                        preserve_manual_card,
                    );
                } else {
                    reg.prune_unreachable_lists();
                }
                wot_modified = true;
                contact.trust_level = Some(level);
                contact.desired_hops = Some(hops);
            } else {
                let removed = reg
                    .remove_contact_trust(&can_did)
                    .map_err(|e| AppFacadeError::ValidationFailed(e.to_string()))?;
                if removed {
                    self.prune_local_data_on_trust_withdrawal(
                        service,
                        &mut reg,
                        Some(&can_did),
                        password,
                        preserve_manual_card,
                    );
                    wot_modified = true;
                }
            }

            if new_eff_level < prev_level {
                crate::services::market_service::bump_own_card_version_for_eviction(
                    self, service, password,
                );
            }
            if wot_modified {
                updated_wot_reg = Some(reg);
            }
        }

        let mut batch = EncryptedBatchCommit::new();
        before_commit(service, &mut updated_wot_reg, &mut batch)?;
        if let Some(ref mut reg) = updated_wot_reg {
            if reg.own_list_dirty || contact.trust_level.is_some() {
                self.ensure_wot_list_signed(service, reg)?;
            }
        }

        let removed_endpoints: Vec<crate::models::FrontendSyncDescriptor> = old_contact_syncs
            .into_iter()
            .filter(|old_ep| {
                !contact.syncs.iter().any(|new_ep| {
                    endpoints_overlap(
                        &old_ep.read_url,
                        &old_ep.write_url,
                        &new_ep.read_url,
                        &new_ep.write_url,
                    )
                })
            })
            .collect();

        if !is_own_root {
            for sibling in address_book.contacts.values_mut() {
                if canonicalize_did(&sibling.did).ok().as_deref() == Some(can_did.as_str()) {
                    sibling.trust_level = contact.trust_level;
                    sibling.desired_hops = contact.desired_hops;
                }
            }
        }

        address_book.contacts.insert(contact.did.clone(), contact);

        batch.contacts(address_book.clone());
        if let Some(reg) = updated_wot_reg {
            batch.wot(reg);
        }
        batch.commit(self, service, password)?;

        self.prune_orphaned_market_selected_syncs(
            service,
            &address_book,
            &removed_endpoints,
            password,
        );
        Ok(())
    }

    pub fn delete_contact(
        &self,
        service: &mut AppService,
        did: &str,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        self.delete_contact_with_options(service, did, password, false)
    }

    pub fn delete_contact_with_options(
        &self,
        service: &mut AppService,
        did: &str,
        password: Option<&str>,
        preserve_manual_card: bool,
    ) -> Result<(), AppFacadeError> {
        let did = did.trim();
        let mut address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)?;

        let can_target = canonicalize_did(did).ok();
        let own_can_did = service
            .with_wallet(|w| canonicalize_did(w.get_user_id()).ok())
            .ok()
            .flatten();
        let is_own_root = can_target.is_some() && own_can_did == can_target;

        let keys_to_remove: Vec<String> = address_book
            .contacts
            .keys()
            .filter(|k| k.trim() == did)
            .cloned()
            .collect();

        let mut removed_endpoints = Vec::new();
        let mut changed = false;
        for k in keys_to_remove {
            if let Some(removed_contact) = address_book.contacts.remove(&k) {
                removed_endpoints.extend(removed_contact.syncs);
                changed = true;
            }
        }

        let has_remaining_sibling = can_target.as_ref().is_some_and(|ct| {
            address_book
                .contacts
                .keys()
                .any(|k| canonicalize_did(k).ok().as_ref() == Some(ct))
        });

        if !is_own_root && !has_remaining_sibling && service.with_wallet(|_| ()).is_ok() {
            let mut reg = self.get_or_load_wot_register(service, password)?;
            let target_key = can_target.as_deref().unwrap_or(did);
            let prev_level = reg
                .own_contacts
                .get(target_key)
                .map(|e| e.level)
                .unwrap_or(0);
            let removed = reg
                .remove_contact_trust(target_key)
                .map_err(|e| AppFacadeError::ValidationFailed(e.to_string()))?;
            if removed {
                self.prune_local_data_on_trust_withdrawal(
                    service,
                    &mut reg,
                    Some(target_key),
                    password,
                    preserve_manual_card,
                );
                if reg.own_list_dirty {
                    self.ensure_wot_list_signed(service, &mut reg)?;
                }
                self.save_wot_register(service, &reg, password)?;
                if prev_level >= 1 {
                    crate::services::market_service::bump_own_card_version_for_eviction(
                        self, service, password,
                    );
                }
            }
        }

        if changed {
            save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password)
                .map_err(|e| app_state_err(format!("Failed to save address book: {}", e)))?;

            self.set_cached_contacts_for_service(service, Some(address_book.clone()));
            self.prune_orphaned_market_selected_syncs(
                service,
                &address_book,
                &removed_endpoints,
                password,
            );
        }

        Ok(())
    }

    pub fn try_import_piggybacked_contact_card(
        &self,
        service: &mut AppService,
        contact_card_json: Option<&str>,
        bundle_sender_id: Option<&str>,
        password: Option<&str>,
    ) {
        let json = match contact_card_json {
            Some(j) if !j.trim().is_empty() => j.trim(),
            _ => return,
        };

        let env: humoco_market::CardEnvelope = match serde_json::from_str(json) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Malformed piggybacked CardEnvelope: {}", e);
                return;
            }
        };

        let ui_prefs = match self.get_or_load_ui_preferences(service, password) {
            Ok(p) => p,
            Err(e) => {
                log::warn!(
                    "Failed to load ui_preferences for piggybacked contact card: {}",
                    e
                );
                return;
            }
        };
        let max_stored_cards = ui_prefs.max_stored_cards.clamp(100, 10_000);
        let now = Utc::now().timestamp().max(0) as u64;

        let mut store: humoco_market::CardStore =
            match self.load_card_store_or_cached(service, password) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!(
                        "Failed to load CardStore for piggybacked contact card: {}",
                        e
                    );
                    return;
                }
            };

        let outcome = service.with_wallet(|wallet| {
            let is_offender = |did: &str| -> bool {
                use human_money_core::models::conflict::TrustStatus;
                matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_))
            };
            let own_coords = wallet
                .profile
                .coordinates
                .as_deref()
                .and_then(humoco_market::parse_coordinates_pair);
            humoco_market::accept_envelope_with_writer(
                &mut store,
                &env,
                bundle_sender_id,
                now,
                max_stored_cards,
                own_coords,
                &is_offender,
            )
        });

        match outcome {
            Ok(humoco_market::AcceptOutcome::Accepted { did, replaced }) => {
                log::info!("Accepted piggybacked contact card (replaced={})", replaced);
                if let Err(e) = self.save_card_store(service, &store, password) {
                    log::warn!(
                        "Failed to save CardStore after accepting piggybacked card: {}",
                        e
                    );
                }
                if let Some(stored) = store.cards.get(&did) {
                    let card_syncs = sanitize_untrusted_sync_descriptors(
                        &stored.card.payload.drops,
                        humoco_market::MAX_CARD_DROPS,
                    );
                    let card_profile = humoco_market::extract_public_profile(&stored.card);
                    if let Err(e) = self.update_contact_from_card(
                        service,
                        &did,
                        &card_syncs,
                        Some(&card_profile),
                        password,
                    ) {
                        log::warn!(
                            "Failed to persist piggybacked contact update for {}: {}",
                            did,
                            e.message
                        );
                    }
                }
            }
            Ok(other) => {
                log::debug!("Piggybacked contact card outcome: {:?}", other);
            }
            Err(e) => {
                log::warn!(
                    "Error verifying reputation for piggybacked contact card: {}",
                    e
                );
            }
        }
    }

    pub fn update_contact_from_card(
        &self,
        service: &mut AppService,
        card_did: &str,
        card_syncs: &[crate::models::FrontendSyncDescriptor],
        card_profile: Option<&human_money_core::models::profile::PublicProfile>,
        password: Option<&str>,
    ) -> Result<bool, crate::models::FrontendError> {
        let trimmed_card_did = card_did.trim();
        let sanitized_syncs =
            sanitize_untrusted_frontend_sync_descriptors(card_syncs, humoco_market::MAX_CARD_DROPS);
        if (sanitized_syncs.is_empty() && card_profile.is_none()) || trimmed_card_did.is_empty() {
            return Ok(false);
        }

        let mut address_book: FrontendAddressBook =
            load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password)
                .map_err(crate::models::FrontendError::from)?;

        if apply_contact_update_from_card(
            &mut address_book,
            trimmed_card_did,
            &sanitized_syncs,
            card_profile,
        ) {
            save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password)
                .map_err(crate::models::FrontendError::from)?;
            self.set_cached_contacts_for_service(service, Some(address_book));
            return Ok(true);
        }
        Ok(false)
    }

    pub fn record_contact_sync_health_batch(
        &self,
        service: &mut AppService,
        results: &[(String, bool)],
        now_secs: u64,
        password: Option<&str>,
    ) -> bool {
        if results.is_empty() {
            return false;
        }

        let mut address_book: FrontendAddressBook =
            match load_encrypted_json_or_default(service, CONTACTS_DATA_NAME, password) {
                Ok(ab) => ab,
                Err(e) => {
                    log::warn!(
                        "Failed to load address book in record_contact_sync_health_batch: {}",
                        e
                    );
                    return false;
                }
            };

        let mut policy_opt = match self.get_or_load_security_policy(service, password) {
            Ok(p) => Some(p),
            Err(e) => {
                log::warn!(
                    "Failed to load security policy during record_contact_sync_health_batch: {}",
                    e
                );
                None
            }
        };

        let (updated, policy_updated) = apply_contact_sync_health_batch(
            &mut address_book,
            policy_opt.as_mut(),
            results,
            now_secs,
        );

        if updated {
            let _ = save_encrypted_json(service, CONTACTS_DATA_NAME, &address_book, password);
            self.set_cached_contacts_for_service(service, Some(address_book));
        }

        if let (true, Some(policy)) = (policy_updated, policy_opt) {
            let _ = self.save_security_policy(service, policy, password);
        }
        updated
    }

    pub fn prune_orphaned_market_selected_syncs(
        &self,
        service: &mut AppService,
        address_book: &FrontendAddressBook,
        removed_endpoints: &[crate::models::FrontendSyncDescriptor],
        password: Option<&str>,
    ) {
        if removed_endpoints.is_empty() {
            return;
        }

        let mut policy = match self.get_or_load_security_policy(service, password) {
            Ok(p) => p,
            Err(_) => return,
        };

        if policy.market_selected_syncs.is_empty() {
            return;
        }

        let admin_opt: Option<StoredSyncAdmin> = match load_encrypted_json_opt::<
            Option<StoredSyncAdmin>,
        >(
            service, MARKET_SYNC_ADMIN_KEY, password
        ) {
            Ok(opt) => opt.flatten(),
            Err(e) => {
                log::warn!(
                    "Failed to load {} during prune_orphaned_market_selected_syncs: {}",
                    MARKET_SYNC_ADMIN_KEY,
                    e
                );
                return;
            }
        };

        let initial_count = policy.market_selected_syncs.len();
        policy.market_selected_syncs.retain(|sel| {
            let sr = normalize_sync_url(&sel.read_url);
            let sw = normalize_sync_url(&sel.write_url);
            if sr.is_empty() && sw.is_empty() {
                return false;
            }

            let was_removed = removed_endpoints
                .iter()
                .any(|rem| endpoints_overlap(&rem.read_url, &rem.write_url, &sr, &sw));

            if !was_removed {
                return true;
            }

            // 1. Matches StoredSyncAdmin?
            let matches_admin = admin_opt.as_ref().is_some_and(|a| {
                endpoints_overlap(&a.descriptor.read_url, &a.descriptor.write_url, &sr, &sw)
            });

            if matches_admin {
                return true;
            }

            // 2. Matches any remaining contact's syncs?
            let matches_contact = address_book.contacts.values().any(|c| {
                c.syncs
                    .iter()
                    .any(|ep| endpoints_overlap(&ep.read_url, &ep.write_url, &sr, &sw))
            });

            if matches_contact {
                return true;
            }

            // 3. Matches any policy.public_syncs?
            policy
                .public_syncs
                .iter()
                .any(|ps| endpoints_overlap(&ps.read_url, &ps.write_url, &sr, &sw))
        });

        if policy.market_selected_syncs.len() != initial_count {
            let updated_syncs = policy.market_selected_syncs.clone();
            let _ = self.save_security_policy(service, policy, password);
            crate::services::market_service::sync_own_card_drops_on_settings_change(
                self,
                service,
                &updated_syncs,
                password,
            );
        }
    }
}

pub(super) fn enrich_contact_with_wot(
    contact: &mut FrontendContact,
    wot_opt: Option<&humoco_wot::TrustRegister>,
) {
    if let Some(reg) = wot_opt {
        if let Ok(can_did) = canonicalize_did(&contact.did) {
            if reg.self_did == can_did {
                contact.trust_level = None;
                contact.desired_hops = None;
                contact.has_peer_list = false;
                contact.peer_list_updated_at = None;
                return;
            }
            contact.trust_level = reg.own_contacts.get(&can_did).map(|e| e.level);
            contact.desired_hops = reg.own_contacts.get(&can_did).map(|e| e.desired_hops);
            contact.has_peer_list = reg.raw_list_store.contains_key(&can_did);
            contact.peer_list_updated_at = reg.raw_list_store.get(&can_did).map(|l| l.iat);
        }
    }
}

fn format_contact_display_name(c: &FrontendContact) -> Option<String> {
    let first = c.profile.first_name.as_deref().unwrap_or("").trim();
    let last = c.profile.last_name.as_deref().unwrap_or("").trim();
    let name = match (first.is_empty(), last.is_empty()) {
        (false, false) => format!("{} {}", first, last),
        (false, true) => first.to_string(),
        (true, false) => last.to_string(),
        (true, true) => c
            .profile
            .organization
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string(),
    };
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

pub(super) fn lookup_contact_name(
    contacts_map: Option<&FrontendAddressBook>,
    target_did: &str,
) -> Option<String> {
    let book = contacts_map?;
    let trimmed_target = target_did.trim();
    if let Some((_, exact_contact)) = book
        .contacts
        .iter()
        .find(|(k, _)| k.trim() == trimmed_target)
    {
        if let Some(name) = format_contact_display_name(exact_contact) {
            return Some(name);
        }
    }
    let can_target = canonicalize_did(trimmed_target).ok()?;
    for c in book.contacts.values() {
        if canonicalize_did(&c.did).ok().as_ref() == Some(&can_target) {
            if let Some(name) = format_contact_display_name(c) {
                return Some(name);
            }
        }
    }
    None
}

/// Applies disclosed business card profile fields to an existing address-book contact.
/// Always updates `picture_url` when present on the card, and populates missing names
/// without overwriting custom names already set by the user.
pub fn apply_card_profile_to_contact(
    contact: &mut FrontendContact,
    card_profile: &human_money_core::models::profile::PublicProfile,
) -> bool {
    let mut changed = false;

    if let Some(ref pic) = card_profile.picture_url {
        let trimmed = pic.trim();
        if !trimmed.is_empty() && contact.profile.picture_url.as_deref() != Some(trimmed) {
            contact.profile.picture_url = Some(trimmed.to_string());
            changed = true;
        }
    }

    let has_existing_name = contact
        .profile
        .first_name
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
        || contact
            .profile
            .last_name
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
        || contact
            .profile
            .organization
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());

    if !has_existing_name {
        if let Some(ref first) = card_profile.first_name {
            if !first.trim().is_empty() {
                contact.profile.first_name = Some(first.trim().to_string());
                changed = true;
            }
        }
        if let Some(ref last) = card_profile.last_name {
            if !last.trim().is_empty() {
                contact.profile.last_name = Some(last.trim().to_string());
                changed = true;
            }
        }
        if let Some(ref org) = card_profile.organization {
            if !org.trim().is_empty() {
                contact.profile.organization = Some(org.trim().to_string());
                changed = true;
            }
        }
    }

    changed
}

pub(crate) fn apply_contact_update_from_card(
    address_book: &mut FrontendAddressBook,
    card_did: &str,
    card_syncs: &[crate::models::FrontendSyncDescriptor],
    card_profile: Option<&human_money_core::models::profile::PublicProfile>,
) -> bool {
    let trimmed_card_did = card_did.trim();
    let sanitized_syncs =
        sanitize_untrusted_frontend_sync_descriptors(card_syncs, humoco_market::MAX_CARD_DROPS);
    if (sanitized_syncs.is_empty() && card_profile.is_none()) || trimmed_card_did.is_empty() {
        return false;
    }
    let mut updated = false;
    for (k, contact) in address_book.contacts.iter_mut() {
        if k.trim() == trimmed_card_did {
            if !sanitized_syncs.is_empty() {
                let merged = merge_contact_syncs(&contact.syncs, &sanitized_syncs);
                if contact.syncs != merged {
                    contact.syncs = merged;
                    updated = true;
                }
            }
            if let Some(prof) = card_profile {
                if apply_card_profile_to_contact(contact, prof) {
                    updated = true;
                }
            }
        }
    }
    updated
}

pub(crate) fn apply_contact_sync_health_batch(
    address_book: &mut FrontendAddressBook,
    mut policy_opt: Option<&mut SecurityPolicy>,
    results: &[(String, bool)],
    now_secs: u64,
) -> (bool, bool) {
    if results.is_empty() {
        return (false, false);
    }
    let mut updated = false;
    for (url, success) in results {
        let norm_target = crate::services::common::normalize_sync_url(url);
        if norm_target.is_empty() {
            continue;
        }
        for contact in address_book.contacts.values_mut() {
            for sync_ep in &mut contact.syncs {
                let norm_read = crate::services::common::normalize_sync_url(&sync_ep.read_url);
                let norm_write = crate::services::common::normalize_sync_url(&sync_ep.write_url);
                if norm_read == norm_target || norm_write == norm_target {
                    if sync_ep.status == "deprecating"
                        && sync_ep.sunset_at.is_some_and(|eff| now_secs >= eff)
                    {
                        sync_ep.status = "revoked".to_string();
                    }
                    let is_revocation_state =
                        sync_ep.status == "deprecating" || sync_ep.status == "revoked";
                    if *success {
                        sync_ep.last_seen_at = Some(now_secs);
                        sync_ep.fail_count = 0;
                        if !is_revocation_state {
                            if sync_ep.status == "active" {
                                // remains active
                            } else if sync_ep.status == "unreachable" {
                                let is_selected = policy_opt
                                    .as_deref()
                                    .map(|s| {
                                        s.market_selected_syncs.iter().any(|sel| {
                                            endpoints_overlap(
                                                &sel.read_url,
                                                &sel.write_url,
                                                &norm_read,
                                                &norm_write,
                                            )
                                        })
                                    })
                                    .unwrap_or(false);
                                if is_selected {
                                    sync_ep.status = "active".to_string();
                                } else {
                                    sync_ep.status = "unverified".to_string();
                                }
                            } else if sync_ep.status.is_empty() {
                                sync_ep.status = "unverified".to_string();
                            }
                        }
                    } else {
                        sync_ep.last_error_at = Some(now_secs);
                        sync_ep.fail_count = sync_ep.fail_count.saturating_add(1);
                        if !is_revocation_state
                            && (sync_ep.status != "pending_auth" || sync_ep.fail_count >= 3)
                        {
                            sync_ep.status = "unreachable".to_string();
                        }
                    }
                    updated = true;
                }
            }
        }
    }

    let mut policy_updated = false;
    if let Some(ref mut policy) = policy_opt {
        for (url, success) in results {
            let norm_target = crate::services::common::normalize_sync_url(url);
            if norm_target.is_empty() {
                continue;
            }
            for sel in &mut policy.market_selected_syncs {
                let norm_read = crate::services::common::normalize_sync_url(&sel.read_url);
                let norm_write = crate::services::common::normalize_sync_url(&sel.write_url);
                if norm_read == norm_target || norm_write == norm_target {
                    if sel.status == "deprecating"
                        && sel.sunset_at.is_some_and(|eff| now_secs >= eff)
                    {
                        sel.status = "revoked".to_string();
                    }
                    let is_revocation_state =
                        sel.status == "deprecating" || sel.status == "revoked";
                    if *success {
                        sel.last_seen_at = Some(now_secs);
                        sel.fail_count = 0;
                        if !is_revocation_state {
                            sel.status = "active".to_string();
                        }
                    } else {
                        sel.last_error_at = Some(now_secs);
                        sel.fail_count = sel.fail_count.saturating_add(1);
                        if !is_revocation_state {
                            sel.status = "unreachable".to_string();
                        }
                    }
                    policy_updated = true;
                }
            }
        }
    }
    (updated, policy_updated)
}

pub fn merge_contact_syncs(
    existing: &[crate::models::FrontendSyncDescriptor],
    card_syncs: &[crate::models::FrontendSyncDescriptor],
) -> Vec<crate::models::FrontendSyncDescriptor> {
    let mut result: Vec<crate::models::FrontendSyncDescriptor> = Vec::with_capacity(2);

    let merge_entry = |res: &mut Vec<crate::models::FrontendSyncDescriptor>,
                       d: &crate::models::FrontendSyncDescriptor| {
        let w = crate::services::common::normalize_sync_url(&d.write_url);
        let r = crate::services::common::normalize_sync_url(&d.read_url);
        if w.is_empty() && r.is_empty() {
            return;
        }

        if let Some(target) = res.iter_mut().find(|existing_entry| {
            let ew = crate::services::common::normalize_sync_url(&existing_entry.write_url);
            let er = crate::services::common::normalize_sync_url(&existing_entry.read_url);
            (!w.is_empty() && !ew.is_empty() && w == ew)
                || (!r.is_empty() && !er.is_empty() && r == er)
        }) {
            if target.read_url.is_empty() && !d.read_url.is_empty() {
                target.read_url = d.read_url.clone();
            }
            if target.drop_pub.is_empty() && !d.drop_pub.is_empty() {
                target.drop_pub = d.drop_pub.clone();
            }
            if target.write_url.is_empty() && !d.write_url.is_empty() {
                target.write_url = d.write_url.clone();
            }
            if target.slot_id.is_none() && d.slot_id.is_some() {
                target.slot_id = d.slot_id.clone();
            }
            if target.status == "unverified" && d.status != "unverified" {
                target.status = d.status.clone();
                target.last_probed_at = d.last_probed_at;
                target.last_seen_at = d.last_seen_at;
                target.last_error_at = d.last_error_at;
                target.fail_count = d.fail_count;
            }
        } else if res.len() < 2 {
            res.push(d.clone());
        }
    };

    // Newest card sync endpoints take precedence, preserving priority order
    for d in card_syncs {
        merge_entry(&mut result, d);
    }
    for d in existing {
        merge_entry(&mut result, d);
    }

    result
}

pub fn apply_revocation_notice<T: SyncRevocationFields>(
    ep: &mut T,
    notice: &crate::sync_client::SyncRevocationNotice,
    now_secs: u64,
) {
    *ep.status_mut() = if now_secs >= notice.effective_at {
        "revoked".to_string()
    } else {
        "deprecating".to_string()
    };
    *ep.sunset_at_mut() = Some(notice.effective_at);
    *ep.revocation_reason_mut() = notice.reason.clone();
    *ep.replacement_mut() = notice.replacement.clone().map(Into::into);
}

/// Returns `true` if `ep` has already transitioned into an effective (irreversible) revocation.
pub fn is_revocation_effective<T: SyncRevocationFields>(ep: &mut T, now_secs: u64) -> bool {
    is_endpoint_revoked(ep.status_mut())
        || ep.sunset_at_mut().is_some_and(|sunset| now_secs >= sunset)
}

pub fn clear_revocation<T: SyncRevocationFields>(
    ep: &mut T,
    fallback_status: &str,
    now_secs: u64,
) -> bool {
    // Once a revocation has taken effect (status == "revoked" or now_secs >= sunset_at),
    // it remains permanently sticky so a compromised server cannot un-revoke via HTTP 404.
    if is_revocation_effective(ep, now_secs) {
        return false;
    }
    let is_deprecating = ep.status_mut().eq_ignore_ascii_case("deprecating");
    if ep.sunset_at_mut().is_some()
        || ep.revocation_reason_mut().is_some()
        || ep.replacement_mut().is_some()
        || is_deprecating
    {
        if is_deprecating {
            *ep.status_mut() = fallback_status.to_string();
        }
        *ep.sunset_at_mut() = None;
        *ep.revocation_reason_mut() = None;
        *ep.replacement_mut() = None;
        true
    } else {
        false
    }
}

pub fn apply_revocation_notice_to_address_book(
    address_book: &mut crate::models::FrontendAddressBook,
    old_read_url: &str,
    notice: &crate::sync_client::SyncRevocationNotice,
    now_secs: u64,
) -> bool {
    let old_norm = crate::services::common::normalize_sync_url(old_read_url);
    let notice_pub = notice.drop_pub.trim().to_lowercase();
    let is_revoked_now = now_secs >= notice.effective_at;
    let mut updated = false;

    for contact in address_book.contacts.values_mut() {
        let syncs_len = contact.syncs.len();
        let existing_endpoints: Vec<(String, String)> = contact
            .syncs
            .iter()
            .map(|o| {
                (
                    crate::services::common::normalize_sync_url(&o.read_url),
                    crate::services::common::normalize_sync_url(&o.write_url),
                )
            })
            .collect();
        let mut idx_to_replace: Option<(usize, crate::models::FrontendSyncDescriptor)> = None;
        let mut idx_to_append: Option<crate::models::FrontendSyncDescriptor> = None;
        let mut idx_to_remove: Option<usize> = None;

        for (idx, ep) in contact.syncs.iter_mut().enumerate() {
            if normalize_sync_url(&ep.read_url) != old_norm {
                continue;
            }
            if !drop_pub_matches(&ep.drop_pub, &notice_pub) {
                continue;
            }

            let prev_status = ep.status.clone();
            let prev_sunset = ep.sunset_at;
            let prev_reason = ep.revocation_reason.clone();
            let prev_rep = ep.replacement.clone();

            apply_revocation_notice(ep, notice, now_secs);
            if ep.status != prev_status
                || ep.sunset_at != prev_sunset
                || ep.revocation_reason != prev_reason
                || ep.replacement != prev_rep
            {
                updated = true;
            }

            if let Some(ref rep) = notice.replacement {
                let rep_r = crate::services::common::normalize_sync_url(&rep.read_url);
                let rep_w = crate::services::common::normalize_sync_url(&rep.write_url);
                let ep_r = crate::services::common::normalize_sync_url(&ep.read_url);
                let ep_w = crate::services::common::normalize_sync_url(&ep.write_url);

                // Anti-loop: ignore self-redirects or targets already present on this contact
                let is_self_redirect = rep_r == ep_r || (!rep_w.is_empty() && rep_w == ep_w);
                let already_present = existing_endpoints
                    .iter()
                    .any(|(or, ow)| *or == rep_r || (!rep_w.is_empty() && *ow == rep_w));

                if !is_self_redirect && !already_present {
                    let new_desc = crate::models::FrontendSyncDescriptor {
                        write_url: rep.write_url.clone(),
                        read_url: rep.read_url.clone(),
                        slot_id: None, // Force index resolution on new server until contact's new SdCard arrives
                        drop_pub: rep.drop_pub.clone(),
                        status: "unverified".to_string(),
                        ..Default::default()
                    };
                    if is_revoked_now {
                        idx_to_replace = Some((idx, new_desc));
                    } else if syncs_len < 2 {
                        idx_to_append = Some(new_desc);
                    }
                } else if is_revoked_now && !is_self_redirect && already_present {
                    // Grace-period appended replacement is already present: prune the expired revoked slot.
                    idx_to_remove = Some(idx);
                }
            }
        }

        if let Some((idx, new_desc)) = idx_to_replace {
            contact.syncs[idx] = new_desc;
            updated = true;
        } else if let Some(rm_idx) = idx_to_remove {
            contact.syncs.remove(rm_idx);
            updated = true;
        } else if let Some(new_desc) = idx_to_append {
            if contact.syncs.len() < 2 {
                contact.syncs.push(new_desc);
                updated = true;
            }
        }
    }

    updated
}
