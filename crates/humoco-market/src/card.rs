use crate::model::{CardEnvelope, StoredCard, WireFilter};
pub use crate::sd_card::*;

pub fn next_card_version(last_v: Option<u64>, now_unix: u64) -> u64 {
    last_v.map_or(now_unix, |lv| lv.saturating_add(1).max(now_unix))
}

pub fn own_envelope(card_wire: String) -> CardEnvelope {
    CardEnvelope { card_wire, hops: 1 }
}

pub fn forward_envelope(stored: &StoredCard) -> Option<CardEnvelope> {
    if stored.hops == 0 {
        return None;
    }
    let next_hops = stored.hops.checked_add(1)?;
    if next_hops > stored.card.payload.max_hops {
        return None;
    }
    let wire = render_wire(&stored.card, next_hops, WireFilter::Full)?;
    Some(CardEnvelope {
        card_wire: wire,
        hops: next_hops,
    })
}
