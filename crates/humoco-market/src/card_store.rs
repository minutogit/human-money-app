use human_money_core::crypto::get_pubkey_from_user_id;

use crate::card::verify_card;
use crate::model::{CardEnvelope, CardStore, StoredCard, MAX_STORED_CARDS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptOutcome {
    Accepted { did: String, replaced: bool },
    IgnoredStale,
    RejectedOffender,
    RejectedExpired,
    RejectedHops,
    Invalid(String),
}

pub fn accept_envelope(
    store: &mut CardStore,
    env: &CardEnvelope,
    now: u64,
    max_store_hops: u8,
    is_offender: &dyn Fn(&str) -> bool,
) -> AcceptOutcome {
    if env.hops == 0 {
        return AcceptOutcome::RejectedHops;
    }

    let card = match verify_card(&env.card_jws) {
        Ok(c) => c,
        Err(e) => return AcceptOutcome::Invalid(e.to_string()),
    };

    let pk = match get_pubkey_from_user_id(&card.did) {
        Ok(pk) => pk,
        Err(e) => return AcceptOutcome::Invalid(format!("Invalid DID pubkey: {}", e)),
    };

    if now >= card.valid_until {
        return AcceptOutcome::RejectedExpired;
    }

    if env.hops > card.max_hops || env.hops > max_store_hops {
        return AcceptOutcome::RejectedHops;
    }

    if is_offender(&card.did) {
        return AcceptOutcome::RejectedOffender;
    }

    let pk_bytes = *pk.as_bytes();
    let existing = store.cards.get(&pk_bytes);
    let should_accept = match existing {
        None => true,
        Some(ext) => card.v > ext.card.v || (card.v == ext.card.v && env.hops < ext.hops),
    };

    if !should_accept {
        return AcceptOutcome::IgnoredStale;
    }

    let replaced = existing.is_some();

    store.cards.insert(
        pk_bytes,
        StoredCard {
            card: card.clone(),
            card_jws: env.card_jws.clone(),
            hops: env.hops,
            received_at: now,
        },
    );

    // Purge expired cards
    store.cards.retain(|_, c| c.card.valid_until > now);

    // Enforce MAX_STORED_CARDS limit:
    // Evict largest hops first, then oldest received_at (smallest) first, tie-break by pubkey bytes.
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
        did: card.did,
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
