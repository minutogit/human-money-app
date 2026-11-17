use std::collections::HashMap;

use chrono::DateTime;
use human_money_core::models::profile::PublicProfile;
use human_money_core::services::crypto::{create_user_id, get_pubkey_from_user_id};
use human_money_core::Voucher;
use serde::{Deserialize, Serialize};

use crate::card::forward_envelope;
use crate::model::{
    Ad, DropDescriptor, EntrySource, StandardRef, StoredCard, TrustTier, MAX_SKILL_TEXT_CHARS,
    MAX_SKILL_TITLE_CHARS,
};
use crate::sd_card::{active_ads, decoded_items, extract_public_profile, extract_skills, CardItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillOrigin {
    Card,
    Voucher,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketSkill {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<String>,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_remote: bool,
    pub origin: SkillOrigin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketPerson {
    pub did: String,
    pub profile: PublicProfile,
    pub skills: Vec<MarketSkill>,
    pub active_ads: Vec<Ad>,
    pub ads_pending: u8,
    pub allow_inquiries: bool,
    pub sources: Vec<EntrySource>,
    pub as_of: u64,
    pub standards: Vec<StandardRef>,
    pub trust: TrustTier,
    pub can_forward: bool,
    pub card_drops: Vec<DropDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardItem {
    pub owner_did: String,
    pub owner_name: String,
    pub trust: TrustTier,
    pub ad: Ad,
}

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

/// Extracts a single skill from a voucher's service_offer profile field.
fn extract_voucher_skill(raw_input: &str) -> Option<MarketSkill> {
    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let char_count = trimmed.chars().count();
    let (title, text) = if char_count <= MAX_SKILL_TITLE_CHARS {
        (trimmed.to_string(), None)
    } else {
        let title = truncate_at_word_boundary(trimmed, MAX_SKILL_TITLE_CHARS);
        let text = if char_count <= MAX_SKILL_TEXT_CHARS {
            trimmed.to_string()
        } else {
            truncate_at_word_boundary(trimmed, MAX_SKILL_TEXT_CHARS)
        };
        (title, Some(text))
    };

    Some(MarketSkill {
        title,
        text,
        pricing: None,
        is_remote: false,
        origin: SkillOrigin::Voucher,
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

/// Builds the aggregated marketplace view from vouchers and stored SD-cards.
pub fn build_market_view<'a>(
    vouchers: impl IntoIterator<Item = &'a Voucher>,
    cards: &[StoredCard],
    own_did: &str,
    classify: &dyn Fn(&str) -> TrustTier,
    now: u64,
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
        let card_pk = match get_pubkey_from_user_id(&stored.card.payload.did) {
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
                if stored.card.payload.v > existing_card.card.payload.v {
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

        let mut skills = Vec::new();
        let mut sources = Vec::new();
        let mut standards = Vec::new();
        let mut can_forward = false;
        let mut card_drops = Vec::new();
        let mut ads = Vec::new();
        let mut ads_pending = 0;
        let mut allow_inquiries = false;

        // Card skills appear first
        if let Some(ref stored) = entry.card_data {
            for s in extract_skills(&stored.card) {
                skills.push(MarketSkill {
                    title: s.title,
                    text: s.text,
                    pricing: s.pricing,
                    is_remote: s.is_remote,
                    origin: SkillOrigin::Card,
                });
            }
            ads = active_ads(&stored.card, now);
            let disclosed_ads_count = decoded_items(&stored.card)
                .iter()
                .filter(|(item, _)| matches!(item, CardItem::Ad(_)))
                .count() as u8;
            ads_pending = stored
                .card
                .payload
                .ad_count
                .saturating_sub(disclosed_ads_count);
            allow_inquiries = stored.card.payload.allow_inquiries;
            sources.push(EntrySource::Card { hops: stored.hops });
            can_forward = forward_envelope(stored).is_some();
            card_drops = stored.card.payload.drops.clone();
        }

        // Voucher service_offer becomes a skill, with deduplication:
        // A card skill with the same title (case-insensitive, trimmed) supersedes a voucher skill.
        if let Some(ref vd) = entry.voucher_data {
            if let Some(ref offer) = vd.newest_profile.service_offer {
                if let Some(vs) = extract_voucher_skill(offer) {
                    let is_dup = skills
                        .iter()
                        .any(|s| s.title.trim().eq_ignore_ascii_case(vs.title.trim()));
                    if !is_dup {
                        skills.push(vs);
                    }
                }
            }
            sources.push(EntrySource::Voucher);
            standards = vd.standards.clone();
        }

        // Profile selection:
        // Card profile wins if card.payload.v >= voucher_as_of.
        // Maintainer decision 7: Preserve voucher `needs` on `profile.needs`.
        let (profile, as_of) = match (&entry.card_data, &entry.voucher_data) {
            (Some(c), Some(v)) => {
                let max_as_of = c.card.payload.v.max(v.newest_as_of);
                let mut prof = if c.card.payload.v >= v.newest_as_of {
                    extract_public_profile(&c.card)
                } else {
                    v.newest_profile.clone()
                };
                if prof.needs.is_none() {
                    prof.needs = v.newest_profile.needs.clone();
                }
                (prof, max_as_of)
            }
            (Some(c), None) => (extract_public_profile(&c.card), c.card.payload.v),
            (None, Some(v)) => (v.newest_profile.clone(), v.newest_as_of),
            (None, None) => continue,
        };

        persons.push(MarketPerson {
            did: entry.canonical_did,
            profile,
            skills,
            active_ads: ads,
            ads_pending,
            allow_inquiries,
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

fn format_person_name(profile: &PublicProfile, fallback_did: &str) -> String {
    let first = profile.first_name.as_deref().unwrap_or("").trim();
    let last = profile.last_name.as_deref().unwrap_or("").trim();
    let full = match (first.is_empty(), last.is_empty()) {
        (false, false) => format!("{} {}", first, last),
        (false, true) => first.to_string(),
        (true, false) => last.to_string(),
        (true, true) => String::new(),
    };
    if !full.is_empty() {
        return full;
    }
    if let Some(ref org) = profile.organization {
        let trimmed = org.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Some(ref com) = profile.community {
        let trimmed = com.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    fallback_did.to_string()
}

pub fn build_ad_board(people: &[MarketPerson]) -> Vec<BoardItem> {
    let mut board = Vec::new();

    for person in people {
        let owner_name = format_person_name(&person.profile, &person.did);
        for ad in &person.active_ads {
            board.push(BoardItem {
                owner_did: person.did.clone(),
                owner_name: owner_name.clone(),
                trust: person.trust,
                ad: ad.clone(),
            });
        }
    }

    // Sorted by ad.renewed_at.unwrap_or(ad.published_at) descending, tie-breaker: ad.id
    board.sort_by(|a, b| {
        let time_a = a.ad.renewed_at.unwrap_or(a.ad.published_at);
        let time_b = b.ad.renewed_at.unwrap_or(b.ad.published_at);
        time_b.cmp(&time_a).then_with(|| a.ad.id.cmp(&b.ad.id))
    });

    board
}
