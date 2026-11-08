use tauri::State;

use crate::models::{
    FrontendError, FrontendForwardCardExport, FrontendImportCardResult, FrontendMarketPerson,
    FrontendOwnContactCard, FrontendSaveOwnCardRequest,
};
use crate::state::{AppState, StoredOwnCard, MARKET_CARD_STORE_KEY, MARKET_OWN_CARD_KEY};

pub fn get_market_view_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendMarketPerson>, FrontendError> {
    let mut service = state.service.lock().unwrap();

    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password.as_deref()))
        .unwrap_or_default();

    let reg = state.get_or_load_wot_register(&mut service, password.as_deref()).ok();

    let now_secs = chrono::Utc::now().timestamp().max(0) as u64;

    let mut store: humoco_market::CardStore = service
        .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let before_count = store.cards.len();
    store.cards.retain(|_, c| c.card.valid_until > now_secs);
    if store.cards.len() != before_count {
        if let Ok(bytes) = serde_json::to_vec(&store) {
            let _ = service.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password.as_deref());
        }
    }

    let cards_vec: Vec<humoco_market::StoredCard> = store.cards.values().cloned().collect();

    let sync_state: crate::state::StoredDropSync = service
        .load_encrypted_data(crate::state::MARKET_DROP_SYNC_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let mut persons = service
        .with_wallet(|wallet| {
            let own_did = wallet.get_user_id().to_string();
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
                    let can_did = crate::state::canonicalize_did(did).unwrap_or_else(|_| did.to_string());
                    if r.own_contacts.get(&can_did).is_some_and(|c| c.level >= 1) {
                        return humoco_market::TrustTier::Trusted;
                    }
                    if let Ok(eval) = r.evaluate_voucher(&can_did, &[], false, Some(now_secs)) {
                        if eval.score > 0.0 {
                            return humoco_market::TrustTier::Trusted;
                        }
                    }
                }

                humoco_market::TrustTier::Unknown
            };

            let raw_persons = humoco_market::build_market_view(vouchers, &cards_vec, &own_did, &classify);

            raw_persons
                .into_iter()
                .filter(|p| settings.market_show_unknown || p.trust == humoco_market::TrustTier::Trusted)
                .map(FrontendMarketPerson::from)
                .collect::<Vec<FrontendMarketPerson>>()
        })
        .map_err(FrontendError::from)?;

    for person in persons.iter_mut() {
        let can_did = crate::state::canonicalize_did(&person.did).unwrap_or_else(|_| person.did.clone());
        if let Some(read_url) = sync_state
            .drop_sources_by_did
            .get(&person.did)
            .or_else(|| sync_state.drop_sources_by_did.get(&can_did))
        {
            let drop_src = crate::models::FrontendEntrySource::Drop {
                read_url: read_url.clone(),
            };
            if !person.sources.contains(&drop_src) {
                person.sources.push(drop_src);
            }
        }
    }

    Ok(persons)
}

#[tauri::command]
pub fn get_market_view(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FrontendMarketPerson>, FrontendError> {
    get_market_view_impl(password, &state)
}

pub fn get_own_contact_card_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendOwnContactCard>, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let stored_opt: Option<StoredOwnCard> = service
        .load_encrypted_data(MARKET_OWN_CARD_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let stored = match stored_opt {
        Some(s) => s,
        None => return Ok(None),
    };

    let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
    if now_secs >= stored.card.valid_until {
        return Ok(None);
    }

    let env = humoco_market::own_envelope(stored.card_jws.clone());
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

    Ok(Some(FrontendOwnContactCard {
        card: stored.card.into(),
        card_jws: stored.card_jws,
        qr_svg,
        share_uri,
        envelope_json,
    }))
}

#[tauri::command]
pub fn get_own_contact_card(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<FrontendOwnContactCard>, FrontendError> {
    get_own_contact_card_impl(password, &state)
}

pub fn save_own_contact_card_impl(
    request: FrontendSaveOwnCardRequest,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendOwnContactCard, FrontendError> {
    let mut service = state.service.lock().unwrap();

    let existing_opt: Option<StoredOwnCard> = service
        .load_encrypted_data(MARKET_OWN_CARD_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let now_unix = chrono::Utc::now().timestamp().max(0) as u64;
    let v = humoco_market::next_card_version(existing_opt.as_ref().map(|c| c.card.v), now_unix);
    let validity_days = request.validity_days.unwrap_or(60) as u64;
    let valid_until = now_unix.saturating_add(validity_days.saturating_mul(86_400));

    let existing_drops = existing_opt
        .map(|c| c.card.drops)
        .unwrap_or_default();

    let card_jws = service
        .with_wallet_and_identity(|_wallet, identity| {
            let root_did = human_money_core::crypto::create_user_id(&identity.signing_key.verifying_key(), None)
                .map_err(|e| human_money_core::app_service::AppFacadeError::Generic(e.to_string()))?;

            let card = humoco_market::ContactCard {
                did: root_did,
                v,
                valid_until,
                max_hops: request.max_hops,
                profile: request.profile.into(),
                listings: request
                    .listings
                    .into_iter()
                    .map(humoco_market::Listing::from)
                    .collect(),
                drops: existing_drops,
            };

            humoco_market::sign_card(card, &identity.signing_key)
                .map_err(|e| human_money_core::app_service::AppFacadeError::Generic(e.to_string()))
        })
        .map_err(FrontendError::from)??;

    let verified_card = humoco_market::verify_card(&card_jws)
        .map_err(|e| FrontendError::from(format!("Failed to verify newly signed card: {}", e)))?;

    let stored_own = StoredOwnCard {
        card: verified_card.clone(),
        card_jws: card_jws.clone(),
    };

    let bytes = serde_json::to_vec(&stored_own)
        .map_err(|e| FrontendError::from(format!("Failed to serialize StoredOwnCard: {}", e)))?;

    service
        .save_encrypted_data(MARKET_OWN_CARD_KEY, &bytes, password.as_deref())
        .map_err(FrontendError::from)?;

    let env = humoco_market::own_envelope(card_jws.clone());
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

    Ok(FrontendOwnContactCard {
        card: verified_card.into(),
        card_jws,
        qr_svg,
        share_uri,
        envelope_json,
    })
}

#[tauri::command]
pub fn save_own_contact_card(
    request: FrontendSaveOwnCardRequest,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendOwnContactCard, FrontendError> {
    save_own_contact_card_impl(request, password, &state)
}

pub fn delete_own_contact_card_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();
    service
        .save_encrypted_data(MARKET_OWN_CARD_KEY, b"null", password.as_deref())
        .map_err(FrontendError::from)?;
    Ok(())
}

#[tauri::command]
pub fn delete_own_contact_card(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    delete_own_contact_card_impl(password, &state)
}

pub fn export_forward_card_impl(
    did: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendForwardCardExport, FrontendError> {
    let mut service = state.service.lock().unwrap();

    let store: humoco_market::CardStore = service
        .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let pk = human_money_core::crypto::get_pubkey_from_user_id(&did)
        .map_err(|e| FrontendError::from(format!("Invalid DID: {}", e)))?;

    let stored = store
        .cards
        .get(pk.as_bytes())
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
        did: stored.card.did.clone(),
        envelope_json,
        share_uri,
        qr_svg,
    })
}

#[tauri::command]
pub fn export_forward_card(
    did: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendForwardCardExport, FrontendError> {
    export_forward_card_impl(did, password, &state)
}

pub fn import_contact_card_impl(
    payload: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendImportCardResult, FrontendError> {
    let mut service = state.service.lock().unwrap();

    let json_str = if let Some((_, query_part)) = payload.trim().split_once("?env=") {
        let b64 = query_part.split('&').next().unwrap_or(query_part);
        let bytes = human_money_core::crypto::decode_base64(b64)
            .map_err(|e| FrontendError::from(format!("Failed to decode card share URI base64: {}", e)))?;
        String::from_utf8(bytes)
            .map_err(|e| FrontendError::from(format!("Invalid UTF-8 in card share URI: {}", e)))?
    } else {
        payload
    };

    let env: humoco_market::CardEnvelope = serde_json::from_str(&json_str)
        .map_err(|e| FrontendError::from(format!("Failed to parse CardEnvelope: {}", e)))?;

    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password.as_deref()))
        .unwrap_or_default();
    let max_store_hops = settings.max_store_hops.clamp(1, 3);
    let now = chrono::Utc::now().timestamp().max(0) as u64;

    let mut store: humoco_market::CardStore = service
        .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let outcome = service
        .with_wallet(|wallet| {
            let is_offender = |did: &str| -> bool {
                use human_money_core::models::conflict::TrustStatus;
                matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_))
            };
            humoco_market::accept_envelope(&mut store, &env, now, max_store_hops, &is_offender)
        })
        .map_err(FrontendError::from)?;

    match outcome {
        humoco_market::AcceptOutcome::Accepted { did, replaced } => {
            let bytes = serde_json::to_vec(&store)
                .map_err(|e| FrontendError::from(format!("Failed to serialize CardStore: {}", e)))?;
            service
                .save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password.as_deref())
                .map_err(FrontendError::from)?;
            if let Some(stored) = store.cards.values().find(|sc| sc.card.did == did) {
                let card_drops: Vec<crate::models::FrontendDropDescriptor> =
                    stored.card.drops.iter().cloned().map(Into::into).collect();
                state.update_contact_drops_from_card(
                    &mut service,
                    &did,
                    &card_drops,
                    password.as_deref(),
                );
            }
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

#[tauri::command]
pub fn import_contact_card(
    payload: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendImportCardResult, FrontendError> {
    import_contact_card_impl(payload, password, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use human_money_core::app_service::AppService;
    use crate::models::{
        FrontendCardListingInput, FrontendEntrySource, FrontendListingOrigin,
        FrontendListingType, FrontendMarketListing, FrontendStandardRef,
        FrontendTrustTier, FrontendUserProfile,
    };
    use humoco_market::{
        EntrySource, ListingOrigin, ListingType, MarketListing, MarketPerson, StandardRef, TrustTier,
    };

    const ALICE_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const BOB_PHRASE: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
    const CHARLIE_PHRASE: &str = "legal winner thank year wave sausage worth useful legal winner thank yellow";

    fn temp_test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("humoco_market_test_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_test_app(dir: &Path, profile_name: &str, phrase: &str, password: &str) -> AppService {
        let mut service = AppService::new(dir).unwrap();
        service.create_profile(
            profile_name,
            phrase,
            None,
            Some("pc"),
            password,
            human_money_core::MnemonicLanguage::English,
            "device-market-test".into(),
        ).unwrap();
        service
    }

    fn create_test_state(service: AppService) -> AppState {
        AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(None),
            regional_views: Mutex::new(None),
            wot: Mutex::new(None),
        }
    }

    #[test]
    fn test_market_dto_conversions_and_camelcase_serialization() {
        let domain_listing = MarketListing {
            listing_type: ListingType::Offer,
            title: "Web Design".to_string(),
            text: Some("Professional design".to_string()),
            pricing: Some("50 Minuto".to_string()),
            origin: ListingOrigin::Voucher,
            allow_inquiries: true,
        };
        let dto_listing: FrontendMarketListing = domain_listing.into();
        assert_eq!(dto_listing.listing_type, FrontendListingType::Offer);
        assert_eq!(dto_listing.origin, FrontendListingOrigin::Voucher);
        assert!(dto_listing.allow_inquiries);

        let domain_person = MarketPerson {
            did: "did:key:z12345".to_string(),
            profile: human_money_core::models::profile::PublicProfile {
                id: Some("did:key:z12345".to_string()),
                first_name: Some("Alice".to_string()),
                service_offer: Some("Web Design".to_string()),
                ..Default::default()
            },
            listings: vec![MarketListing {
                listing_type: ListingType::Offer,
                title: "Web Design".to_string(),
                text: None,
                pricing: None,
                origin: ListingOrigin::Voucher,
                allow_inquiries: false,
            }],
            sources: vec![
                EntrySource::Voucher,
                EntrySource::Card { hops: 2 },
                EntrySource::Drop { read_url: "https://drop.example.com".to_string() },
            ],
            as_of: 1769940000,
            standards: vec![StandardRef {
                name: "Minuto".to_string(),
                uuid: "std-1".to_string(),
            }],
            trust: TrustTier::Trusted,
            can_forward: true,
            card_drops: vec![],
        };

        let dto_person: FrontendMarketPerson = domain_person.into();
        assert_eq!(dto_person.did, "did:key:z12345");
        assert_eq!(dto_person.trust, FrontendTrustTier::Trusted);
        assert_eq!(dto_person.listings.len(), 1);
        assert_eq!(dto_person.sources.len(), 3);
        assert_eq!(dto_person.sources[0], FrontendEntrySource::Voucher);
        assert_eq!(dto_person.sources[1], FrontendEntrySource::Card { hops: 2 });
        assert_eq!(
            dto_person.sources[2],
            FrontendEntrySource::Drop {
                read_url: "https://drop.example.com".to_string(),
            }
        );
        assert_eq!(dto_person.standards[0], FrontendStandardRef {
            name: "Minuto".to_string(),
            uuid: "std-1".to_string(),
        });
        assert!(dto_person.can_forward);
        assert!(dto_person.card_drops.is_empty());

        let json = serde_json::to_string(&dto_person).unwrap();
        assert!(json.contains("\"did\":\"did:key:z12345\""));
        assert!(json.contains("\"asOf\":1769940000"));
        assert!(json.contains("\"trust\":\"trusted\""));
        assert!(json.contains("\"kind\":\"voucher\""));
        assert!(json.contains("\"kind\":\"card\""));
        assert!(json.contains("\"hops\":2"));
        assert!(json.contains("\"kind\":\"drop\""));
        assert!(json.contains("\"readUrl\":\"https://drop.example.com\""));
        assert!(json.contains("\"listingType\":\"offer\""));
        assert!(json.contains("\"origin\":\"voucher\""));
        assert!(json.contains("\"canForward\":true"));
        assert!(json.contains("\"cardDrops\":[]"));

        let deserialized: FrontendMarketPerson = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.did, dto_person.did);
        assert_eq!(deserialized.trust, dto_person.trust);
        assert_eq!(deserialized.listings, dto_person.listings);
        assert_eq!(deserialized.sources, dto_person.sources);
        assert_eq!(deserialized.can_forward, dto_person.can_forward);
        assert_eq!(deserialized.card_drops, dto_person.card_drops);
    }

    #[test]
    fn test_phase_2_acceptance_criteria_flow() {
        let base_dir = temp_test_dir("phase2_flow");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let charlie_service = setup_test_app(&base_dir.join("charlie"), "Charlie", CHARLIE_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);
        let charlie_state = create_test_state(charlie_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // 1. Alice creates card with max_hops = 2
        let save_req = FrontendSaveOwnCardRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                last_name: Some("Smith".to_string()),
                service_offer: Some("Carpentry".to_string()),
                ..Default::default()
            },
            listings: vec![FrontendCardListingInput {
                listing_type: FrontendListingType::Offer,
                title: "Handmade Tables".to_string(),
                text: Some("Solid oak".to_string()),
                pricing: Some("100 M".to_string()),
                allow_inquiries: true,
            }],
            max_hops: 2,
            validity_days: Some(30),
        };

        let alice_card = save_own_contact_card_impl(save_req, Some(password.to_string()), &alice_state).unwrap();
        assert_eq!(alice_card.card.max_hops, 2);
        assert!(alice_card.qr_svg.is_some());

        // Alice can read her own card
        let own = get_own_contact_card_impl(Some(password.to_string()), &alice_state).unwrap();
        assert!(own.is_some());

        // 2. Bob imports Alice's card via humoco:// share URI
        let import_res_bob = import_contact_card_impl(alice_card.share_uri.clone(), Some(password.to_string()), &bob_state).unwrap();
        assert_eq!(import_res_bob.status, "accepted");
        assert!(!import_res_bob.replaced);

        // Bob verifies card is in market view
        let bob_market = get_market_view_impl(Some(password.to_string()), &bob_state).unwrap();
        let alice_in_bob = bob_market.iter().find(|p| p.profile.first_name.as_deref() == Some("Alice")).expect("Alice in Bob market");
        assert!(alice_in_bob.can_forward);
        assert_eq!(alice_in_bob.sources[0], FrontendEntrySource::Card { hops: 1 });

        // 3. Bob forwards Alice's card to Charlie (hops = 2)
        let bob_export = export_forward_card_impl(alice_did.clone(), Some(password.to_string()), &bob_state).unwrap();
        let charlie_import = import_contact_card_impl(bob_export.envelope_json, Some(password.to_string()), &charlie_state).unwrap();
        assert_eq!(charlie_import.status, "accepted");

        // Charlie verifies card is in market view with hops = 2
        let charlie_market = get_market_view_impl(Some(password.to_string()), &charlie_state).unwrap();
        let alice_in_charlie = charlie_market.iter().find(|p| p.profile.first_name.as_deref() == Some("Alice")).expect("Alice in Charlie market");
        assert_eq!(alice_in_charlie.sources[0], FrontendEntrySource::Card { hops: 2 });
        // Since max_hops is 2, Charlie cannot forward further!
        assert!(!alice_in_charlie.can_forward);
        assert!(export_forward_card_impl(alice_did.clone(), Some(password.to_string()), &charlie_state).is_err());

        // 4. max_hops = 1 blocks forwarding from Bob
        let save_req_hops1 = FrontendSaveOwnCardRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            listings: vec![FrontendCardListingInput {
                listing_type: FrontendListingType::Offer,
                title: "Local only".to_string(),
                text: None,
                pricing: None,
                allow_inquiries: false,
            }],
            max_hops: 1,
            validity_days: Some(30),
        };
        let alice_card_hops1 = save_own_contact_card_impl(save_req_hops1, Some(password.to_string()), &alice_state).unwrap();
        // Bob imports updated v2 card with max_hops = 1
        let bob_import_v2 = import_contact_card_impl(alice_card_hops1.share_uri, Some(password.to_string()), &bob_state).unwrap();
        assert_eq!(bob_import_v2.status, "accepted");
        assert!(bob_import_v2.replaced);

        // Bob can no longer forward!
        assert!(export_forward_card_impl(alice_did.clone(), Some(password.to_string()), &bob_state).is_err());

        // 5. Charlie with max_store_hops = 1 rejects hops = 2
        {
            let mut s = charlie_state.service.lock().unwrap();
            let mut settings = charlie_state.load_settings(&mut s, Some(password)).unwrap();
            settings.max_store_hops = 1;
            let bytes = serde_json::to_vec(&settings).unwrap();
            s.save_encrypted_data(crate::settings::SETTINGS_KEY, &bytes, Some(password)).unwrap();
            *charlie_state.settings.lock().unwrap() = Some(settings);
        }

        // Bob attempts to forward an envelope with hops = 2 to Charlie
        let fwd_env_hops2 = humoco_market::CardEnvelope {
            card_jws: alice_card.card_jws.clone(),
            hops: 2,
        };
        let charlie_import_rejected = import_contact_card_impl(
            serde_json::to_string(&fwd_env_hops2).unwrap(),
            Some(password.to_string()),
            &charlie_state,
        ).unwrap();
        assert_eq!(charlie_import_rejected.status, "rejected_hops");

        // 6. Piggybacking import helper: corrupted contact_card_json does NOT fail
        {
            let mut s = charlie_state.service.lock().unwrap();
            charlie_state.try_import_piggybacked_contact_card(&mut s, Some("not-valid-json"), Some(password));
            charlie_state.try_import_piggybacked_contact_card(&mut s, Some(r#"{"card_jws":"invalid.jws.signature","hops":1}"#), Some(password));
        }

        // 7. Delete own card
        delete_own_contact_card_impl(Some(password.to_string()), &alice_state).unwrap();
        let own_after_del = get_own_contact_card_impl(Some(password.to_string()), &alice_state).unwrap();
        assert!(own_after_del.is_none());
    }
}
