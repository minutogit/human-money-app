use std::collections::BTreeSet;
use std::io::{Read, Write};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, XChaCha20Poly1305, XNonce};
use ed25519_dalek::VerifyingKey;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use hkdf::Hkdf;
use human_money_core::crypto::ed25519_pub_to_x25519;
use rand::seq::SliceRandom;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::card::forward_envelope;
use crate::card_store::{haversine_km, parse_coordinates_pair};
use crate::error::MarketError;
use crate::model::{CardEnvelope, StoredCard, MAX_SLOT_RECIPIENTS};
use crate::sd_card::{decoded_items, CardItem};

pub const SLOT_MAGIC: &[u8; 8] = b"HMSLOT02";
pub const SLOT_MULTI_MAGIC: &[u8; 8] = b"HMSLOTM2";
pub const BACKUP_SLOT_MAGIC: &[u8; 8] = b"HMBKP001";
pub const BACKUP_PAD_BLOCK_BYTES: usize = 65_536; // 64 KB
pub const DROP_BACKUP_MAX_BYTES: usize = 5 * 1024 * 1024; // 5 MB, §2
pub const MAX_DECOMPRESSED_SLOT_BYTES: usize = 8 * 1024 * 1024; // 8 MiB zip-bomb cap
const RECIPIENT_ENTRY_SIZE: usize = 92; // 32 (eph_pk) + 12 (nonce) + 48 (ct)
const MAX_RECIPIENT_OVERHEAD: usize = 8 + 2 + 256 * RECIPIENT_ENTRY_SIZE + 24 + 16; // 23602 bytes

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CardBundle {
    pub envelopes: Vec<CardEnvelope>,
}

/// Computes exact cryptographic envelope overhead for `recipient_count` padded into `{16, 32, 64, 128, 256}`.
pub fn slot_overhead_bytes(recipient_count: usize) -> Result<usize, MarketError> {
    let bucket = target_recipient_bucket(recipient_count)?;
    Ok(8 + 2 + bucket * RECIPIENT_ENTRY_SIZE + 24 + 16)
}

fn deflate_compress(data: &[u8]) -> Result<Vec<u8>, MarketError> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| MarketError::Serialization(format!("Deflate encode failed: {}", e)))?;
    encoder
        .finish()
        .map_err(|e| MarketError::Serialization(format!("Deflate finish failed: {}", e)))
}

fn deflate_decompress_bounded(compressed: &[u8]) -> Result<Vec<u8>, MarketError> {
    let decoder = DeflateDecoder::new(compressed);
    let mut limited = decoder.take((MAX_DECOMPRESSED_SLOT_BYTES + 1) as u64);
    let mut out = Vec::new();
    limited
        .read_to_end(&mut out)
        .map_err(|e| MarketError::Slot(format!("Invalid compressed slot payload: {}", e)))?;
    if out.len() > MAX_DECOMPRESSED_SLOT_BYTES {
        return Err(MarketError::PayloadTooLarge(format!(
            "Decompressed slot payload exceeds {} bytes safety limit",
            MAX_DECOMPRESSED_SLOT_BYTES
        )));
    }
    Ok(out)
}

fn card_relevance_rank(stored: &StoredCard, own_coords: Option<(f64, f64)>, now_secs: u64) -> u64 {
    let items = decoded_items(&stored.card);
    let has_remote = items.iter().any(|(item, _)| match item {
        CardItem::Skill(s) => s.is_remote,
        CardItem::Ad(a) => a.is_remote && a.expires_at > now_secs,
        _ => false,
    });
    match own_coords {
        Some(my_pos) => {
            let their_pos = items.iter().find_map(|(item, _)| match item {
                CardItem::Coordinates(s) => parse_coordinates_pair(s),
                _ => None,
            });
            match their_pos {
                Some(pos) => {
                    let km = haversine_km(my_pos, pos);
                    if km <= 50.0 {
                        0
                    } else if has_remote {
                        1
                    } else {
                        (km / 50.0).ceil() as u64 + 1
                    }
                }
                None => {
                    if has_remote {
                        0
                    } else {
                        5
                    }
                }
            }
        }
        None => u64::from(!has_remote),
    }
}

fn compressed_bundle_len(envelopes: &[CardEnvelope]) -> usize {
    #[derive(Serialize)]
    struct BundleRef<'a> {
        envelopes: &'a [CardEnvelope],
    }
    let json = serde_json::to_vec(&BundleRef { envelopes }).unwrap_or_default();
    deflate_compress(&json).map(|v| v.len()).unwrap_or(json.len())
}

/// Builds a prioritized, Deflate-budget-trimmed card bundle without hard card count limits.
pub fn build_card_bundle(
    own_card_wire: Option<&str>,
    cards: &[&StoredCard],
    own_coords: Option<(f64, f64)>,
    now_secs: u64,
    trust_level_of: &dyn Fn(&str) -> u8,
    recipient_count: usize,
    max_bytes: usize,
) -> CardBundle {
    let mut envelopes = Vec::new();

    if let Some(wire) = own_card_wire {
        envelopes.push(CardEnvelope {
            card_wire: wire.to_string(),
            hops: 1,
        });
    }

    let mut hop1: Vec<(&StoredCard, CardEnvelope, u64)> = Vec::new();
    let mut hop2_plus: Vec<(&StoredCard, CardEnvelope, u64)> = Vec::new();

    for &stored in cards {
        if stored.card.payload.valid_until <= now_secs {
            continue;
        }
        if let Some(env) = forward_envelope(stored) {
            let rank = card_relevance_rank(stored, own_coords, now_secs);
            if stored.hops <= 1 {
                hop1.push((stored, env, rank));
            } else {
                hop2_plus.push((stored, env, rank));
            }
        }
    }

    let cmp_candidates = |(s1, _, r1): &(&StoredCard, CardEnvelope, u64),
                          (s2, _, r2): &(&StoredCard, CardEnvelope, u64)| {
        let t1 = trust_level_of(&s1.card.payload.did);
        let t2 = trust_level_of(&s2.card.payload.did);
        s1.hops
            .cmp(&s2.hops)
            .then_with(|| t2.cmp(&t1))
            .then_with(|| r1.cmp(r2))
            .then_with(|| s2.card.payload.v.cmp(&s1.card.payload.v))
            .then_with(|| s1.card.payload.did.cmp(&s2.card.payload.did))
    };

    hop1.sort_by(cmp_candidates);
    hop2_plus.sort_by(cmp_candidates);

    // Fair 50/50 interleave between Hop-1 and Hop-2+ with full spillover once one pool exhausts
    let mut h1_iter = hop1.into_iter();
    let mut h2_iter = hop2_plus.into_iter();
    loop {
        match (h1_iter.next(), h2_iter.next()) {
            (None, None) => break,
            (Some((_, e1, _)), Some((_, e2, _))) => {
                envelopes.push(e1);
                envelopes.push(e2);
            }
            (Some((_, e1, _)), None) => envelopes.push(e1),
            (None, Some((_, e2, _))) => envelopes.push(e2),
        }
    }

    let overhead = slot_overhead_bytes(recipient_count).unwrap_or(MAX_RECIPIENT_OVERHEAD);
    let payload_budget = max_bytes.saturating_sub(overhead);

    if !envelopes.is_empty() && compressed_bundle_len(&envelopes) > payload_budget {
        let min_keep = usize::from(own_card_wire.is_some());
        let mut low = min_keep;
        let mut high = envelopes.len();
        while low < high {
            let mid = (low + high).div_ceil(2);
            if compressed_bundle_len(&envelopes[..mid]) <= payload_budget {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        envelopes.truncate(low);
    }

    CardBundle { envelopes }
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

pub fn seal_slot_payload(
    plaintext: &[u8],
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

    // 2. Deflate-compress plaintext before AEAD encryption
    let compressed_plaintext = deflate_compress(plaintext)?;

    // 3. Random 32-byte content key k
    let mut k = [0u8; 32];
    OsRng.fill_bytes(&mut k);

    // 4. Encrypt compressed payload with XChaCha20Poly1305
    let mut payload_nonce_bytes = [0u8; 24];
    OsRng.fill_bytes(&mut payload_nonce_bytes);
    let payload_nonce = XNonce::from_slice(&payload_nonce_bytes);

    let payload_cipher = XChaCha20Poly1305::new_from_slice(&k)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    let payload_ciphertext = payload_cipher
        .encrypt(payload_nonce, compressed_plaintext.as_slice())
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
        return Err(MarketError::PayloadTooLarge(format!(
            "Sealed SlotBlob length {} exceeds max_bytes {}",
            blob.len(),
            max_bytes
        )));
    }

    Ok(blob)
}

pub fn seal_slot_blob(
    bundle: &CardBundle,
    readers: &[VerifyingKey],
    max_bytes: usize,
) -> Result<Vec<u8>, MarketError> {
    let payload_bytes = serde_json::to_vec(bundle)
        .map_err(|e| MarketError::Serialization(e.to_string()))?;
    seal_slot_payload(&payload_bytes, readers, max_bytes)
}

pub fn open_slot_payload(
    blob: &[u8],
    my_x25519_secret: &StaticSecret,
) -> Result<Option<Vec<u8>>, MarketError> {
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
                if let Ok(compressed_bytes) = payload_cipher.decrypt(payload_nonce, payload_ciphertext) {
                    let decompressed = deflate_decompress_bounded(&compressed_bytes)?;
                    return Ok(Some(decompressed));
                }
            }
        }
    }

    Ok(None)
}

pub fn open_slot_blob(
    blob: &[u8],
    my_x25519_secret: &StaticSecret,
) -> Result<Option<CardBundle>, MarketError> {
    match open_slot_payload(blob, my_x25519_secret)? {
        Some(bytes) => {
            let bundle: CardBundle = serde_json::from_slice(&bytes)
                .map_err(|e| MarketError::Serialization(e.to_string()))?;
            Ok(Some(bundle))
        }
        None => Ok(None),
    }
}

/// Seals up to 3 distance-class payloads into a single multi-class `SlotBlob`.
///
/// Multi-class slot format:
/// `[b"HMSLOTM2" (8B) || count: u8 || for each class: (blob_len: u32_be || sub_blob_bytes)]`
///
/// Guarantees that a reader in Class 2 cannot decrypt the payload of Class 1.
pub fn seal_multi_class_slot_blob(
    classes: &[(&[u8], &[VerifyingKey])],
    max_total_bytes: usize,
) -> Result<Vec<u8>, MarketError> {
    if classes.is_empty() || classes.len() > 3 {
        return Err(MarketError::Validation(format!(
            "Classes count must be between 1 and 3, got {}",
            classes.len()
        )));
    }

    let mut sub_blobs = Vec::with_capacity(classes.len());
    for (idx, (plaintext, readers)) in classes.iter().enumerate() {
        if readers.is_empty() {
            return Err(MarketError::Validation(format!(
                "Class {} has empty reader list",
                idx + 1
            )));
        }
        let sub_blob = seal_slot_payload(plaintext, readers, max_total_bytes)?;
        sub_blobs.push(sub_blob);
    }

    let total_len: usize = 9 + sub_blobs.iter().map(|b| 4 + b.len()).sum::<usize>();
    if total_len > max_total_bytes {
        return Err(MarketError::PayloadTooLarge(format!(
            "Sealed multi-class slot blob length {} exceeds max_total_bytes {}",
            total_len, max_total_bytes
        )));
    }

    let mut encoded = Vec::with_capacity(total_len);
    encoded.extend_from_slice(SLOT_MULTI_MAGIC);
    encoded.push(classes.len() as u8);

    for sub_blob in &sub_blobs {
        let len_bytes = (sub_blob.len() as u32).to_be_bytes();
        encoded.extend_from_slice(&len_bytes);
        encoded.extend_from_slice(sub_blob);
    }

    Ok(encoded)
}

/// Opens a multi-class `SlotBlob` and returns the first subpackage decrypted by `my_x25519_secret`.
///
/// Returns `Ok(Some(plaintext))` on the first decrypted class payload, or `Ok(None)` if no class
/// payload was decryptable by this secret. Returns `Err(MarketError::Slot(...))` on invalid magic
/// or truncated/malformed buffers without panicking.
pub fn open_multi_class_slot_blob(
    blob: &[u8],
    my_x25519_secret: &StaticSecret,
) -> Result<Option<Vec<u8>>, MarketError> {
    if blob.len() < 9 {
        return Err(MarketError::Slot("Multi-class SlotBlob too short".to_string()));
    }

    if &blob[0..8] != SLOT_MULTI_MAGIC {
        return Err(MarketError::Slot("Invalid multi-class SlotBlob magic header".to_string()));
    }

    let count = blob[8] as usize;
    if !(1..=3).contains(&count) {
        return Err(MarketError::Slot(format!(
            "Invalid multi-class count: {}",
            count
        )));
    }

    let mut offset = 9;
    for _ in 0..count {
        if offset + 4 > blob.len() {
            return Err(MarketError::Slot(
                "Truncated class length in multi-class SlotBlob".to_string(),
            ));
        }
        let sub_len = u32::from_be_bytes([
            blob[offset],
            blob[offset + 1],
            blob[offset + 2],
            blob[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + sub_len > blob.len() {
            return Err(MarketError::Slot(
                "Truncated sub-blob in multi-class SlotBlob".to_string(),
            ));
        }

        let sub_blob = &blob[offset..offset + sub_len];
        offset += sub_len;

        if let Some(plaintext) = open_slot_payload(sub_blob, my_x25519_secret)? {
            return Ok(Some(plaintext));
        }
    }

    Ok(None)
}

/// Pads encrypted `.hmcbackup` bytes into a fixed-block multiple (64 KB) backup slot blob (§5).
///
/// Output format: `[b"HMBKP001" (8B) || (inner_len as u32_be) (4B) || inner_bytes || CSPRNG padding]`
/// rounded UP to an exact multiple of 65,536 bytes (minimum 65,536 bytes).
pub fn pad_backup_slot_bytes(
    hmcbackup_bytes: &[u8],
    max_bytes: usize,
) -> Result<Vec<u8>, MarketError> {
    if hmcbackup_bytes.is_empty() {
        return Err(MarketError::Validation(
            "Backup payload cannot be empty".to_string(),
        ));
    }
    let inner_len = hmcbackup_bytes.len();
    if inner_len > u32::MAX as usize {
        return Err(MarketError::PayloadTooLarge(format!(
            "Backup payload length {} exceeds 32-bit limit",
            inner_len
        )));
    }

    let unpadded_len = 12 + inner_len;
    let remainder = unpadded_len % BACKUP_PAD_BLOCK_BYTES;
    let padded_len = if remainder == 0 {
        unpadded_len
    } else {
        unpadded_len + (BACKUP_PAD_BLOCK_BYTES - remainder)
    };

    if padded_len > max_bytes {
        return Err(MarketError::PayloadTooLarge(format!(
            "Padded backup slot size {} exceeds max allowed {}",
            padded_len, max_bytes
        )));
    }

    let mut out = Vec::with_capacity(padded_len);
    out.extend_from_slice(BACKUP_SLOT_MAGIC);
    out.extend_from_slice(&(inner_len as u32).to_be_bytes());
    out.extend_from_slice(hmcbackup_bytes);

    let pad_len = padded_len - out.len();
    if pad_len > 0 {
        let mut pad = vec![0u8; pad_len];
        OsRng.fill_bytes(&mut pad);
        out.extend_from_slice(&pad);
    }

    Ok(out)
}

/// Unpads a 64-KB-multiple backup slot blob, returning the original `.hmcbackup` bytes.
pub fn unpad_backup_slot_bytes(slot_bytes: &[u8]) -> Result<Vec<u8>, MarketError> {
    if slot_bytes.len() < BACKUP_PAD_BLOCK_BYTES
        || !slot_bytes.len().is_multiple_of(BACKUP_PAD_BLOCK_BYTES)
    {
        return Err(MarketError::Slot(format!(
            "Invalid backup slot size: {} (must be a non-empty multiple of {} bytes)",
            slot_bytes.len(),
            BACKUP_PAD_BLOCK_BYTES
        )));
    }

    if &slot_bytes[0..8] != BACKUP_SLOT_MAGIC {
        return Err(MarketError::Slot(
            "Invalid backup slot magic header".to_string(),
        ));
    }

    let inner_len = u32::from_be_bytes([
        slot_bytes[8],
        slot_bytes[9],
        slot_bytes[10],
        slot_bytes[11],
    ]) as usize;

    if inner_len == 0 || 12 + inner_len > slot_bytes.len() {
        return Err(MarketError::Slot(format!(
            "Invalid inner backup payload length: {} in slot size {}",
            inner_len,
            slot_bytes.len()
        )));
    }

    Ok(slot_bytes[12..12 + inner_len].to_vec())
}

