use human_money_core::crypto::get_pubkey_from_user_id;

use crate::model::{CardEnvelope, CardStore, StoredCard, WireFilter, MAX_STORED_CARDS};
use crate::sd_card::{merge, strip_disclosures_beyond_distance, verify_sd_card};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptOutcome {
    Accepted { did: String, replaced: bool },
    IgnoredStale,
    RejectedOffender,
    RejectedExpired,
    RejectedHops,
    Invalid(String),
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
    max_store_hops: u8,
    is_offender: &dyn Fn(&str) -> bool,
) -> AcceptOutcome {
    accept_envelope_with_writer(store, env, None, now, max_store_hops, is_offender)
}

pub fn accept_envelope_with_writer(
    store: &mut CardStore,
    env: &CardEnvelope,
    slot_writer_did: Option<&str>,
    now: u64,
    max_store_hops: u8,
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

    if effective_hops > card.payload.max_hops || effective_hops > max_store_hops {
        return AcceptOutcome::RejectedHops;
    }

    if is_offender(&card.payload.did) {
        return AcceptOutcome::RejectedOffender;
    }

    // Receiver rule (honest app): strip disclosures beyond effective_hops
    strip_disclosures_beyond_distance(&mut card, effective_hops);

    let pk_bytes = *pk.as_bytes();
    let existing = store.cards.get_mut(&pk_bytes);

    let replaced = match existing {
        None => {
            let card_wire = crate::sd_card::render_wire(&card, effective_hops, WireFilter::Full)
                .unwrap_or_else(|| env.card_wire.clone());
            store.cards.insert(
                pk_bytes,
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
                ext.card_wire =
                    crate::sd_card::render_wire(&ext.card, ext.hops, WireFilter::Full)
                        .unwrap_or_else(|| ext.card_wire.clone());
                false
            } else {
                // Either card.payload.v < ext.card.payload.v or
                // (card.payload.v == ext.card.payload.v && card.jws != ext.card.jws)
                return AcceptOutcome::IgnoredStale;
            }
        }
    };

    // Purge expired cards
    store.cards.retain(|_, c| c.card.payload.valid_until > now);

    // Enforce MAX_STORED_CARDS limit:
    // Evict largest hops first, then oldest received_at (smallest), tie-break by pubkey bytes.
    while store.cards.len() > MAX_STORED_CARDS {
        let victim_pk = store
            .cards
            .iter()
            .max_by(|(k1, c1), (k2, c2)| {
                c1.hops
                    .cmp(&c2.hops)
                    .then_with(|| c2.received_at.cmp(&c1.received_at))
                    .then_with(|| k1.cmp(k2))
            })
            .map(|(k, _)| *k);

        if let Some(k) = victim_pk {
            store.cards.remove(&k);
        } else {
            break;
        }
    }

    AcceptOutcome::Accepted {
        did: card.payload.did,
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
