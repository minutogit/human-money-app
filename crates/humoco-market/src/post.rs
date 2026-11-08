use std::collections::{BTreeMap, BTreeSet};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ed25519_dalek::{SigningKey, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use human_money_core::crypto::get_pubkey_from_user_id;
use human_money_core::services::crypto::dh::{ed25519_pub_to_x25519, ed25519_sk_to_x25519_sk};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::MarketError;
use crate::model::{DropDescriptor, MAX_LISTING_TITLE_CHARS};

type HmacSha256 = Hmac<Sha256>;

pub const POST_SLOT_MAGIC: &[u8; 8] = b"HMPOST01";
pub const DROP_POST_MAX_BYTES: usize = 524_288; // 512 KB (§2)
pub const POST_TTL_SECS: u64 = 21 * 86_400; // 21 days (§2)
pub const POST_BOX_TIERS: [usize; 4] = [4_096, 16_384, 65_536, 262_144]; // 4 KB / 16 KB / 64 KB / 256 KB (§6.1)

pub const PAYLOAD_POST_RECEIPT_V1: &str = "post_delivery_receipt_v1";
pub const PAYLOAD_MARKET_INQUIRY_V1: &str = "market_inquiry_v1";
pub const PAYLOAD_MARKET_INQUIRY_REPLY_V1: &str = "market_inquiry_reply_v1";

pub const MAX_INQUIRY_CHARS: usize = 280;
pub const DEFAULT_MAX_OPEN_INQUIRIES_PER_LISTING: usize = 20;

/// Derives a contributory X25519 shared secret between an Ed25519 signing key and a peer Ed25519 verifying key.
pub fn derive_post_shared_secret(
    my_ed_sk: &SigningKey,
    peer_ed_pk: &VerifyingKey,
) -> Result<[u8; 32], MarketError> {
    let my_x_sk = ed25519_sk_to_x25519_sk(my_ed_sk);
    let peer_x_pub = ed25519_pub_to_x25519(peer_ed_pk);
    let shared = my_x_sk.diffie_hellman(&peer_x_pub);
    if !shared.was_contributory() {
        return Err(MarketError::Crypto(
            "Non-contributory Diffie-Hellman exchange".to_string(),
        ));
    }
    let bytes = *shared.as_bytes();
    if bytes == [0u8; 32] {
        return Err(MarketError::Crypto("All-zero DH shared secret".to_string()));
    }
    Ok(bytes)
}

/// Computes the 16-byte recipient tag for fast, trial-decryption-free routing (§6.1).
pub fn compute_post_tag16(shared_secret: &[u8; 32], msg_nonce: &[u8; 24]) -> [u8; 16] {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(shared_secret).expect("HMAC can accept key of any size");
    mac.update(b"humoco-post-tag-v1");
    mac.update(msg_nonce);
    let result = mac.finalize().into_bytes();
    let mut tag = [0u8; 16];
    tag.copy_from_slice(&result[0..16]);
    tag
}

/// Derives the outer box encryption key from the shared secret and message nonce using HKDF-SHA256 (§6.1).
pub fn derive_post_box_key(
    shared_secret: &[u8; 32],
    msg_nonce: &[u8; 24],
) -> Result<[u8; 32], MarketError> {
    let hkdf = Hkdf::<Sha256>::new(None, shared_secret);
    let mut info = Vec::with_capacity(b"humoco-post-box-v1".len() + 24);
    info.extend_from_slice(b"humoco-post-box-v1");
    info.extend_from_slice(msg_nonce);
    let mut k_box = [0u8; 32];
    hkdf.expand(&info, &mut k_box)
        .map_err(|e| MarketError::Crypto(format!("HKDF expand failed: {}", e)))?;
    Ok(k_box)
}

/// Derives the master dummy key for self-recognizable dummy boxes (§6.1).
pub fn derive_dummy_key(master_seed: &[u8; 32]) -> [u8; 32] {
    blake3::derive_key("HUMOCO_POST_DUMMY_KEY_V1", master_seed)
}

/// Computes the 16-byte dummy tag for a dummy box entry (§6.1).
pub fn compute_dummy_tag16(dummy_key: &[u8; 32], msg_nonce: &[u8; 24]) -> [u8; 16] {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(dummy_key).expect("HMAC can accept key of any size");
    mac.update(b"humoco-post-dummy-v1");
    mac.update(msg_nonce);
    let result = mac.finalize().into_bytes();
    let mut tag = [0u8; 16];
    tag.copy_from_slice(&result[0..16]);
    tag
}

/// Creates a 4 KB dummy post box entry that is recognizable only by devices possessing `master_seed`.
pub fn create_dummy_post_box(master_seed: &[u8; 32]) -> PostBoxEntry {
    let dummy_key = derive_dummy_key(master_seed);
    let mut msg_nonce = [0u8; 24];
    OsRng.fill_bytes(&mut msg_nonce);
    let tag16 = compute_dummy_tag16(&dummy_key, &msg_nonce);
    let mut ciphertext = vec![0u8; 4_096];
    OsRng.fill_bytes(&mut ciphertext);
    PostBoxEntry {
        msg_nonce,
        tag16,
        ciphertext,
    }
}

/// Checks in constant time whether an entry is an internal dummy box belonging to the holder of `master_seed`.
pub fn is_own_dummy_post_box(master_seed: &[u8; 32], entry: &PostBoxEntry) -> bool {
    let dummy_key = derive_dummy_key(master_seed);
    let expected = compute_dummy_tag16(&dummy_key, &entry.msg_nonce);
    bool::from(expected.ct_eq(&entry.tag16))
}

/// Inner plaintext structure encapsulated within a `PostBoxEntry`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostEnvelope {
    pub msg_id: String,
    pub created_at: u64,
    pub reply_drops: Vec<DropDescriptor>,
    /// Raw serialized `SecureContainer` bytes (JSON/JWE).
    pub container_bytes: Vec<u8>,
}

impl PostEnvelope {
    pub fn new(
        msg_id: impl Into<String>,
        created_at: u64,
        reply_drops: Vec<DropDescriptor>,
        container_bytes: Vec<u8>,
    ) -> Self {
        Self {
            msg_id: msg_id.into(),
            created_at,
            reply_drops,
            container_bytes,
        }
    }
}

/// Wire entry inside a `post` channel slot (§6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostBoxEntry {
    pub msg_nonce: [u8; 24],
    pub tag16: [u8; 16],
    /// XChaCha20-Poly1305 ciphertext whose length is EXACTLY one of POST_BOX_TIERS (4096, 16384, 65536, 262144).
    pub ciphertext: Vec<u8>,
}

/// Seals a `PostEnvelope` into a padded `PostBoxEntry` whose ciphertext length strictly matches one of `POST_BOX_TIERS`.
pub fn seal_post_box(
    sender_ed_sk: &SigningKey,
    recipient_ed_pk: &VerifyingKey,
    envelope: &PostEnvelope,
) -> Result<PostBoxEntry, MarketError> {
    let shared_secret = derive_post_shared_secret(sender_ed_sk, recipient_ed_pk)?;
    let mut msg_nonce = [0u8; 24];
    OsRng.fill_bytes(&mut msg_nonce);

    let tag16 = compute_post_tag16(&shared_secret, &msg_nonce);
    let k_box = derive_post_box_key(&shared_secret, &msg_nonce)?;

    let envelope_json =
        serde_json::to_vec(envelope).map_err(|e| MarketError::Serialization(e.to_string()))?;

    let needed = 4 + envelope_json.len() + 16;
    let target_tier = POST_BOX_TIERS
        .iter()
        .copied()
        .find(|&t| needed <= t)
        .ok_or_else(|| {
            MarketError::PayloadTooLarge(format!(
                "Envelope size {} exceeds maximum post box tier {}",
                needed,
                POST_BOX_TIERS[POST_BOX_TIERS.len() - 1]
            ))
        })?;

    let plain_len = target_tier - 16;
    let mut padded_plaintext = Vec::with_capacity(plain_len);
    padded_plaintext.extend_from_slice(&(envelope_json.len() as u32).to_be_bytes());
    padded_plaintext.extend_from_slice(&envelope_json);

    let pad_len = plain_len - padded_plaintext.len();
    if pad_len > 0 {
        let mut pad = vec![0u8; pad_len];
        OsRng.fill_bytes(&mut pad);
        padded_plaintext.extend_from_slice(&pad);
    }

    let cipher = XChaCha20Poly1305::new_from_slice(&k_box)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    let nonce = XNonce::from_slice(&msg_nonce);

    let payload = Payload {
        msg: &padded_plaintext,
        aad: &tag16,
    };

    let ciphertext = cipher
        .encrypt(nonce, payload)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;

    debug_assert_eq!(ciphertext.len(), target_tier);

    Ok(PostBoxEntry {
        msg_nonce,
        tag16,
        ciphertext,
    })
}

/// Attempts to open a `PostBoxEntry` from a specific sender.
///
/// First verifies `tag16` in constant time without AEAD decryption. If the tag does not match,
/// returns `Ok(None)` immediately. If matching, decrypts, validates tier length and authenticates.
pub fn try_open_post_box_from_sender(
    recipient_ed_sk: &SigningKey,
    sender_ed_pk: &VerifyingKey,
    entry: &PostBoxEntry,
) -> Result<Option<PostEnvelope>, MarketError> {
    let shared_secret = derive_post_shared_secret(recipient_ed_sk, sender_ed_pk)?;
    let expected_tag16 = compute_post_tag16(&shared_secret, &entry.msg_nonce);

    if !bool::from(expected_tag16.ct_eq(&entry.tag16)) {
        return Ok(None);
    }

    if !POST_BOX_TIERS.contains(&entry.ciphertext.len()) {
        return Err(MarketError::Slot(format!(
            "Invalid post box ciphertext tier length: {}",
            entry.ciphertext.len()
        )));
    }

    let k_box = derive_post_box_key(&shared_secret, &entry.msg_nonce)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&k_box)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    let nonce = XNonce::from_slice(&entry.msg_nonce);

    let payload = Payload {
        msg: &entry.ciphertext,
        aad: &entry.tag16,
    };

    let decrypted = cipher
        .decrypt(nonce, payload)
        .map_err(|e| MarketError::Crypto(format!("AEAD authentication failed: {}", e)))?;

    if decrypted.len() < 4 {
        return Err(MarketError::Slot("Decrypted post box too short".to_string()));
    }

    let inner_len =
        u32::from_be_bytes([decrypted[0], decrypted[1], decrypted[2], decrypted[3]]) as usize;
    if 4 + inner_len > decrypted.len() {
        return Err(MarketError::Slot(format!(
            "Invalid inner envelope length {} in post box decrypted length {}",
            inner_len,
            decrypted.len()
        )));
    }

    let envelope: PostEnvelope = serde_json::from_slice(&decrypted[4..4 + inner_len])
        .map_err(|e| MarketError::Serialization(e.to_string()))?;

    Ok(Some(envelope))
}

/// Checks `entry` against all precomputed/authorized senders and returns `(sender_did, envelope)` on match.
pub fn try_open_post_box_any_sender(
    recipient_ed_sk: &SigningKey,
    authorized_senders: &[(String, VerifyingKey)],
    entry: &PostBoxEntry,
) -> Result<Option<(String, PostEnvelope)>, MarketError> {
    for (sender_did, sender_pk) in authorized_senders {
        if let Some(envelope) = try_open_post_box_from_sender(recipient_ed_sk, sender_pk, entry)? {
            return Ok(Some((sender_did.clone(), envelope)));
        }
    }
    Ok(None)
}

/// Encodes a list of real entries into a binary `post` slot blob (§6.1).
///
/// Strips any dummy boxes from `real_entries`, appends 1–2 fresh dummy boxes if total real count < 2
/// (so that small or empty slots do not reveal active message counts), sorts entries by `tag16`,
/// and encodes with the `HMPOST01` frame.
pub fn encode_post_slot(
    master_seed: &[u8; 32],
    real_entries: &[PostBoxEntry],
    max_bytes: usize,
) -> Result<Vec<u8>, MarketError> {
    let mut entries: Vec<PostBoxEntry> = real_entries
        .iter()
        .filter(|e| !is_own_dummy_post_box(master_seed, e))
        .cloned()
        .collect();

    let needed_dummies = 2usize.saturating_sub(entries.len());
    for _ in 0..needed_dummies {
        let dummy = create_dummy_post_box(master_seed);
        let current_size: usize =
            10 + entries.iter().map(|e| 44 + e.ciphertext.len()).sum::<usize>();
        if current_size + 44 + dummy.ciphertext.len() <= max_bytes {
            entries.push(dummy);
        } else {
            break;
        }
    }

    entries.sort_by(|a, b| a.tag16.cmp(&b.tag16));

    if entries.len() > u16::MAX as usize {
        return Err(MarketError::PayloadTooLarge(format!(
            "Entries count {} exceeds u16 limit",
            entries.len()
        )));
    }

    let total_len: usize = 10 + entries.iter().map(|e| 44 + e.ciphertext.len()).sum::<usize>();
    if total_len > max_bytes {
        return Err(MarketError::PayloadTooLarge(format!(
            "Encoded post slot size {} exceeds max allowed {}",
            total_len, max_bytes
        )));
    }

    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(POST_SLOT_MAGIC);
    out.extend_from_slice(&(entries.len() as u16).to_be_bytes());

    for entry in entries {
        out.extend_from_slice(&entry.msg_nonce);
        out.extend_from_slice(&entry.tag16);
        out.extend_from_slice(&(entry.ciphertext.len() as u32).to_be_bytes());
        out.extend_from_slice(&entry.ciphertext);
    }

    Ok(out)
}

/// Decodes and validates a binary `post` slot blob (§6.1).
///
/// Validates magic header, exact bounds, and tier sizes for each entry without panicking.
pub fn decode_post_slot(slot_bytes: &[u8]) -> Result<Vec<PostBoxEntry>, MarketError> {
    if slot_bytes.len() < 10 {
        return Err(MarketError::Slot(format!(
            "Post slot too short: {} bytes",
            slot_bytes.len()
        )));
    }

    if &slot_bytes[0..8] != POST_SLOT_MAGIC {
        return Err(MarketError::Slot(
            "Invalid post slot magic header".to_string(),
        ));
    }

    let count = u16::from_be_bytes([slot_bytes[8], slot_bytes[9]]) as usize;
    let mut entries = Vec::with_capacity(count);
    let mut offset = 10;

    for i in 0..count {
        if offset + 44 > slot_bytes.len() {
            return Err(MarketError::Slot(format!(
                "Truncated post box header at entry {}",
                i
            )));
        }

        let mut msg_nonce = [0u8; 24];
        msg_nonce.copy_from_slice(&slot_bytes[offset..offset + 24]);
        offset += 24;

        let mut tag16 = [0u8; 16];
        tag16.copy_from_slice(&slot_bytes[offset..offset + 16]);
        offset += 16;

        let ct_len = u32::from_be_bytes([
            slot_bytes[offset],
            slot_bytes[offset + 1],
            slot_bytes[offset + 2],
            slot_bytes[offset + 3],
        ]) as usize;
        offset += 4;

        if !POST_BOX_TIERS.contains(&ct_len) {
            return Err(MarketError::Slot(format!(
                "Invalid post box tier length {} at entry {}",
                ct_len, i
            )));
        }

        if offset + ct_len > slot_bytes.len() {
            return Err(MarketError::Slot(format!(
                "Truncated ciphertext for entry {}: expected {} bytes, have {}",
                i,
                ct_len,
                slot_bytes.len() - offset
            )));
        }

        let ciphertext = slot_bytes[offset..offset + ct_len].to_vec();
        offset += ct_len;

        entries.push(PostBoxEntry {
            msg_nonce,
            tag16,
            ciphertext,
        });
    }

    if offset != slot_bytes.len() {
        return Err(MarketError::Slot(format!(
            "Trailing unexpected bytes in post slot: read {} / total {}",
            offset,
            slot_bytes.len()
        )));
    }

    Ok(entries)
}

/// Merges a remote slot with local additions while removing acknowledged tags (§6.1).
///
/// Strips old dummy boxes and acknowledged entries, merges new entries (deduplicating by `tag16`),
/// and encodes the combined slot with fresh dummy boxes if needed.
pub fn merge_post_slot(
    master_seed: &[u8; 32],
    remote_slot_bytes: Option<&[u8]>,
    local_additions: &[PostBoxEntry],
    remove_tags: &[[u8; 16]],
    max_bytes: usize,
) -> Result<Vec<u8>, MarketError> {
    let mut retained_entries: Vec<PostBoxEntry> = Vec::new();

    if let Some(bytes) = remote_slot_bytes {
        if !bytes.is_empty() {
            let remote_entries = decode_post_slot(bytes)?;
            for entry in remote_entries {
                if is_own_dummy_post_box(master_seed, &entry) {
                    continue;
                }
                if remove_tags.iter().any(|t| t == &entry.tag16) {
                    continue;
                }
                retained_entries.push(entry);
            }
        }
    }

    for addition in local_additions {
        if is_own_dummy_post_box(master_seed, addition) {
            continue;
        }
        if remove_tags.iter().any(|t| t == &addition.tag16) {
            continue;
        }
        if !retained_entries.iter().any(|e| e.tag16 == addition.tag16) {
            retained_entries.push(addition.clone());
        }
    }

    encode_post_slot(master_seed, &retained_entries, max_bytes)
}

/// Delivery receipt payload (§6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostDeliveryReceipt {
    pub acknowledged_msg_id: String,
    pub acknowledged_tag16_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    pub received_at: u64,
}

pub fn validate_post_delivery_receipt(receipt: &PostDeliveryReceipt) -> Result<(), MarketError> {
    if receipt.acknowledged_msg_id.trim().is_empty() {
        return Err(MarketError::Validation(
            "acknowledged_msg_id cannot be empty".to_string(),
        ));
    }
    if receipt.acknowledged_tag16_hex.len() != 32
        || hex::decode(&receipt.acknowledged_tag16_hex).is_err()
    {
        return Err(MarketError::Validation(
            "acknowledged_tag16_hex must be a valid 32-hex character string".to_string(),
        ));
    }
    Ok(())
}

/// Asynchronous Web-of-Trust Handshake Assertion (§6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WotHandshakeAssertion {
    pub sender_did: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub proposed_level: u8, // 0 or 1
    pub wot_sync_json: String,
    pub reply_requested: bool,
    pub created_at: u64,
}

pub fn validate_wot_handshake_assertion(
    assertion: &WotHandshakeAssertion,
) -> Result<(), MarketError> {
    if assertion.sender_did.trim().is_empty() {
        return Err(MarketError::Validation(
            "sender_did cannot be empty".to_string(),
        ));
    }
    get_pubkey_from_user_id(&assertion.sender_did)
        .map_err(|e| MarketError::Identity(format!("Invalid sender_did: {}", e)))?;

    if assertion.proposed_level > 1 {
        return Err(MarketError::Validation(format!(
            "proposed_level must be 0 or 1, got {}",
            assertion.proposed_level
        )));
    }
    if assertion.wot_sync_json.trim().is_empty() {
        return Err(MarketError::Validation(
            "wot_sync_json cannot be empty".to_string(),
        ));
    }
    Ok(())
}

/// 1-Step Marketplace Inquiry payload (§6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketInquiry {
    pub inquiry_id: String,
    pub listing_id: String,
    pub listing_title: String,
    pub message: String, // strictly validated: 1..=280 chars!
    pub created_at: u64,
}

pub fn validate_market_inquiry(inquiry: &MarketInquiry) -> Result<(), MarketError> {
    if inquiry.inquiry_id.trim().is_empty() {
        return Err(MarketError::Validation(
            "inquiry_id cannot be empty".to_string(),
        ));
    }
    if inquiry.listing_id.trim().is_empty() {
        return Err(MarketError::Validation(
            "listing_id cannot be empty".to_string(),
        ));
    }
    let title_trimmed = inquiry.listing_title.trim();
    if title_trimmed.is_empty() {
        return Err(MarketError::Validation(
            "listing_title cannot be empty".to_string(),
        ));
    }
    if title_trimmed.chars().count() > MAX_LISTING_TITLE_CHARS {
        return Err(MarketError::Validation(format!(
            "listing_title length {} exceeds MAX_LISTING_TITLE_CHARS ({})",
            title_trimmed.chars().count(),
            MAX_LISTING_TITLE_CHARS
        )));
    }
    let msg_chars = inquiry.message.trim().chars().count();
    if msg_chars == 0 {
        return Err(MarketError::Validation(
            "message cannot be empty".to_string(),
        ));
    }
    if msg_chars > MAX_INQUIRY_CHARS {
        return Err(MarketError::Validation(format!(
            "message length {} exceeds MAX_INQUIRY_CHARS ({})",
            msg_chars, MAX_INQUIRY_CHARS
        )));
    }
    Ok(())
}

/// Seller decision on a 1-step marketplace inquiry (§6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketInquiryDecision {
    Confirm,
    Decline,
}

/// 1-Step Marketplace Inquiry Reply payload (§6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketInquiryReply {
    pub inquiry_id: String,
    pub listing_id: String,
    pub decision: MarketInquiryDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>, // optional <= 280 chars
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_uuid: Option<String>,
    pub replied_at: u64,
}

pub fn validate_market_inquiry_reply(reply: &MarketInquiryReply) -> Result<(), MarketError> {
    if reply.inquiry_id.trim().is_empty() {
        return Err(MarketError::Validation(
            "inquiry_id cannot be empty".to_string(),
        ));
    }
    if reply.listing_id.trim().is_empty() {
        return Err(MarketError::Validation(
            "listing_id cannot be empty".to_string(),
        ));
    }
    if let Some(ref note) = reply.note {
        let note_chars = note.trim().chars().count();
        if note_chars > MAX_INQUIRY_CHARS {
            return Err(MarketError::Validation(format!(
                "reply note length {} exceeds MAX_INQUIRY_CHARS ({})",
                note_chars, MAX_INQUIRY_CHARS
            )));
        }
    }
    Ok(())
}

/// Rate-limit tracker for marketplace inquiries per listing (§6.4).
///
/// Prevents spam by restricting open inquiries to at most 1 per `(sender_pubkey_hex, listing_id)`
/// and at most `max_per_listing` (default 20) total open inquiries per `listing_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryRateTracker {
    pub max_per_listing: usize,
    pub inquiries: BTreeMap<String, BTreeSet<String>>,
}

impl Default for InquiryRateTracker {
    fn default() -> Self {
        Self {
            max_per_listing: DEFAULT_MAX_OPEN_INQUIRIES_PER_LISTING,
            inquiries: BTreeMap::new(),
        }
    }
}

impl InquiryRateTracker {
    pub fn new(max_per_listing: usize) -> Self {
        Self {
            max_per_listing: if max_per_listing == 0 {
                DEFAULT_MAX_OPEN_INQUIRIES_PER_LISTING
            } else {
                max_per_listing
            },
            inquiries: BTreeMap::new(),
        }
    }

    pub fn record_inquiry(
        &mut self,
        sender_pubkey_hex: &str,
        listing_id: &str,
    ) -> Result<(), MarketError> {
        let sender = sender_pubkey_hex.trim().to_ascii_lowercase();
        let listing = listing_id.trim().to_string();

        let entry = self.inquiries.entry(listing.clone()).or_default();

        if entry.contains(&sender) {
            return Err(MarketError::Validation(format!(
                "Sender {} already has an open inquiry for listing {}",
                sender, listing
            )));
        }

        if entry.len() >= self.max_per_listing {
            return Err(MarketError::Validation(format!(
                "Listing {} reached maximum open inquiries ({})",
                listing, self.max_per_listing
            )));
        }

        entry.insert(sender);
        Ok(())
    }

    pub fn resolve_inquiry(&mut self, sender_pubkey_hex: &str, listing_id: &str) -> bool {
        let sender = sender_pubkey_hex.trim().to_ascii_lowercase();
        let listing = listing_id.trim();

        if let Some(set) = self.inquiries.get_mut(listing) {
            let removed = set.remove(&sender);
            if set.is_empty() {
                self.inquiries.remove(listing);
            }
            removed
        } else {
            false
        }
    }

    pub fn open_count(&self, listing_id: &str) -> usize {
        self.inquiries
            .get(listing_id.trim())
            .map(|s| s.len())
            .unwrap_or(0)
    }

    pub fn has_open_inquiry(&self, sender_pubkey_hex: &str, listing_id: &str) -> bool {
        let sender = sender_pubkey_hex.trim().to_ascii_lowercase();
        self.inquiries
            .get(listing_id.trim())
            .map(|s| s.contains(&sender))
            .unwrap_or(false)
    }
}
