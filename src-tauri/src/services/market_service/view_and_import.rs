use super::card_signing::rebuild_and_sign_own_card;
use super::storage::*;
use crate::models::{
    FrontendBoardItem, FrontendError, FrontendForwardCardExport, FrontendImportCardResult,
    FrontendMarketPerson, FrontendMarketSnapshot, FrontendResolvedCardRecipient, FrontendTrustTier,
};
use crate::state::{AppState, CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY};

pub fn get_market_view_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketSnapshot, FrontendError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let can_persist = matches!(
        service.get_access_mode(),
        Ok(human_money_core::AccessMode::Full | human_money_core::AccessMode::DailyPay)
    ) && (service.is_session_active() || pw.is_some());

    // Auto-renew own card if published and (validity < 30 days OR active ads count changed)
    let mut stored_opt = load_own_market_profile_state(state, &mut service, pw).ok();

    if can_persist {
        if let Some(ref mut stored) = stored_opt {
            if stored.published {
                let active_user_id = service
                    .with_wallet(|w| w.get_user_id().to_string())
                    .ok()
                    .unwrap_or_default();
                if let Some(ref last_card) = stored.last_card {
                    let active_ads_count = stored
                        .ads
                        .iter()
                        .filter(|a| a.is_published_active(now))
                        .count() as u8;
                    let time_left = last_card.payload.valid_until.saturating_sub(now);
                    let recipient_changed = !active_user_id.trim().is_empty()
                        && last_card.payload.recipient_id() != active_user_id.trim();
                    if time_left < humoco_market::CARD_AUTO_RENEW_THRESHOLD_SECS
                        || last_card.payload.ad_count != active_ads_count
                        || recipient_changed
                    {
                        if let Ok(()) = rebuild_and_sign_own_card(&mut service, stored, now) {
                            let _ = save_own_market_profile_state(state, &mut service, stored, pw);
                        }
                    }
                }
            }
        }
    }

    let ui_prefs = state
        .get_or_load_ui_preferences(&mut service, pw)
        .map_err(FrontendError::from)?;

    let reg = state.get_or_load_wot_register(&mut service, pw).ok();

    let mut store = state
        .load_card_store_or_cached(&mut service, pw)
        .map_err(FrontendError::from)?;

    let before_count = store.cards.len();
    store.cards.retain(|_, c| c.card.payload.valid_until > now);
    if store.cards.len() != before_count && can_persist {
        if let Err(e) = state.save_card_store(&mut service, &store, pw) {
            log::warn!("Failed to persist pruned expired market cards: {}", e);
        }
    }

    let cards_vec: Vec<humoco_market::StoredCard> = store.cards.values().cloned().collect();

    let sync_state = state
        .load_sync_state_or_cached(&mut service, pw)
        .map_err(FrontendError::from)?;

    let known_contact_dids: std::collections::HashSet<String> = {
        let cached = state.cache_contacts();
        let ab = match cached {
            Some(ab) => ab,
            None => crate::state::load_encrypted_json_or_default::<
                crate::models::FrontendAddressBook,
            >(&mut service, CONTACTS_DATA_NAME, pw)
            .map_err(FrontendError::from)?,
        };
        ab.contacts
            .keys()
            .map(|k| crate::state::canonicalize_did(k).unwrap_or_else(|_| k.clone()))
            .collect()
    };

    let (raw_persons, own_did, wallet_owner_name) = service
        .with_wallet(|wallet| {
            let own_did = wallet.get_user_id().to_string();
            let prof = wallet.profile.to_public_profile();
            let name = [prof.first_name.as_deref(), prof.last_name.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            let owner_name = if !name.is_empty() {
                name
            } else if let Some(ref org) = prof.organization {
                org.clone()
            } else if let Some(ref com) = prof.community {
                com.clone()
            } else {
                "Ich".to_string()
            };

            let vouchers: Vec<&human_money_core::Voucher> = wallet
                .voucher_store
                .vouchers
                .values()
                .map(|vi| &vi.voucher)
                .collect();

            let classify = |did: &str| -> humoco_market::TrustTier {
                use human_money_core::models::conflict::TrustStatus;
                if matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_)) {
                    return humoco_market::TrustTier::Hidden;
                }

                if let Some(ref r) = reg {
                    let can_did =
                        crate::state::canonicalize_did(did).unwrap_or_else(|_| did.to_string());
                    if r.own_contacts.get(&can_did).is_some_and(|c| c.level >= 1) {
                        return humoco_market::TrustTier::Trusted;
                    }
                    if let Ok(eval) = r.evaluate_voucher(&can_did, &[], false, Some(now)) {
                        if eval.score > 0.0 {
                            return humoco_market::TrustTier::Trusted;
                        }
                    }
                }

                humoco_market::TrustTier::Unknown
            };

            let persons =
                humoco_market::build_market_view(vouchers, &cards_vec, &own_did, &classify, now);
            (persons, own_did, owner_name)
        })
        .map_err(FrontendError::from)?;

    let canonical_own_did =
        crate::state::canonicalize_did(&own_did).unwrap_or_else(|_| own_did.clone());

    let raw_board = humoco_market::build_ad_board(&raw_persons);

    let mut board: Vec<FrontendBoardItem> = raw_board
        .into_iter()
        .filter(|bi| ui_prefs.market_show_unknown || bi.trust == humoco_market::TrustTier::Trusted)
        .map(|bi| FrontendBoardItem::from_board_item(bi, now))
        .collect();

    if let Some(ref stored) = stored_opt {
        if stored.published {
            for entry in &stored.ads {
                if entry.is_published_active(now) {
                    let days_left = entry.ad.expires_at.saturating_sub(now).div_ceil(86_400);
                    board.push(FrontendBoardItem {
                        owner_did: canonical_own_did.clone(),
                        recipient_did: own_did.clone(),
                        owner_name: wallet_owner_name.clone(),
                        trust: FrontendTrustTier::Trusted,
                        ad: entry.ad.clone(),
                        days_left,
                        is_own: true,
                    });
                }
            }
        }
    }

    // Sort combined board vector descending by renewed_at / published_at
    board.sort_by(|a, b| {
        let time_a = a.ad.renewed_at.unwrap_or(a.ad.published_at);
        let time_b = b.ad.renewed_at.unwrap_or(b.ad.published_at);
        time_b.cmp(&time_a).then_with(|| a.ad.id.cmp(&b.ad.id))
    });

    let mut persons: Vec<FrontendMarketPerson> = raw_persons
        .into_iter()
        .filter(|p| {
            if p.trust == humoco_market::TrustTier::Hidden {
                return false;
            }
            if ui_prefs.market_show_unknown || p.trust == humoco_market::TrustTier::Trusted {
                return true;
            }
            let can_did = crate::state::canonicalize_did(&p.did).unwrap_or_else(|_| p.did.clone());
            known_contact_dids.contains(&can_did)
        })
        .map(FrontendMarketPerson::from)
        .collect();

    for person in persons.iter_mut() {
        let can_did =
            crate::state::canonicalize_did(&person.did).unwrap_or_else(|_| person.did.clone());
        if let Some(read_url) = sync_state
            .sync_sources_by_did
            .get(&person.recipient_did)
            .or_else(|| sync_state.sync_sources_by_did.get(&person.did))
            .or_else(|| sync_state.sync_sources_by_did.get(&can_did))
        {
            let sync_src = crate::models::FrontendEntrySource::Sync {
                read_url: read_url.clone(),
            };
            if !person.sources.contains(&sync_src) {
                person.sources.push(sync_src);
            }
        }
    }

    Ok(FrontendMarketSnapshot {
        board,
        people: persons,
        stored_card_count: store.cards.len(),
        now,
    })
}

pub fn export_forward_card_impl(
    did: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendForwardCardExport, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let store: humoco_market::CardStore =
        crate::state::load_encrypted_json_or_default(&mut service, MARKET_CARD_STORE_KEY, pw)
            .map_err(FrontendError::from)?;

    let validated_did = human_money_core::ValidatedUserId::parse(&did)
        .map_err(|e| FrontendError::from(format!("Invalid DID: {}", e)))?;

    let stored = store
        .cards
        .get(validated_did.as_str())
        .ok_or_else(|| FrontendError::from("Card not found in store".to_string()))?;

    let env = humoco_market::forward_envelope(stored)
        .ok_or_else(|| FrontendError::from("Card reached its maximum hop count".to_string()))?;

    let envelope_json = serde_json::to_string(&env)
        .map_err(|e| FrontendError::from(format!("Failed to serialize CardEnvelope: {}", e)))?;
    let share_uri = format!(
        "humoco://card?env={}",
        human_money_core::crypto::encode_base64(envelope_json.as_bytes())
    );

    let qr_svg = if share_uri.len() <= humoco_market::MAX_QR_SHARE_BYTES {
        humoco_market::encode_qr_svg(share_uri.as_bytes()).ok()
    } else {
        None
    };

    Ok(FrontendForwardCardExport {
        did: stored.card.payload.recipient_id().to_string(),
        envelope_json,
        share_uri,
        qr_svg,
    })
}

pub(super) fn parse_card_envelope_payload(
    payload: &str,
) -> Result<humoco_market::CardEnvelope, FrontendError> {
    if payload.len() > 64 * 1024 {
        return Err(FrontendError::from(
            "Card envelope payload exceeds 64 KiB limit".to_string(),
        ));
    }
    const CARD_URI_PREFIX: &str = "humoco://card?";
    fn clean_wrappers(s: &str) -> &str {
        s.trim_matches(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '<' | '>' | '"' | '\'' | '(' | ')' | '[' | ']' | '“' | '”' | '„' | '’' | '‘'
                )
        })
    }
    let unwrapped = clean_wrappers(payload);
    // Unwrap accidental line breaks/whitespace inside pasted card share URIs
    let compact: String = unwrapped.split_whitespace().collect();
    let card_token = if compact
        .get(..CARD_URI_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(CARD_URI_PREFIX))
    {
        Some(compact.as_str())
    } else {
        unwrapped
            .split_whitespace()
            .map(clean_wrappers)
            .find(|tok| {
                tok.get(..CARD_URI_PREFIX.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(CARD_URI_PREFIX))
            })
    };
    let json_str = if let Some(uri_token) = card_token {
        let after_scheme = &uri_token[CARD_URI_PREFIX.len()..];
        let query_slice = after_scheme
            .split(['#', '"', '\'', '>', '<', '”', '’'])
            .next()
            .unwrap_or(after_scheme);
        let raw_b64 = query_slice
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(k, _)| k.eq_ignore_ascii_case("env"))
            .map(|(_, v)| {
                v.trim_end_matches([
                    '.', ',', ';', ':', '!', '?', ')', ']', '”', '’', '"', '\'', '>',
                ])
            })
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                FrontendError::from("Card share URI is missing env query parameter".to_string())
            })?;
        // Normalize both standard and URL-safe Base64 without padding before decoding.
        let norm_b64 = raw_b64
            .replace('+', "-")
            .replace('/', "_")
            .trim_end_matches('=')
            .to_string();
        let bytes = human_money_core::crypto::decode_base64(&norm_b64).map_err(|e| {
            FrontendError::from(format!("Failed to decode card share URI base64: {}", e))
        })?;
        String::from_utf8(bytes)
            .map_err(|e| FrontendError::from(format!("Invalid UTF-8 in card share URI: {}", e)))?
    } else {
        unwrapped.to_string()
    };

    let json_trimmed = json_str.trim();
    if !json_trimmed.starts_with('{') {
        return Err(FrontendError::from(
            "Payload is not a valid CardEnvelope JSON".to_string(),
        ));
    }

    serde_json::from_str(json_trimmed)
        .map_err(|e| FrontendError::from(format!("Failed to parse CardEnvelope: {}", e)))
}

pub(super) fn format_card_display_name(
    profile: &human_money_core::models::profile::PublicProfile,
) -> Option<String> {
    let full = [
        profile.first_name.as_deref().map(str::trim),
        profile.last_name.as_deref().map(str::trim),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    if !full.is_empty() {
        return Some(full);
    }
    profile
        .organization
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn resolve_card_recipient_impl(
    payload: String,
    state: &AppState,
) -> Result<FrontendResolvedCardRecipient, FrontendError> {
    state.authorize(crate::state::RequiredAccess::Unlocked, None)?;
    let env = parse_card_envelope_payload(&payload)?;
    let card = humoco_market::verify_sd_card(&env.card_wire)
        .map_err(|e| FrontendError::from(format!("Invalid contact card: {}", e)))?;
    let profile = humoco_market::extract_public_profile(&card);
    Ok(FrontendResolvedCardRecipient {
        recipient_id: card.payload.recipient_id().to_string(),
        did: card.payload.did,
        display_name: format_card_display_name(&profile),
    })
}

pub fn import_contact_card_impl(
    payload: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendImportCardResult, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let env = parse_card_envelope_payload(&payload)?;

    let ui_prefs = state
        .get_or_load_ui_preferences(&mut service, pw)
        .map_err(FrontendError::from)?;
    let max_stored_cards = ui_prefs.max_stored_cards.clamp(100, 10_000);
    let now = chrono::Utc::now().timestamp().max(0) as u64;

    let mut store: humoco_market::CardStore =
        crate::state::load_encrypted_json_or_default(&mut service, MARKET_CARD_STORE_KEY, pw)
            .map_err(FrontendError::from)?;

    let outcome = service
        .with_wallet(|wallet| {
            let is_offender = |did: &str| -> bool {
                use human_money_core::models::conflict::TrustStatus;
                matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_))
            };
            let own_coords = wallet
                .profile
                .coordinates
                .as_deref()
                .and_then(humoco_market::parse_coordinates_pair);
            humoco_market::accept_envelope_manual(
                &mut store,
                &env,
                now,
                max_stored_cards,
                own_coords,
                &is_offender,
            )
        })
        .map_err(FrontendError::from)?;

    match outcome {
        humoco_market::AcceptOutcome::Accepted { did, replaced } => {
            let mut updated_address_book: Option<crate::models::FrontendAddressBook> = None;

            if let Some(stored) = store.cards.get(&did) {
                let card_syncs =
                    crate::services::sync_endpoints::sanitize_untrusted_sync_descriptors(
                        &stored.card.payload.drops,
                        humoco_market::MAX_CARD_DROPS,
                    );

                let is_exact_own_profile = service
                    .with_wallet(|w| w.get_user_id().trim() == did.trim())
                    .unwrap_or(false);

                if !is_exact_own_profile {
                    let mut address_book: crate::models::FrontendAddressBook =
                        crate::state::load_encrypted_json_or_default(
                            &mut service,
                            CONTACTS_DATA_NAME,
                            pw,
                        )
                        .map_err(FrontendError::from)?;

                    if address_book.contacts.keys().any(|k| k.trim() == did.trim()) {
                        let card_profile = humoco_market::extract_public_profile(&stored.card);
                        if crate::state::apply_contact_update_from_card(
                            &mut address_book,
                            &did,
                            &card_syncs,
                            Some(&card_profile),
                        ) {
                            updated_address_book = Some(address_book);
                        }
                    }
                }
            }

            let mut batch = crate::state::EncryptedBatchCommit::new();
            batch.card_store(store);
            if let Some(ab) = updated_address_book {
                batch.contacts(ab);
            }
            batch
                .commit(state, &mut service, pw)
                .map_err(FrontendError::from)?;

            Ok(FrontendImportCardResult {
                status: "accepted".to_string(),
                did: Some(did),
                replaced,
            })
        }
        humoco_market::AcceptOutcome::IgnoredStale => Ok(FrontendImportCardResult {
            status: "ignored_stale".to_string(),
            did: None,
            replaced: false,
        }),
        humoco_market::AcceptOutcome::RejectedExpired => Ok(FrontendImportCardResult {
            status: "rejected_expired".to_string(),
            did: None,
            replaced: false,
        }),
        humoco_market::AcceptOutcome::RejectedHops => Ok(FrontendImportCardResult {
            status: "rejected_hops".to_string(),
            did: None,
            replaced: false,
        }),
        humoco_market::AcceptOutcome::RejectedOffender => Ok(FrontendImportCardResult {
            status: "rejected_offender".to_string(),
            did: None,
            replaced: false,
        }),
        humoco_market::AcceptOutcome::Invalid(msg) => Ok(FrontendImportCardResult {
            status: format!("invalid: {}", msg),
            did: None,
            replaced: false,
        }),
    }
}

#[cfg(test)]
pub fn delete_foreign_contact_card_impl(
    did: String,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let mut store = state
        .load_card_store_or_cached(&mut service, pw)
        .map_err(FrontendError::from)?;

    let validated_did = human_money_core::ValidatedUserId::parse(&did)
        .map_err(|e| FrontendError::from(format!("Invalid DID: {}", e)))?;

    let mut removed_tombstones: Vec<(String, u64)> = Vec::new();
    if let Some(sc) = store.cards.remove(validated_did.as_str()) {
        removed_tombstones.push((
            sc.card.payload.recipient_id().to_string(),
            sc.card.payload.v,
        ));
    }
    if removed_tombstones.is_empty() && validated_did.as_str() == validated_did.canonical_did() {
        let target_pk = validated_did.public_key_bytes();
        store.cards.retain(|_, sc| {
            let matches_pk =
                human_money_core::crypto::get_pubkey_from_user_id(&sc.card.payload.did)
                    .ok()
                    .map(|k| k.to_bytes())
                    == Some(target_pk);
            if matches_pk {
                removed_tombstones.push((
                    sc.card.payload.recipient_id().to_string(),
                    sc.card.payload.v,
                ));
            }
            !matches_pk
        });
    }

    if !removed_tombstones.is_empty() {
        for (card_key, version) in removed_tombstones {
            humoco_market::mark_card_deleted(&mut store, &card_key, version);
        }
        state
            .save_card_store(&mut service, &store, pw)
            .map_err(FrontendError::from)?;
    }

    Ok(())
}
