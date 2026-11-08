use std::collections::HashMap;

use chrono::DateTime;
use human_money_core::models::profile::PublicProfile;
use human_money_core::services::crypto::{create_user_id, get_pubkey_from_user_id};
use human_money_core::Voucher;

use crate::card::forward_envelope;
use crate::model::{
    EntrySource, ListingOrigin, ListingType, MarketListing, MarketPerson, StandardRef, StoredCard,
    TrustTier, MAX_LISTING_TEXT_CHARS, MAX_LISTING_TITLE_CHARS,
};

/// Truncates a UTF-8 string at a word boundary (whitespace) such that the resulting
/// string including the trailing '…' does not exceed `max_chars`.
/// If no whitespace exists within the budget, it hard-cuts at `max_chars - 1` and appends '…'.
fn truncate_at_word_boundary(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }

    let budget = max_chars.saturating_sub(1);
    let prefix: String = text.chars().take(budget).collect();

    if let Some(last_ws_idx) = prefix.rfind(|c: char| c.is_whitespace()) {
        let trimmed_word = prefix[..last_ws_idx].trim_end();
        if !trimmed_word.is_empty() {
            return format!("{}…", trimmed_word);
        }
    }

    format!("{}…", prefix)
}

/// Extracts a single listing from an optional profile text field.
fn extract_listing(raw_input: &str, listing_type: ListingType) -> Option<MarketListing> {
    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let char_count = trimmed.chars().count();
    let (title, text) = if char_count <= MAX_LISTING_TITLE_CHARS {
        (trimmed.to_string(), None)
    } else {
        let title = truncate_at_word_boundary(trimmed, MAX_LISTING_TITLE_CHARS);
        let text = if char_count <= MAX_LISTING_TEXT_CHARS {
            trimmed.to_string()
        } else {
            truncate_at_word_boundary(trimmed, MAX_LISTING_TEXT_CHARS)
        };
        (title, Some(text))
    };

    Some(MarketListing {
        listing_type,
        title,
        text,
        pricing: None,
        origin: ListingOrigin::Voucher,
        allow_inquiries: false,
    })
}

struct VoucherData {
    newest_profile: PublicProfile,
    newest_as_of: u64,
    standards: Vec<StandardRef>,
}

struct MergedEntry {
    canonical_did: String,
    voucher_data: Option<VoucherData>,
    card_data: Option<StoredCard>,
}

/// Builds the aggregated marketplace view from vouchers (Phase 1) and stored cards (Phase 2+).
pub fn build_market_view<'a>(
    vouchers: impl IntoIterator<Item = &'a Voucher>,
    cards: &[StoredCard],
    own_did: &str,
    classify: &dyn Fn(&str) -> TrustTier,
) -> Vec<MarketPerson> {
    let own_pk = get_pubkey_from_user_id(own_did.trim()).ok();

    let mut merged: HashMap<[u8; 32], MergedEntry> = HashMap::new();
    let mut key_order: Vec<[u8; 32]> = Vec::new();

    // 1. Process vouchers
    for voucher in vouchers {
        if voucher.non_redeemable_test_voucher {
            continue;
        }

        let creator_id = match voucher.creator_profile.id.as_deref() {
            Some(id) if !id.trim().is_empty() => id.trim(),
            _ => continue,
        };

        let creator_pk = match get_pubkey_from_user_id(creator_id) {
            Ok(pk) => pk,
            Err(_) => continue,
        };

        let pk_bytes = *creator_pk.as_bytes();

        if let Some(ref own) = own_pk {
            if own.as_bytes() == &pk_bytes {
                continue;
            }
        }

        let canonical_did = match create_user_id(&creator_pk, None) {
            Ok(did) => did,
            Err(_) => continue,
        };

        let as_of = DateTime::parse_from_rfc3339(&voucher.creation_date)
            .map(|dt| dt.timestamp().max(0) as u64)
            .unwrap_or(0);

        let standard = StandardRef {
            name: voucher.voucher_standard.name.clone(),
            uuid: voucher.voucher_standard.uuid.clone(),
        };

        if let Some(entry) = merged.get_mut(&pk_bytes) {
            if let Some(ref mut vd) = entry.voucher_data {
                if !vd.standards.iter().any(|s| s.uuid == standard.uuid) {
                    vd.standards.push(standard);
                }
                if as_of >= vd.newest_as_of {
                    vd.newest_as_of = as_of;
                    vd.newest_profile = voucher.creator_profile.clone();
                }
            } else {
                entry.voucher_data = Some(VoucherData {
                    newest_profile: voucher.creator_profile.clone(),
                    newest_as_of: as_of,
                    standards: vec![standard],
                });
            }
        } else {
            key_order.push(pk_bytes);
            merged.insert(
                pk_bytes,
                MergedEntry {
                    canonical_did,
                    voucher_data: Some(VoucherData {
                        newest_profile: voucher.creator_profile.clone(),
                        newest_as_of: as_of,
                        standards: vec![standard],
                    }),
                    card_data: None,
                },
            );
        }
    }

    // 2. Process stored cards
    for stored in cards {
        let card_pk = match get_pubkey_from_user_id(&stored.card.did) {
            Ok(pk) => pk,
            Err(_) => continue,
        };
        let pk_bytes = *card_pk.as_bytes();

        if let Some(ref own) = own_pk {
            if own.as_bytes() == &pk_bytes {
                continue;
            }
        }

        let canonical_did = match create_user_id(&card_pk, None) {
            Ok(did) => did,
            Err(_) => continue,
        };

        if let Some(entry) = merged.get_mut(&pk_bytes) {
            if let Some(ref existing_card) = entry.card_data {
                if stored.card.v > existing_card.card.v {
                    entry.card_data = Some(stored.clone());
                }
            } else {
                entry.card_data = Some(stored.clone());
            }
        } else {
            key_order.push(pk_bytes);
            merged.insert(
                pk_bytes,
                MergedEntry {
                    canonical_did,
                    voucher_data: None,
                    card_data: Some(stored.clone()),
                },
            );
        }
    }

    // 3. Aggregate into MarketPerson
    let mut persons = Vec::new();

    for pk in key_order {
        let entry = match merged.remove(&pk) {
            Some(e) => e,
            None => continue,
        };

        let trust = classify(&entry.canonical_did);
        if trust == TrustTier::Hidden {
            continue;
        }

        let mut listings = Vec::new();
        let mut sources = Vec::new();
        let mut standards = Vec::new();
        let mut can_forward = false;
        let mut card_drops = Vec::new();

        // Card listings appear first
        if let Some(ref stored) = entry.card_data {
            for l in &stored.card.listings {
                listings.push(MarketListing {
                    listing_type: l.listing_type,
                    title: l.title.clone(),
                    text: l.text.clone(),
                    pricing: l.pricing.clone(),
                    origin: ListingOrigin::Card,
                    allow_inquiries: l.allow_inquiries,
                });
            }
            sources.push(EntrySource::Card { hops: stored.hops });
            can_forward = forward_envelope(stored).is_some();
            card_drops = stored.card.drops.clone();
        }

        // Voucher listings appear after card listings
        if let Some(ref vd) = entry.voucher_data {
            if let Some(ref offer) = vd.newest_profile.service_offer {
                if let Some(l) = extract_listing(offer, ListingType::Offer) {
                    listings.push(l);
                }
            }
            if let Some(ref needs) = vd.newest_profile.needs {
                if let Some(l) = extract_listing(needs, ListingType::Request) {
                    listings.push(l);
                }
            }
            sources.push(EntrySource::Voucher);
            standards = vd.standards.clone();
        }

        // Profile selection: Card profile wins if card.v >= voucher_as_of or no voucher
        let (profile, as_of) = match (&entry.card_data, &entry.voucher_data) {
            (Some(c), Some(v)) => {
                let max_as_of = c.card.v.max(v.newest_as_of);
                let prof = if c.card.v >= v.newest_as_of {
                    c.card.profile.clone()
                } else {
                    v.newest_profile.clone()
                };
                (prof, max_as_of)
            }
            (Some(c), None) => (c.card.profile.clone(), c.card.v),
            (None, Some(v)) => (v.newest_profile.clone(), v.newest_as_of),
            (None, None) => continue,
        };

        persons.push(MarketPerson {
            did: entry.canonical_did,
            profile,
            listings,
            sources,
            as_of,
            standards,
            trust,
            can_forward,
            card_drops,
        });
    }

    // Sort: Trusted before Unknown, then as_of descending, then did ascending
    persons.sort_by(|a, b| {
        let tier_rank = |t: TrustTier| match t {
            TrustTier::Trusted => 0,
            TrustTier::Unknown => 1,
            TrustTier::Hidden => 2,
        };

        tier_rank(a.trust)
            .cmp(&tier_rank(b.trust))
            .then_with(|| b.as_of.cmp(&a.as_of))
            .then_with(|| a.did.cmp(&b.did))
    });

    persons
}
