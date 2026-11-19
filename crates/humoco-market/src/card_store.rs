use human_money_core::crypto::get_pubkey_from_user_id;

use crate::model::{CardEnvelope, CardStore, StoredCard, WireFilter, MAX_CARDS_PER_PUBKEY};
use crate::sd_card::{
    decoded_items, merge, strip_disclosures_beyond_distance, verify_sd_card, CardItem,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptOutcome {
    Accepted { did: String, replaced: bool },
    IgnoredStale,
    RejectedOffender,
    RejectedExpired,
    RejectedHops,
    Invalid(String),
}

/// Parses a "lat, lon" pair (comma, semicolon, or whitespace separated),
/// validating that latitude is in [-90.0, 90.0], longitude is in [-180.0, 180.0],
/// and both values are finite.
pub fn parse_coordinates_pair(s: &str) -> Option<(f64, f64)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() != 2 {
        return None;
    }
    let lat = parts[0].parse::<f64>().ok()?;
    let lon = parts[1].parse::<f64>().ok()?;
    if lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
    {
        Some((lat, lon))
    } else {
        None
    }
}

/// Computes the great-circle distance between two points on Earth using the Haversine formula (km).
/// Uses Earth mean radius of 6371.0 km.
pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = a;
    let (lat2, lon2) = b;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let sin_dlat_2 = (d_lat / 2.0).sin();
    let sin_dlon_2 = (d_lon / 2.0).sin();

    let h = sin_dlat_2 * sin_dlat_2
        + lat1.to_radians().cos() * lat2.to_radians().cos() * sin_dlon_2 * sin_dlon_2;
    let c = 2.0 * h.clamp(0.0, 1.0).sqrt().asin();
    6371.0 * c
}

pub fn derive_effective_hops(env_hops: u8, card_did: &str, slot_writer_did: Option<&str>) -> u8 {
    match slot_writer_did {
        Some(writer) if writer == card_did => 1,
        Some(_) => env_hops.max(2),
        None => env_hops,
    }
}

pub fn accept_envelope(
    store: &mut CardStore,
    env: &CardEnvelope,
    now: u64,
    max_stored_cards: usize,
    own_coords: Option<(f64, f64)>,
    is_offender: &dyn Fn(&str) -> bool,
) -> AcceptOutcome {
    accept_envelope_with_writer(
        store,
        env,
        None,
        now,
        max_stored_cards,
        own_coords,
        is_offender,
    )
}

pub fn accept_envelope_with_writer(
    store: &mut CardStore,
    env: &CardEnvelope,
    slot_writer_did: Option<&str>,
    now: u64,
    max_stored_cards: usize,
    own_coords: Option<(f64, f64)>,
    is_offender: &dyn Fn(&str) -> bool,
) -> AcceptOutcome {
    if env.hops == 0 {
        return AcceptOutcome::RejectedHops;
    }

    let mut card = match verify_sd_card(&env.card_wire) {
        Ok(c) => c,
        Err(e) => return AcceptOutcome::Invalid(e.to_string()),
    };

    let effective_hops = derive_effective_hops(env.hops, &card.payload.did, slot_writer_did);

    let pk = match get_pubkey_from_user_id(&card.payload.did) {
        Ok(pk) => pk,
        Err(e) => return AcceptOutcome::Invalid(format!("Invalid DID pubkey: {}", e)),
    };

    if now >= card.payload.valid_until {
        return AcceptOutcome::RejectedExpired;
    }

    if effective_hops > card.payload.max_hops {
        return AcceptOutcome::RejectedHops;
    }

    if is_offender(&card.payload.did) {
        return AcceptOutcome::RejectedOffender;
    }

    // Receiver rule (honest app): strip disclosures beyond effective_hops
    strip_disclosures_beyond_distance(&mut card, effective_hops);

    // Purge expired cards before enforcing per-pubkey and global capacity limits
    store.cards.retain(|_, c| c.card.payload.valid_until > now);

    let pk_bytes = *pk.as_bytes();
    let card_key = card.payload.recipient_id().to_string();
    let existing = store.cards.get_mut(&card_key);

    let replaced = match existing {
        None => {
            let card_wire = crate::sd_card::render_wire(&card, effective_hops, WireFilter::Full)
                .unwrap_or_else(|| env.card_wire.clone());
            store.cards.insert(
                card_key.clone(),
                StoredCard {
                    card: card.clone(),
                    card_wire,
                    hops: effective_hops,
                    received_at: now,
                },
            );
            false
        }
        Some(ext) => {
            if card.payload.v > ext.card.payload.v {
                let card_wire =
                    crate::sd_card::render_wire(&card, effective_hops, WireFilter::Full)
                        .unwrap_or_else(|| env.card_wire.clone());
                *ext = StoredCard {
                    card: card.clone(),
                    card_wire,
                    hops: effective_hops,
                    received_at: now,
                };
                true
            } else if card.payload.v == ext.card.payload.v && card.jws == ext.card.jws {
                merge(&mut ext.card, card.clone());
                ext.hops = ext.hops.min(effective_hops);
                ext.card_wire = crate::sd_card::render_wire(&ext.card, ext.hops, WireFilter::Full)
                    .unwrap_or_else(|| ext.card_wire.clone());
                false
            } else {
                // Either card.payload.v < ext.card.payload.v or
                // (card.payload.v == ext.card.payload.v && card.jws != ext.card.jws)
                return AcceptOutcome::IgnoredStale;
            }
        }
    };

    // Enforce max MAX_CARDS_PER_PUBKEY cards per root public key:
    // Evict tombstones before active cards, then larger hops, then older versions.
    let same_pubkey = |c: &StoredCard| -> bool {
        get_pubkey_from_user_id(&c.card.payload.did)
            .ok()
            .map(|k| *k.as_bytes())
            == Some(pk_bytes)
    };
    let mut sibling_count = store.cards.values().filter(|c| same_pubkey(c)).count();
    while sibling_count > MAX_CARDS_PER_PUBKEY {
        let victim_key = store
            .cards
            .iter()
            .filter(|(_, c)| same_pubkey(c))
            .max_by(|(k1, c1), (k2, c2)| {
                let t1 = c1.card.disclosures.is_empty();
                let t2 = c2.card.disclosures.is_empty();
                t1.cmp(&t2)
                    .then_with(|| c1.hops.cmp(&c2.hops))
                    .then_with(|| c2.card.payload.v.cmp(&c1.card.payload.v))
                    .then_with(|| c2.received_at.cmp(&c1.received_at))
                    .then_with(|| k1.cmp(k2))
            })
            .map(|(k, _)| k.clone());

        if let Some(vk) = victim_key {
            store.cards.remove(&vk);
            sibling_count -= 1;
            if vk == card_key {
                return AcceptOutcome::IgnoredStale;
            }
        } else {
            break;
        }
    }

    // Enforce max_stored_cards limit:
    // Evict card maximizing:
    // 1. !is_protected (unprotected true > protected false, so unprotected evicted before protected)
    // 2. c.hops (larger hops evicted first: hop 3 before hop 2)
    // 3. dist_bucket_10km (larger distance bucket evicted first)
    // 4. oldest received_at (c2.received_at.cmp(&c1.received_at))
    // 5. tie-break k1.cmp(k2)
    let card_meta = |c: &StoredCard| -> (bool, u64) {
        let items = decoded_items(&c.card);
        let is_protected = c.hops == 1
            || items.iter().any(|(item, _)| match item {
                CardItem::Skill(s) => s.is_remote,
                CardItem::Ad(a) => a.is_remote && a.expires_at > now,
                _ => false,
            });
        let dist_bucket_10km = if let Some(my_pos) = own_coords {
            let coords_str = items.iter().find_map(|(item, _)| match item {
                CardItem::Coordinates(s) => Some(s.as_str()),
                _ => None,
            });
            if let Some(their_pos) = coords_str.and_then(parse_coordinates_pair) {
                (haversine_km(my_pos, their_pos) / 10.0).round() as u64
            } else {
                25 // 250 km neutral fallback
            }
        } else {
            0
        };
        (is_protected, dist_bucket_10km)
    };

    while store.cards.len() > max_stored_cards.max(1) {
        let victim_pk = store
            .cards
            .iter()
            .max_by(|(k1, c1), (k2, c2)| {
                let (p1, d1) = card_meta(c1);
                let (p2, d2) = card_meta(c2);
                (!p1)
                    .cmp(&(!p2))
                    .then_with(|| c1.hops.cmp(&c2.hops))
                    .then_with(|| d1.cmp(&d2))
                    .then_with(|| c2.received_at.cmp(&c1.received_at))
                    .then_with(|| k1.cmp(k2))
            })
            .map(|(k, _)| k.clone());

        if let Some(k) = victim_pk {
            store.cards.remove(&k);
        } else {
            break;
        }
    }

    AcceptOutcome::Accepted {
        did: card_key,
        replaced,
    }
}

pub fn should_attach_card(store: &CardStore, peer_key: &[u8; 32], own_v: u64) -> bool {
    store
        .sent_log
        .get(peer_key)
        .is_none_or(|&last_v| own_v > last_v)
}

pub fn mark_sent(store: &mut CardStore, peer_key: [u8; 32], v: u64) {
    store.sent_log.insert(peer_key, v);
}
