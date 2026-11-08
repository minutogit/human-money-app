use std::collections::BTreeSet;

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, XChaCha20Poly1305, XNonce};
use ed25519_dalek::VerifyingKey;
use hkdf::Hkdf;
use human_money_core::crypto::ed25519_pub_to_x25519;
use rand::seq::SliceRandom;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::card::forward_envelope;
use crate::error::MarketError;
use crate::model::{CardEnvelope, CardStore, MAX_FORWARDED_CARDS, MAX_SLOT_RECIPIENTS};

const SLOT_MAGIC: &[u8; 8] = b"HMSLOT01";
const RECIPIENT_ENTRY_SIZE: usize = 92; // 32 (eph_pk) + 12 (nonce) + 48 (ct)
const MAX_RECIPIENT_OVERHEAD: usize = 8 + 2 + 256 * RECIPIENT_ENTRY_SIZE + 24 + 16; // 23602 bytes

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CardBundle {
    pub envelopes: Vec<CardEnvelope>,
}

pub fn build_card_bundle(
    own_card_jws: Option<&str>,
    store: &CardStore,
    trust_level_of: &dyn Fn(&str) -> u8,
    max_bytes: usize,
) -> CardBundle {
    let mut envelopes = Vec::new();

    if let Some(jws) = own_card_jws {
        envelopes.push(CardEnvelope {
            card_jws: jws.to_string(),
            hops: 1,
        });
    }

    // Collect forwardable stored cards
    let mut forwardable: Vec<(&crate::model::StoredCard, CardEnvelope)> = Vec::new();
    for stored in store.cards.values() {
        if let Some(env) = forward_envelope(stored) {
            forwardable.push((stored, env));
        }
    }

    // Sort by trust_level desc, then v desc, then did asc
    forwardable.sort_by(|(s1, _), (s2, _)| {
        let t1 = trust_level_of(&s1.card.did);
        let t2 = trust_level_of(&s2.card.did);
        t2.cmp(&t1)
            .then_with(|| s2.card.v.cmp(&s1.card.v))
            .then_with(|| s1.card.did.cmp(&s2.card.did))
    });

    for (_, env) in forwardable.into_iter().take(MAX_FORWARDED_CARDS) {
        envelopes.push(env);
    }

    // Trim so resulting bundle fits within max_bytes accounting for max recipient overhead
    let payload_budget = max_bytes.saturating_sub(MAX_RECIPIENT_OVERHEAD);

    let mut bundle = CardBundle { envelopes };
    while bundle.envelopes.len() > 1 {
        let serialized_len = serde_json::to_vec(&bundle).map(|v| v.len()).unwrap_or(0);
        if serialized_len <= payload_budget {
            break;
        }
        bundle.envelopes.pop();
    }

    bundle
}

fn target_recipient_bucket(count: usize) -> Result<usize, MarketError> {
    if count <= 16 {
        Ok(16)
    } else if count <= 32 {
        Ok(32)
    } else if count <= 64 {
        Ok(64)
    } else if count <= 128 {
        Ok(128)
    } else if count <= 256 {
        Ok(256)
    } else {
        Err(MarketError::Validation(format!(
            "Too many recipients: {} > {}",
            count, MAX_SLOT_RECIPIENTS
        )))
    }
}

pub fn seal_slot_blob(
    bundle: &CardBundle,
    readers: &[VerifyingKey],
    max_bytes: usize,
) -> Result<Vec<u8>, MarketError> {
    // 1. Deduplicate readers by pubkey bytes
    let mut seen_keys = BTreeSet::new();
    let mut unique_readers = Vec::new();
    for r in readers {
        if seen_keys.insert(*r.as_bytes()) {
            unique_readers.push(*r);
        }
    }

    let bucket_size = target_recipient_bucket(unique_readers.len())?;

    // 2. Random 32-byte content key k
    let mut k = [0u8; 32];
    OsRng.fill_bytes(&mut k);

    // 3. Encrypt payload with XChaCha20Poly1305
    let payload_bytes = serde_json::to_vec(bundle)
        .map_err(|e| MarketError::Serialization(e.to_string()))?;

    let mut payload_nonce_bytes = [0u8; 24];
    OsRng.fill_bytes(&mut payload_nonce_bytes);
    let payload_nonce = XNonce::from_slice(&payload_nonce_bytes);

    let payload_cipher = XChaCha20Poly1305::new_from_slice(&k)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    let payload_ciphertext = payload_cipher
        .encrypt(payload_nonce, payload_bytes.as_ref())
        .map_err(|e| MarketError::Crypto(e.to_string()))?;

    // 4. Encapsulate content key for each reader
    let mut recipient_entries: Vec<Vec<u8>> = Vec::with_capacity(bucket_size);

    for reader in &unique_readers {
        let reader_x_pub = ed25519_pub_to_x25519(reader);

        let eph_secret = StaticSecret::random_from_rng(OsRng);
        let eph_pub = X25519PublicKey::from(&eph_secret);

        let shared = eph_secret.diffie_hellman(&reader_x_pub);
        let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());
        let mut kek = [0u8; 32];
        hkdf.expand(b"humoco-slot-recipient-kek", &mut kek)
            .expect("32 bytes is valid length for HKDF");

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let kek_cipher = ChaCha20Poly1305::new_from_slice(&kek)
            .map_err(|e| MarketError::Crypto(e.to_string()))?;
        let enc_k = kek_cipher
            .encrypt(nonce, k.as_ref())
            .map_err(|e| MarketError::Crypto(e.to_string()))?;

        let mut entry = Vec::with_capacity(RECIPIENT_ENTRY_SIZE);
        entry.extend_from_slice(eph_pub.as_bytes());
        entry.extend_from_slice(&nonce_bytes);
        entry.extend_from_slice(&enc_k);
        recipient_entries.push(entry);
    }

    // 5. Add indistinguishable dummy entries up to bucket_size
    while recipient_entries.len() < bucket_size {
        let dummy_secret = StaticSecret::random_from_rng(OsRng);
        let dummy_pub = X25519PublicKey::from(&dummy_secret);

        let mut dummy_nonce = [0u8; 12];
        OsRng.fill_bytes(&mut dummy_nonce);

        let mut dummy_ct = [0u8; 48];
        OsRng.fill_bytes(&mut dummy_ct);

        let mut entry = Vec::with_capacity(RECIPIENT_ENTRY_SIZE);
        entry.extend_from_slice(dummy_pub.as_bytes());
        entry.extend_from_slice(&dummy_nonce);
        entry.extend_from_slice(&dummy_ct);
        recipient_entries.push(entry);
    }

    // 6. Shuffle recipient entries using CSPRNG
    recipient_entries.shuffle(&mut OsRng);

    // 7. Assemble final blob
    let mut blob = Vec::with_capacity(
        8 + 2 + bucket_size * RECIPIENT_ENTRY_SIZE + 24 + payload_ciphertext.len(),
    );
    blob.extend_from_slice(SLOT_MAGIC);
    blob.extend_from_slice(&(bucket_size as u16).to_be_bytes());

    for entry in recipient_entries {
        blob.extend_from_slice(&entry);
    }

    blob.extend_from_slice(&payload_nonce_bytes);
    blob.extend_from_slice(&payload_ciphertext);

    if blob.len() > max_bytes {
        return Err(MarketError::Validation(format!(
            "Sealed SlotBlob length {} exceeds max_bytes {}",
            blob.len(),
            max_bytes
        )));
    }

    Ok(blob)
}

pub fn open_slot_blob(
    blob: &[u8],
    my_x25519_secret: &StaticSecret,
) -> Result<Option<CardBundle>, MarketError> {
    if blob.len() < 10 {
        return Err(MarketError::Slot("SlotBlob too short".to_string()));
    }

    if &blob[0..8] != SLOT_MAGIC {
        return Err(MarketError::Slot("Invalid SlotBlob magic header".to_string()));
    }

    let recipient_count = u16::from_be_bytes([blob[8], blob[9]]) as usize;
    if !matches!(recipient_count, 16 | 32 | 64 | 128 | 256) {
        return Err(MarketError::Slot(format!(
            "Invalid SlotBlob recipient bucket count: {}",
            recipient_count
        )));
    }

    let recipients_end = 10 + recipient_count * RECIPIENT_ENTRY_SIZE;
    let min_len = recipients_end + 24 + 16;
    if blob.len() < min_len {
        return Err(MarketError::Slot(format!(
            "SlotBlob too short for {} recipients: {} < {}",
            recipient_count,
            blob.len(),
            min_len
        )));
    }

    let payload_nonce_bytes = &blob[recipients_end..recipients_end + 24];
    let payload_ciphertext = &blob[recipients_end + 24..];
    let payload_nonce = XNonce::from_slice(payload_nonce_bytes);

    let entries_bytes = &blob[10..recipients_end];

    // Trial decryption over all recipient entries
    for chunk in entries_bytes.chunks_exact(RECIPIENT_ENTRY_SIZE) {
        let eph_pub_bytes: [u8; 32] = chunk[..32].try_into().unwrap();
        let nonce_bytes: [u8; 12] = chunk[32..44].try_into().unwrap();
        let enc_k = &chunk[44..92];

        let eph_pub = X25519PublicKey::from(eph_pub_bytes);
        let shared = my_x25519_secret.diffie_hellman(&eph_pub);
        if !shared.was_contributory() {
            continue;
        }

        let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());
        let mut kek = [0u8; 32];
        if hkdf.expand(b"humoco-slot-recipient-kek", &mut kek).is_err() {
            continue;
        }

        let kek_cipher = match ChaCha20Poly1305::new_from_slice(&kek) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let nonce = Nonce::from_slice(&nonce_bytes);

        if let Ok(decrypted_k) = kek_cipher.decrypt(nonce, enc_k) {
            if decrypted_k.len() == 32 {
                let payload_cipher = match XChaCha20Poly1305::new_from_slice(&decrypted_k) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if let Ok(bundle_bytes) = payload_cipher.decrypt(payload_nonce, payload_ciphertext) {
                    let bundle: CardBundle = serde_json::from_slice(&bundle_bytes)
                        .map_err(|e| MarketError::Serialization(e.to_string()))?;
                    return Ok(Some(bundle));
                }
            }
        }
    }

    Ok(None)
}
