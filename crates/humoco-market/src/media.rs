use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use human_money_core::crypto::{create_user_id, get_pubkey_from_user_id};

use crate::error::MarketError;
use crate::model::{
    AVATAR_IMAGE_DIM, MAX_AD_IMAGE_DIM, MAX_AD_MEDIA_BYTES, MAX_AVATAR_DATA_URI_CHARS,
};

pub const MEDIA_DATA_URI_PREFIX: &str = "data:image/webp;base64,";
pub const JPEG_DATA_URI_PREFIX: &str = "data:image/jpeg;base64,";
pub const JPG_DATA_URI_PREFIX: &str = "data:image/jpg;base64,";
pub const PNG_DATA_URI_PREFIX: &str = "data:image/png;base64,";

/// Checks if a string starts with a supported media data URI prefix.
pub fn is_supported_media_data_uri(data_uri: &str) -> bool {
    let u = data_uri.trim();
    u.starts_with(MEDIA_DATA_URI_PREFIX)
        || u.starts_with(JPEG_DATA_URI_PREFIX)
        || u.starts_with(JPG_DATA_URI_PREFIX)
        || u.starts_with(PNG_DATA_URI_PREFIX)
}

/// Detects the data URI prefix for a given image byte slice based on magic bytes.
pub fn detect_media_data_uri_prefix(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 4 && &bytes[0..4] == b"RIFF" {
        MEDIA_DATA_URI_PREFIX
    } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        JPEG_DATA_URI_PREFIX
    } else if bytes.len() >= 8 && bytes[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        PNG_DATA_URI_PREFIX
    } else {
        MEDIA_DATA_URI_PREFIX
    }
}

/// Extracts raw bytes from a supported media data URI.
pub fn extract_media_bytes(data_uri: &str) -> Result<Vec<u8>, MarketError> {
    let trimmed = data_uri.trim();
    let prefix_len = if trimmed.starts_with(MEDIA_DATA_URI_PREFIX) {
        MEDIA_DATA_URI_PREFIX.len()
    } else if trimmed.starts_with(JPEG_DATA_URI_PREFIX) {
        JPEG_DATA_URI_PREFIX.len()
    } else if trimmed.starts_with(JPG_DATA_URI_PREFIX) {
        JPG_DATA_URI_PREFIX.len()
    } else if trimmed.starts_with(PNG_DATA_URI_PREFIX) {
        PNG_DATA_URI_PREFIX.len()
    } else {
        return Err(MarketError::Validation(
            "Media data URI must start with 'data:image/webp;base64,', 'data:image/jpeg;base64,', or 'data:image/png;base64,'".to_string(),
        ));
    };

    let b64_part = &trimmed[prefix_len..];
    STANDARD
        .decode(b64_part.trim().as_bytes())
        .map_err(|e| MarketError::Validation(format!("Invalid base64 in media data URI: {}", e)))
}

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
            let w_minus_1 = u32::from_le_bytes([webp_bytes[24], webp_bytes[25], webp_bytes[26], 0]);
            let h_minus_1 = u32::from_le_bytes([webp_bytes[27], webp_bytes[28], webp_bytes[29], 0]);
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

/// Pure Rust JPEG header parser.
/// Scans through markers until an SOF marker (Start of Frame) is found, then reads height and width.
pub fn parse_jpeg_dimensions(jpeg_bytes: &[u8]) -> Result<(u16, u16), MarketError> {
    if jpeg_bytes.len() < 4 {
        return Err(MarketError::Validation(format!(
            "JPEG bytes too short: {} < 4",
            jpeg_bytes.len()
        )));
    }
    // Check SOI marker (0xFF, 0xD8)
    if jpeg_bytes[0] != 0xFF || jpeg_bytes[1] != 0xD8 {
        return Err(MarketError::Validation(
            "Missing JPEG SOI marker (0xFF, 0xD8)".to_string(),
        ));
    }

    let mut pos = 2;
    while pos + 1 < jpeg_bytes.len() {
        if jpeg_bytes[pos] != 0xFF {
            pos += 1;
            continue;
        }
        // Skip fill bytes 0xFF
        while pos < jpeg_bytes.len() && jpeg_bytes[pos] == 0xFF {
            pos += 1;
        }
        if pos >= jpeg_bytes.len() {
            break;
        }
        let marker = jpeg_bytes[pos];
        pos += 1;

        // Ignore standalone markers without length (RST0..RST7, SOI, TEM)
        if matches!(marker, 0xD0..=0xD7 | 0xD8 | 0x01) {
            continue;
        }
        // EOI (0xD9) or SOS (0xDA): stop before scan data (scan data may contain raw 0xFF bytes)
        if marker == 0xD9 || marker == 0xDA {
            break;
        }

        // Variable length marker
        if pos + 2 > jpeg_bytes.len() {
            return Err(MarketError::Validation(
                "Truncated marker in JPEG header".to_string(),
            ));
        }
        let length = u16::from_be_bytes([jpeg_bytes[pos], jpeg_bytes[pos + 1]]) as usize;
        if length < 2 {
            return Err(MarketError::Validation(
                "Invalid marker length in JPEG".to_string(),
            ));
        }

        // Check if this is an SOF marker (SOF0..SOF3, SOF5..SOF7, SOF9..SOF11, SOF13..SOF15)
        if matches!(
            marker,
            0xC0 | 0xC1
                | 0xC2
                | 0xC3
                | 0xC5
                | 0xC6
                | 0xC7
                | 0xC9
                | 0xCA
                | 0xCB
                | 0xCD
                | 0xCE
                | 0xCF
        ) {
            // SOF segment structure:
            // length: 2 bytes
            // precision: 1 byte
            // height: 2 bytes (big endian)
            // width: 2 bytes (big endian)
            if pos + 7 > jpeg_bytes.len() {
                return Err(MarketError::Validation(
                    "Truncated SOF marker in JPEG header".to_string(),
                ));
            }
            let h = u16::from_be_bytes([jpeg_bytes[pos + 3], jpeg_bytes[pos + 4]]);
            let w = u16::from_be_bytes([jpeg_bytes[pos + 5], jpeg_bytes[pos + 6]]);
            if w == 0 || h == 0 {
                return Err(MarketError::Validation(format!(
                    "Invalid zero JPEG dimensions: {}x{}",
                    w, h
                )));
            }
            return Ok((w, h));
        }

        pos += length;
    }

    Err(MarketError::Validation(
        "No SOF marker found in JPEG stream".to_string(),
    ))
}

/// Pure Rust PNG header parser without external dependencies.
/// Reads dimensions from the IHDR chunk.
pub fn parse_png_dimensions(png_bytes: &[u8]) -> Result<(u16, u16), MarketError> {
    if png_bytes.len() < 24 {
        return Err(MarketError::Validation(format!(
            "PNG bytes too short: {} < 24",
            png_bytes.len()
        )));
    }
    const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if png_bytes[0..8] != PNG_SIG {
        return Err(MarketError::Validation(
            "Missing PNG signature header".to_string(),
        ));
    }
    if &png_bytes[12..16] != b"IHDR" {
        return Err(MarketError::Validation(
            "Expected IHDR chunk in PNG header".to_string(),
        ));
    }
    let w_u32 = u32::from_be_bytes([png_bytes[16], png_bytes[17], png_bytes[18], png_bytes[19]]);
    let h_u32 = u32::from_be_bytes([png_bytes[20], png_bytes[21], png_bytes[22], png_bytes[23]]);

    if w_u32 == 0 || h_u32 == 0 {
        return Err(MarketError::Validation(format!(
            "Invalid zero PNG dimensions: {}x{}",
            w_u32, h_u32
        )));
    }
    if w_u32 > 65535 || h_u32 > 65535 {
        return Err(MarketError::Validation(format!(
            "PNG dimensions exceed u16 limit: {}x{}",
            w_u32, h_u32
        )));
    }
    Ok((w_u32 as u16, h_u32 as u16))
}

/// Pure Rust image dimension parser supporting WebP, JPEG, and PNG.
pub fn parse_image_dimensions(bytes: &[u8]) -> Result<(u16, u16), MarketError> {
    if bytes.len() >= 4 && &bytes[0..4] == b"RIFF" {
        parse_webp_dimensions(bytes)
    } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        parse_jpeg_dimensions(bytes)
    } else if bytes.len() >= 8 && bytes[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        parse_png_dimensions(bytes)
    } else {
        Err(MarketError::Validation(
            "Unsupported image format: must be WebP, JPEG, or PNG".to_string(),
        ))
    }
}

/// Validates an inline avatar data URI.
/// Must be a supported data URI, have total length <= `MAX_AVATAR_DATA_URI_CHARS`,
/// and decode to a valid image matching its declared format (WebP, JPEG, or PNG) with exact dimensions `(96, 96)`.
pub fn validate_avatar_data_uri(data_uri: &str) -> Result<(), MarketError> {
    if !is_supported_media_data_uri(data_uri) {
        return Err(MarketError::Validation(format!(
            "Avatar data URI must start with '{}', '{}', or '{}'",
            MEDIA_DATA_URI_PREFIX, JPEG_DATA_URI_PREFIX, PNG_DATA_URI_PREFIX
        )));
    }
    if data_uri.chars().count() > MAX_AVATAR_DATA_URI_CHARS {
        return Err(MarketError::Validation(format!(
            "Avatar data URI length {} exceeds maximum {}",
            data_uri.chars().count(),
            MAX_AVATAR_DATA_URI_CHARS
        )));
    }
    let image_bytes = extract_media_bytes(data_uri)?;
    let trimmed = data_uri.trim();
    let (w, h) = if trimmed.starts_with(MEDIA_DATA_URI_PREFIX) {
        parse_webp_dimensions(&image_bytes)?
    } else if trimmed.starts_with(JPEG_DATA_URI_PREFIX) || trimmed.starts_with(JPG_DATA_URI_PREFIX)
    {
        parse_jpeg_dimensions(&image_bytes)?
    } else if trimmed.starts_with(PNG_DATA_URI_PREFIX) {
        parse_png_dimensions(&image_bytes)?
    } else {
        return Err(MarketError::Validation(
            "Unsupported image format".to_string(),
        ));
    };

    if (w, h) != (AVATAR_IMAGE_DIM, AVATAR_IMAGE_DIM) {
        return Err(MarketError::Validation(format!(
            "Avatar image dimensions must be {}x{}, got {}x{}",
            AVATAR_IMAGE_DIM, AVATAR_IMAGE_DIM, w, h
        )));
    }
    Ok(())
}

/// Validates image bytes intended for an ad (WebP, JPEG, or PNG).
/// Must be non-empty, <= `MAX_AD_MEDIA_BYTES`, and dimensions <= `MAX_AD_IMAGE_DIM` x `MAX_AD_IMAGE_DIM`.
pub fn validate_ad_media(media_bytes: &[u8]) -> Result<(u16, u16), MarketError> {
    if media_bytes.is_empty() {
        return Err(MarketError::Validation(
            "Ad image bytes cannot be empty".to_string(),
        ));
    }
    if media_bytes.len() > MAX_AD_MEDIA_BYTES {
        return Err(MarketError::Validation(format!(
            "Ad image size {} exceeds maximum {}",
            media_bytes.len(),
            MAX_AD_MEDIA_BYTES
        )));
    }
    let (w, h) = parse_image_dimensions(media_bytes)?;
    if w > MAX_AD_IMAGE_DIM || h > MAX_AD_IMAGE_DIM {
        return Err(MarketError::Validation(format!(
            "Ad image dimensions {}x{} exceed maximum {}x{}",
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

    validate_ad_media(webp_bytes)?;

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

    let cipher =
        XChaCha20Poly1305::new_from_slice(&key).map_err(|e| MarketError::Crypto(e.to_string()))?;
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

/// Decrypts a media blob using the image token, verifies the WebP dimensions, and
/// confirms the image token matches `owner_did` + plaintext content.
pub fn open_media_blob(
    owner_did: &str,
    image_token: &str,
    ciphertext: &[u8],
) -> Result<Vec<u8>, MarketError> {
    if ciphertext.is_empty() || ciphertext.len() > MAX_AD_MEDIA_BYTES + 64 {
        return Err(MarketError::Validation(format!(
            "Invalid media ciphertext length: {}",
            ciphertext.len()
        )));
    }
    let blob_id = derive_media_blob_id(image_token)?;
    let (key, nonce) = derive_media_key_and_nonce(image_token)?;

    let cipher =
        XChaCha20Poly1305::new_from_slice(&key).map_err(|e| MarketError::Crypto(e.to_string()))?;
    let xnonce = XNonce::from_slice(&nonce);
    let payload = Payload {
        msg: ciphertext,
        aad: blob_id.as_bytes(),
    };
    let plaintext = cipher
        .decrypt(xnonce, payload)
        .map_err(|e| MarketError::Crypto(format!("Decryption failed: {}", e)))?;

    validate_ad_media(&plaintext)?;
    let expected_token = compute_image_token(owner_did, &plaintext)?;
    if expected_token != image_token {
        return Err(MarketError::Crypto(
            "Media image_token does not match owner DID and content".to_string(),
        ));
    }

    Ok(plaintext)
}
