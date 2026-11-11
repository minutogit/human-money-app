use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use human_money_core::crypto::{create_user_id, get_pubkey_from_user_id};

use crate::error::MarketError;
use crate::model::{
    AVATAR_IMAGE_DIM, MAX_AD_IMAGE_DIM, MAX_AD_MEDIA_BYTES, MAX_AVATAR_DATA_URI_CHARS,
};
use crate::sync_access::SignedDropHeaders;

pub const MEDIA_DATA_URI_PREFIX: &str = "data:image/webp;base64,";

/// Pure Rust RIFF/WebP header parser without external dependencies.
/// Supports VP8 (lossy), VP8L (lossless), and VP8X (extended).
pub fn parse_webp_dimensions(webp_bytes: &[u8]) -> Result<(u16, u16), MarketError> {
    if webp_bytes.len() < 30 {
        return Err(MarketError::Validation(format!(
            "WebP bytes too short: {} < 30",
            webp_bytes.len()
        )));
    }
    if &webp_bytes[0..4] != b"RIFF" {
        return Err(MarketError::Validation(
            "Missing RIFF header in WebP".to_string(),
        ));
    }
    if &webp_bytes[8..12] != b"WEBP" {
        return Err(MarketError::Validation(
            "Missing WEBP header in WebP".to_string(),
        ));
    }

    let fourcc = &webp_bytes[12..16];
    let (w, h) = match fourcc {
        b"VP8 " => {
            // VP8 lossy: keyframe check (bit 0 of byte 20 must be 0)
            if (webp_bytes[20] & 1) != 0 {
                return Err(MarketError::Validation(
                    "VP8 frame is not a keyframe".to_string(),
                ));
            }
            // Start code check: 0x9d, 0x01, 0x2a
            if webp_bytes[23..26] != [0x9d, 0x01, 0x2a] {
                return Err(MarketError::Validation(
                    "Invalid VP8 start code in header".to_string(),
                ));
            }
            let w = u16::from_le_bytes([webp_bytes[26], webp_bytes[27]]) & 0x3fff;
            let h = u16::from_le_bytes([webp_bytes[28], webp_bytes[29]]) & 0x3fff;
            (w, h)
        }
        b"VP8L" => {
            // VP8L lossless: 1-byte signature must be 0x2f
            if webp_bytes[20] != 0x2f {
                return Err(MarketError::Validation(
                    "Invalid VP8L signature byte".to_string(),
                ));
            }
            let bits = u32::from_le_bytes([
                webp_bytes[21],
                webp_bytes[22],
                webp_bytes[23],
                webp_bytes[24],
            ]);
            let w = ((bits & 0x3fff) + 1) as u16;
            let h = (((bits >> 14) & 0x3fff) + 1) as u16;
            (w, h)
        }
        b"VP8X" => {
            // VP8X extended:
            // 24-bit canvas width - 1 at bytes 24..27
            // 24-bit canvas height - 1 at bytes 27..30
            let w_minus_1 =
                u32::from_le_bytes([webp_bytes[24], webp_bytes[25], webp_bytes[26], 0]);
            let h_minus_1 =
                u32::from_le_bytes([webp_bytes[27], webp_bytes[28], webp_bytes[29], 0]);
            if w_minus_1 >= 65535 || h_minus_1 >= 65535 {
                return Err(MarketError::Validation(format!(
                    "VP8X dimensions exceed u16 limit: w-1={}, h-1={}",
                    w_minus_1, h_minus_1
                )));
            }
            let w = (w_minus_1 + 1) as u16;
            let h = (h_minus_1 + 1) as u16;
            (w, h)
        }
        other => {
            return Err(MarketError::Validation(format!(
                "Unsupported WebP chunk fourcc: {:?}",
                String::from_utf8_lossy(other)
            )));
        }
    };

    if w == 0 || h == 0 {
        return Err(MarketError::Validation(format!(
            "Invalid zero WebP dimensions: {}x{}",
            w, h
        )));
    }

    Ok((w, h))
}

/// Validates an inline avatar data URI.
/// Must start with `MEDIA_DATA_URI_PREFIX`, have total length <= `MAX_AVATAR_DATA_URI_CHARS`,
/// and decode to a valid WebP with exact dimensions `(96, 96)`.
pub fn validate_avatar_data_uri(data_uri: &str) -> Result<(), MarketError> {
    if !data_uri.starts_with(MEDIA_DATA_URI_PREFIX) {
        return Err(MarketError::Validation(format!(
            "Avatar data URI must start with '{}'",
            MEDIA_DATA_URI_PREFIX
        )));
    }
    if data_uri.chars().count() > MAX_AVATAR_DATA_URI_CHARS {
        return Err(MarketError::Validation(format!(
            "Avatar data URI length {} exceeds maximum {}",
            data_uri.chars().count(),
            MAX_AVATAR_DATA_URI_CHARS
        )));
    }
    let suffix = &data_uri[MEDIA_DATA_URI_PREFIX.len()..];
    let webp_bytes = STANDARD
        .decode(suffix.as_bytes())
        .map_err(|e| MarketError::Validation(format!("Invalid base64 in avatar data URI: {}", e)))?;

    let (w, h) = parse_webp_dimensions(&webp_bytes)?;
    if (w, h) != (AVATAR_IMAGE_DIM, AVATAR_IMAGE_DIM) {
        return Err(MarketError::Validation(format!(
            "Avatar image dimensions must be {}x{}, got {}x{}",
            AVATAR_IMAGE_DIM, AVATAR_IMAGE_DIM, w, h
        )));
    }
    Ok(())
}

/// Validates WebP image bytes intended for an ad.
/// Must be non-empty, <= `MAX_AD_MEDIA_BYTES`, and dimensions <= `MAX_AD_IMAGE_DIM` x `MAX_AD_IMAGE_DIM`.
pub fn validate_ad_webp(webp_bytes: &[u8]) -> Result<(u16, u16), MarketError> {
    if webp_bytes.is_empty() {
        return Err(MarketError::Validation(
            "WebP bytes cannot be empty".to_string(),
        ));
    }
    if webp_bytes.len() > MAX_AD_MEDIA_BYTES {
        return Err(MarketError::Validation(format!(
            "WebP size {} exceeds maximum {}",
            webp_bytes.len(),
            MAX_AD_MEDIA_BYTES
        )));
    }
    let (w, h) = parse_webp_dimensions(webp_bytes)?;
    if w > MAX_AD_IMAGE_DIM || h > MAX_AD_IMAGE_DIM {
        return Err(MarketError::Validation(format!(
            "WebP dimensions {}x{} exceed maximum {}x{}",
            w, h, MAX_AD_IMAGE_DIM, MAX_AD_IMAGE_DIM
        )));
    }
    Ok((w, h))
}

/// Checks whether a given string is a valid image token (exactly 32 lowercase hex characters).
pub fn is_valid_image_token(token: &str) -> bool {
    token.len() == 32 && token.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

/// Computes the 128-bit image token (32 lowercase hex chars) from the owner DID and WebP bytes.
pub fn compute_image_token(owner_did: &str, webp_bytes: &[u8]) -> Result<String, MarketError> {
    let pk = get_pubkey_from_user_id(owner_did)
        .map_err(|e| MarketError::Identity(format!("Invalid owner DID: {}", e)))?;
    let root_did = create_user_id(&pk, None)
        .map_err(|e| MarketError::Identity(format!("Failed to recreate root DID: {}", e)))?;

    validate_ad_webp(webp_bytes)?;

    let mut material = Vec::with_capacity(root_did.len() + 1 + webp_bytes.len());
    material.extend_from_slice(root_did.as_bytes());
    material.push(0);
    material.extend_from_slice(webp_bytes);

    let derived = blake3::derive_key("HUMOCO-MEDIA-TOKEN-V1", &material);
    Ok(hex::encode(&derived[0..16]))
}

/// Derives the 64-character lowercase hex blob ID from the image token.
pub fn derive_media_blob_id(image_token: &str) -> Result<String, MarketError> {
    if !is_valid_image_token(image_token) {
        return Err(MarketError::Validation(format!(
            "Invalid image token '{}': expected 32 lowercase hex chars",
            image_token
        )));
    }
    let derived = blake3::derive_key("HUMOCO-MEDIA-BLOB-ID-V1", image_token.as_bytes());
    Ok(hex::encode(derived))
}

/// Derives the 32-byte XChaCha20Poly1305 key and 24-byte nonce from the image token.
pub fn derive_media_key_and_nonce(image_token: &str) -> Result<([u8; 32], [u8; 24]), MarketError> {
    if !is_valid_image_token(image_token) {
        return Err(MarketError::Validation(format!(
            "Invalid image token '{}': expected 32 lowercase hex chars",
            image_token
        )));
    }
    let key = blake3::derive_key("HUMOCO-MEDIA-KEY-V1", image_token.as_bytes());
    let nonce_32 = blake3::derive_key("HUMOCO-MEDIA-NONCE-V1", image_token.as_bytes());
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&nonce_32[0..24]);
    Ok((key, nonce))
}

/// Encrypts WebP image bytes into an authenticated media blob.
/// Returns `(image_token, blob_id, ciphertext)`.
pub fn seal_media_blob(
    owner_did: &str,
    webp_bytes: &[u8],
) -> Result<(String, String, Vec<u8>), MarketError> {
    let image_token = compute_image_token(owner_did, webp_bytes)?;
    let blob_id = derive_media_blob_id(&image_token)?;
    let (key, nonce) = derive_media_key_and_nonce(&image_token)?;

    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    let xnonce = XNonce::from_slice(&nonce);
    let payload = Payload {
        msg: webp_bytes,
        aad: blob_id.as_bytes(),
    };
    let ciphertext = cipher
        .encrypt(xnonce, payload)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;

    Ok((image_token, blob_id, ciphertext))
}

/// Decrypts a media blob using the image token and verifies the WebP dimensions.
pub fn open_media_blob(image_token: &str, ciphertext: &[u8]) -> Result<Vec<u8>, MarketError> {
    if ciphertext.is_empty() || ciphertext.len() > MAX_AD_MEDIA_BYTES + 64 {
        return Err(MarketError::Validation(format!(
            "Invalid media ciphertext length: {}",
            ciphertext.len()
        )));
    }
    let blob_id = derive_media_blob_id(image_token)?;
    let (key, nonce) = derive_media_key_and_nonce(image_token)?;

    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    let xnonce = XNonce::from_slice(&nonce);
    let payload = Payload {
        msg: ciphertext,
        aad: blob_id.as_bytes(),
    };
    let plaintext = cipher
        .decrypt(xnonce, payload)
        .map_err(|e| MarketError::Crypto(format!("Decryption failed: {}", e)))?;

    validate_ad_webp(&plaintext)?;

    Ok(plaintext)
}

/// Signs a media drop request using a member's stealth secret key.
pub fn sign_media_request(
    member_stealth_sk: &[u8; 32],
    method: &str,
    blob_id: &str,
    timestamp: u64,
    nonce_hex: &str,
    body: &[u8],
) -> Result<SignedDropHeaders, MarketError> {
    if blob_id.len() != 64 || !blob_id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        return Err(MarketError::Validation(format!(
            "Invalid media blob_id '{}': expected 64 lowercase hex chars",
            blob_id
        )));
    }
    crate::sync_access::sign_member_request(
        member_stealth_sk,
        method,
        &format!("media:{}", blob_id),
        timestamp,
        nonce_hex,
        body,
    )
}
