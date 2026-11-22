// src-tauri/src/state/wot_repo.rs
use super::contacts_repo::lookup_contact_name;
use super::{app_state_err, load_encrypted_json_opt, save_encrypted_json, AppState, WOT_DATA_NAME};
use chrono::Utc;
use human_money_core::app_service::{AppFacadeError, AppService};
use log::info;

impl AppState {
    // 7. Web of Trust (WoT) Helpers
    pub fn prune_local_data_on_trust_withdrawal(
        &self,
        service: &mut AppService,
        reg: &mut humoco_wot::TrustRegister,
        revoked_did: Option<&str>,
        password: Option<&str>,
        preserve_manual_card: bool,
    ) {
        let can_revoked = revoked_did.and_then(|d| canonicalize_did(d).ok());
        if let Some(ref can) = can_revoked {
            reg.raw_list_store.remove(can);
            reg.invalidate_graph_cache();
        }
        reg.prune_unreachable_lists();
        reg.clear_drop_sent_log(None);

        match self.load_card_store_or_cached(service, password) {
            Ok(mut store) => {
                let before = store.cards.len();
                if !preserve_manual_card {
                    if let Some(ref can) = can_revoked {
                        store.cards.retain(|_, sc| {
                            canonicalize_did(&sc.card.payload.did).ok().as_deref()
                                != Some(can.as_str())
                        });
                    }
                }
                store.cards.retain(|_, sc| {
                    if sc.hops < 2 {
                        return true;
                    }
                    if preserve_manual_card
                        && can_revoked.as_deref().is_some_and(|rev| {
                            canonicalize_did(&sc.card.payload.did).ok().as_deref() == Some(rev)
                        })
                    {
                        return true;
                    }
                    reg.evaluate_did(&sc.card.payload.did, None)
                        .is_ok_and(|ev| ev.shortest_hops.is_some())
                });
                if store.cards.len() != before {
                    let _ = self.save_card_store(service, &store, password);
                }
            }
            Err(e) => {
                log::warn!(
                    "Failed to load CardStore during trust withdrawal prune: {}",
                    e
                );
            }
        }
    }

    pub fn ensure_wot_list_signed(
        &self,
        service: &mut AppService,
        reg: &mut humoco_wot::TrustRegister,
    ) -> Result<bool, AppFacadeError> {
        let now_secs = Utc::now().timestamp().max(0) as u64;
        service.with_wallet_and_identity(|_wallet, identity| {
            let signer = DidWotSigner::from_identity(identity);
            reg.ensure_own_list_signed(&signer, now_secs)
                .map_err(|e| app_state_err(format!("Failed to sign WoT trust list: {}", e)))
        })?
    }

    pub fn load_wot_register(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<humoco_wot::TrustRegister, AppFacadeError> {
        info!("Loading WoT register from disk...");
        let own_user_id = service.with_wallet(|w| w.get_user_id().to_string())?;
        let own_can_did = canonicalize_did(&own_user_id).map_err(|e| {
            AppFacadeError::ValidationFailed(format!("Invalid own wallet DID: {}", e))
        })?;
        let mut reg = match load_encrypted_json_opt::<humoco_wot::TrustRegister>(
            service,
            WOT_DATA_NAME,
            password,
        )? {
            Some(loaded) => loaded,
            None => {
                info!("No WoT register found, creating new one");
                humoco_wot::TrustRegister::new(&own_can_did, humoco_wot::WotPolicy::default())
                    .map_err(|e| {
                        app_state_err(format!("Failed to initialize TrustRegister: {}", e))
                    })?
            }
        };

        reg.policy.own_list_max_hops = reg.policy.own_list_max_hops.clamp(1, 3);
        reg.policy.max_eval_depth = reg.policy.max_eval_depth.max(5);
        reg.policy.max_import_depth = reg.policy.max_import_depth.max(5);
        for entry in reg.own_contacts.values_mut() {
            if entry.level >= 1 && entry.desired_hops != reg.policy.own_list_max_hops {
                entry.desired_hops = reg.policy.own_list_max_hops;
                reg.own_list_dirty = true;
            }
        }

        self.write_cache_slot_for_service(service, |c| &mut c.wot, Some(reg.clone()));
        Ok(reg)
    }

    pub fn get_or_load_wot_register(
        &self,
        service: &mut AppService,
        password: Option<&str>,
    ) -> Result<humoco_wot::TrustRegister, AppFacadeError> {
        if let Some(reg) = self.cache_wot() {
            return Ok(reg);
        }
        self.load_wot_register(service, password)
    }

    pub fn save_wot_register(
        &self,
        service: &mut AppService,
        reg: &humoco_wot::TrustRegister,
        password: Option<&str>,
    ) -> Result<(), AppFacadeError> {
        save_encrypted_json(service, WOT_DATA_NAME, reg, password)
            .map_err(|e| app_state_err(format!("Failed to save TrustRegister: {}", e)))?;
        self.write_cache_slot_for_service(service, |c| &mut c.wot, Some(reg.clone()));
        Ok(())
    }

    pub fn evaluate_voucher_trust_internal(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        creator_id: &str,
        guarantor_ids: &[&str],
    ) -> crate::models::FrontendVoucherTrustDto {
        use crate::models::FrontendVoucherTrustDto;
        use human_money_core::models::conflict::TrustStatus;

        let own_did_can = canonicalize_did(wallet.get_user_id()).ok();
        let creator_can = canonicalize_did(creator_id).ok();

        if let (Some(own), Some(creator)) = (&own_did_can, &creator_can) {
            if own == creator {
                return FrontendVoucherTrustDto {
                    score: 1.0,
                    trust_percent: 100,
                    is_own_voucher: true,
                    shortest_hops: Some(0),
                    ..Default::default()
                };
            }
        }

        let is_offender = matches!(
            wallet.check_reputation(creator_id),
            TrustStatus::KnownOffender(_)
        );
        if is_offender {
            return FrontendVoucherTrustDto {
                score: 0.0,
                trust_percent: 0,
                is_own_voucher: false,
                blocked_by_offender: true,
                ..Default::default()
            };
        }

        let valid_guarantors: Vec<&str> = guarantor_ids
            .iter()
            .copied()
            .filter(|gid| !matches!(wallet.check_reputation(gid), TrustStatus::KnownOffender(_)))
            .collect();

        let guarantor_canonical_set: std::collections::HashSet<String> = valid_guarantors
            .iter()
            .filter_map(|gid| canonicalize_did(gid).ok())
            .collect();

        let (eval, provider_filter_names, contacts_cache) = {
            let current_gen = self.current_session_generation();
            let caches_guard = self.profile_caches.lock().unwrap();
            let current_caches = caches_guard
                .as_ref()
                .filter(|c| c.generation == current_gen);
            let reg = match current_caches.and_then(|c| c.wot.as_ref()) {
                Some(r) => r,
                None => return FrontendVoucherTrustDto::default(),
            };

            let creator_canonical = creator_can.as_deref().unwrap_or(creator_id);
            let valid_guarantor_canonicals: Vec<String> = valid_guarantors
                .iter()
                .map(|gid| canonicalize_did(gid).unwrap_or_else(|_| gid.to_string()))
                .collect();
            let valid_guarantor_refs: Vec<&str> = valid_guarantor_canonicals
                .iter()
                .map(|s| s.as_str())
                .collect();

            let now_secs = Utc::now().timestamp().max(0) as u64;
            let eval = match reg.evaluate_voucher(
                creator_canonical,
                &valid_guarantor_refs,
                false,
                Some(now_secs),
            ) {
                Ok(e) => e,
                Err(_) => return FrontendVoucherTrustDto::default(),
            };

            let mut provider_filter_names = std::collections::HashMap::new();
            if let Some(best_path) = eval.paths.first() {
                for hop in &best_path.hops {
                    if let Ok(can) = canonicalize_did(hop) {
                        if let Some(pf) = reg.provider_filters.get(&can) {
                            provider_filter_names.insert(can, pf.payload.name.clone());
                        }
                    }
                }
            }
            let contacts_cache = current_caches.and_then(|c| c.contacts.clone());
            (eval, provider_filter_names, contacts_cache)
        };

        let score = eval.score;
        let trust_percent = (score * 100.0).round().clamp(0.0, 100.0) as u8;
        let shortest_hops = eval.shortest_hops;
        let additional_paths_count = eval.paths.len().saturating_sub(1);

        let contacts_ref = contacts_cache.as_ref();

        let mut primary_bridge_did = None;
        let mut primary_bridge_name = None;
        let mut via_guarantor = false;
        let mut best_path_names = Vec::new();

        let resolve_name = |did: &str| -> Option<String> {
            lookup_contact_name(contacts_ref, did).or_else(|| {
                canonicalize_did(did)
                    .ok()
                    .and_then(|can| provider_filter_names.get(&can))
                    .cloned()
            })
        };

        if let Some(best_path) = eval.paths.first() {
            let hops = &best_path.hops;
            if hops.len() >= 2 {
                let hop1 = &hops[1];
                primary_bridge_did = Some(hop1.clone());
                primary_bridge_name = resolve_name(hop1);

                if hops.len() >= 3 {
                    let penultimate = &hops[hops.len() - 2];
                    if let Ok(pen_can) = canonicalize_did(penultimate) {
                        if guarantor_canonical_set.contains(&pen_can) {
                            via_guarantor = true;
                        }
                    }
                }
            }

            if hops.len() > 2 {
                for hop in &hops[1..hops.len() - 1] {
                    let name = resolve_name(hop).unwrap_or_else(|| truncate_did(hop));
                    best_path_names.push(name);
                }
            }
        }

        FrontendVoucherTrustDto {
            score,
            trust_percent,
            is_own_voucher: false,
            blocked_by_offender: false,
            shortest_hops,
            primary_bridge_did,
            primary_bridge_name,
            additional_paths_count,
            via_guarantor,
            best_path_names,
        }
    }

    pub fn evaluate_voucher_trust(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        creator_id: &str,
        signatures: &[human_money_core::models::voucher::VoucherSignature],
    ) -> crate::models::FrontendVoucherTrustDto {
        let guarantor_ids: Vec<&str> = signatures
            .iter()
            .filter(|s| s.role.eq_ignore_ascii_case("guarantor"))
            .map(|s| s.signer_id.as_str())
            .collect();
        self.evaluate_voucher_trust_internal(wallet, creator_id, &guarantor_ids)
    }

    pub fn evaluate_voucher_trust_preview(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        creator_id: &str,
        guarantor_ids: &[String],
    ) -> crate::models::FrontendVoucherTrustDto {
        let refs: Vec<&str> = guarantor_ids.iter().map(|s| s.as_str()).collect();
        self.evaluate_voucher_trust_internal(wallet, creator_id, &refs)
    }

    pub fn evaluate_did_trust_for_endorsement(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        did: &str,
    ) -> (Option<String>, bool, crate::models::FrontendVoucherTrustDto) {
        let wot_trust = self.evaluate_voucher_trust_internal(wallet, did, &[]);
        let own_can = canonicalize_did(wallet.get_user_id()).ok();
        let target_can = canonicalize_did(did).ok();
        let is_own_identity = own_can.is_some() && own_can == target_can;

        let endorser_name = if is_own_identity {
            let first = wallet.profile.first_name.as_deref().unwrap_or("").trim();
            let last = wallet.profile.last_name.as_deref().unwrap_or("").trim();
            match (first.is_empty(), last.is_empty()) {
                (false, false) => Some(format!("{} {}", first, last)),
                (false, true) => Some(first.to_string()),
                (true, false) => Some(last.to_string()),
                (true, true) => None,
            }
        } else {
            let current_gen = self.current_session_generation();
            let caches_guard = self.profile_caches.lock().unwrap();
            let current_caches = caches_guard
                .as_ref()
                .filter(|c| c.generation == current_gen);
            let contacts_ref = current_caches.and_then(|c| c.contacts.as_ref());
            lookup_contact_name(contacts_ref, did).or_else(|| {
                let reg = current_caches.and_then(|c| c.wot.as_ref())?;
                target_can
                    .as_ref()
                    .and_then(|can| reg.provider_filters.get(can))
                    .map(|pf| pf.payload.name.clone())
            })
        };

        (endorser_name, is_own_identity, wot_trust)
    }

    pub fn collect_trusted_endorser_pks(
        &self,
        wallet: &human_money_core::wallet::Wallet,
        candidate_dids: &[String],
    ) -> std::collections::HashSet<[u8; 32]> {
        use human_money_core::models::conflict::TrustStatus;

        let mut trusted = std::collections::HashSet::new();
        let now_secs = Utc::now().timestamp().max(0) as u64;
        let current_gen = self.current_session_generation();
        let caches_guard = self.profile_caches.lock().unwrap();
        let reg_opt = caches_guard
            .as_ref()
            .filter(|c| c.generation == current_gen)
            .and_then(|c| c.wot.as_ref());
        let Some(reg) = reg_opt else {
            return trusted;
        };

        for did in candidate_dids {
            if matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_)) {
                continue;
            }
            let Ok(can) = canonicalize_did(did) else {
                continue;
            };
            let is_trusted = reg.own_contacts.get(&can).is_some_and(|e| e.level >= 1)
                || reg
                    .evaluate_did(&can, Some(now_secs))
                    .is_ok_and(|ev| !ev.blocked_by_offender && ev.score > 0.0);
            if is_trusted {
                if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(did) {
                    trusted.insert(pk.to_bytes());
                }
            }
        }

        trusted
    }

    pub fn try_import_piggybacked_wot(
        &self,
        service: &mut AppService,
        wot_sync_json: &str,
        bundle_sender_id: &str,
        password: Option<&str>,
    ) -> Option<usize> {
        let pkg: humoco_wot::WotSyncPackage = match serde_json::from_str(wot_sync_json) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Malformed piggybacked WoT sync package: {}", e);
                return None;
            }
        };

        let direct_sender = canonicalize_did(bundle_sender_id).ok();
        let now_secs = Utc::now().timestamp().max(0) as u64;

        let mut reg = match self.get_or_load_wot_register(service, password) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Could not load WoT register for piggybacked import: {}", e);
                return None;
            }
        };

        let stats = match reg.import_sync_package(&pkg, direct_sender.as_deref(), now_secs) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to import piggybacked WoT package: {}", e);
                return None;
            }
        };

        if reg.own_list_dirty {
            let _ = self.ensure_wot_list_signed(service, &mut reg);
        }

        if let Err(e) = self.save_wot_register(service, &reg, password) {
            log::warn!(
                "Failed to save WoT register after piggybacked import: {}",
                e
            );
        }

        Some(stats.imported + stats.updated + stats.merged)
    }
}

pub fn canonicalize_did(input: &str) -> Result<String, humoco_wot::WotError> {
    let validated = human_money_core::ValidatedUserId::parse(input)
        .map_err(|e| humoco_wot::WotError::InvalidDid(format!("{input}: {e}")))?;
    humoco_wot::canonicalize_did(validated.canonical_did())
}

pub struct DidWotSigner {
    pub did: String,
    pub signing_key: ed25519_dalek::SigningKey,
}

impl DidWotSigner {
    pub fn from_identity(identity: &human_money_core::models::profile::UserIdentity) -> Self {
        let did = canonicalize_did(&identity.user_id).unwrap_or_else(|_| {
            identity
                .user_id
                .split_once('@')
                .map(|(_, d)| d.to_string())
                .unwrap_or_else(|| identity.user_id.clone())
        });
        Self {
            did,
            signing_key: identity.signing_key.clone(),
        }
    }
}

impl humoco_wot::WotSigner for DidWotSigner {
    fn did(&self) -> &str {
        &self.did
    }

    fn sign(&self, message: &[u8]) -> [u8; 64] {
        human_money_core::services::crypto::sign_ed25519(&self.signing_key, message).to_bytes()
    }
}

pub(super) fn truncate_did(did: &str) -> String {
    let char_count = did.chars().count();
    if char_count > 16 {
        let prefix: String = did.chars().take(8).collect();
        let suffix: String = did.chars().skip(char_count - 4).collect();
        format!("{}…{}", prefix, suffix)
    } else {
        did.to_string()
    }
}
