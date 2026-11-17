use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ed25519_dalek::SigningKey;
use human_money_core::crypto::create_user_id;
use humoco_market::error::MarketError;
use humoco_market::media::{
    compute_image_token, derive_media_blob_id, derive_media_key_and_nonce,
    detect_media_data_uri_prefix, extract_media_bytes, is_supported_media_data_uri,
    is_valid_image_token, open_media_blob, parse_image_dimensions, parse_jpeg_dimensions,
    parse_png_dimensions, parse_webp_dimensions, seal_media_blob, sign_media_request,
    validate_ad_media, validate_ad_webp, validate_avatar_data_uri, JPEG_DATA_URI_PREFIX,
    MEDIA_DATA_URI_PREFIX, PNG_DATA_URI_PREFIX,
};
use humoco_market::model::{
    Ad, AdKind, CardPayloadInput, DropDescriptor, WireFilter, AVATAR_IMAGE_DIM, MAX_AD_IMAGE_DIM,
    MAX_AD_MEDIA_BYTES, MAX_AVATAR_DATA_URI_CHARS, MAX_QR_SHARE_BYTES,
};
use humoco_market::sd_card::{decoded_items, render_wire, sign_sd_card, verify_sd_card, CardItem};

fn make_key(seed_byte: u8) -> SigningKey {
    let seed = [seed_byte; 32];
    SigningKey::from_bytes(&seed)
}

fn make_vp8_webp(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(30);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(22u32).to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8 ");
    bytes.extend_from_slice(&(10u32).to_le_bytes());
    bytes.push(0x00); // keyframe bit 0 = 0
    bytes.extend_from_slice(&[0x00, 0x00]);
    bytes.extend_from_slice(&[0x9d, 0x01, 0x2a]); // VP8 start code
    bytes.extend_from_slice(&(width & 0x3fff).to_le_bytes());
    bytes.extend_from_slice(&(height & 0x3fff).to_le_bytes());
    bytes
}

fn make_vp8l_webp(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(30);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(22u32).to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8L");
    bytes.extend_from_slice(&(10u32).to_le_bytes());
    bytes.push(0x2f); // 1-byte signature
    let w_m1 = (width.saturating_sub(1) as u32) & 0x3fff;
    let h_m1 = (height.saturating_sub(1) as u32) & 0x3fff;
    let bits = w_m1 | (h_m1 << 14);
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 5]);
    bytes
}

fn make_vp8x_webp(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(30);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(22u32).to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8X");
    bytes.extend_from_slice(&(10u32).to_le_bytes());
    bytes.extend_from_slice(&[0u8; 4]); // flags
    let wm = width.saturating_sub(1);
    bytes.extend_from_slice(&[wm as u8, (wm >> 8) as u8, (wm >> 16) as u8]);
    let hm = height.saturating_sub(1);
    bytes.extend_from_slice(&[hm as u8, (hm >> 8) as u8, (hm >> 16) as u8]);
    bytes
}

fn make_test_jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    // SOI
    bytes.extend_from_slice(&[0xFF, 0xD8]);
    // APP0 (JFIF)
    bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
    bytes.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
    // SOF0 (Baseline DCT)
    bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08]);
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
    // EOI
    bytes.extend_from_slice(&[0xFF, 0xD9]);
    bytes
}

fn make_test_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    // PNG signature
    bytes.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    // IHDR chunk (13 bytes)
    bytes.extend_from_slice(&(13u32).to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[0x08, 0x06, 0x00, 0x00, 0x00]); // 8-bit RGBA
    bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // CRC dummy
    bytes
}

#[test]
fn test_parse_webp_dimensions_all_formats() {
    // VP8 lossy
    let vp8 = make_vp8_webp(640, 480);
    assert_eq!(parse_webp_dimensions(&vp8).unwrap(), (640, 480));

    // VP8L lossless
    let vp8l = make_vp8l_webp(96, 96);
    assert_eq!(parse_webp_dimensions(&vp8l).unwrap(), (96, 96));

    // VP8X extended
    let vp8x = make_vp8x_webp(800, 600);
    assert_eq!(parse_webp_dimensions(&vp8x).unwrap(), (800, 600));
}

#[test]
fn test_parse_webp_dimensions_invalid_headers() {
    // Too short (< 30 bytes)
    assert!(matches!(
        parse_webp_dimensions(&[0u8; 29]),
        Err(MarketError::Validation(_))
    ));

    // Missing RIFF
    let mut bad_riff = make_vp8_webp(100, 100);
    bad_riff[0..4].copy_from_slice(b"FORM");
    assert!(matches!(
        parse_webp_dimensions(&bad_riff),
        Err(MarketError::Validation(_))
    ));

    // Missing WEBP
    let mut bad_webp = make_vp8_webp(100, 100);
    bad_webp[8..12].copy_from_slice(b"JPEG");
    assert!(matches!(
        parse_webp_dimensions(&bad_webp),
        Err(MarketError::Validation(_))
    ));

    // Unsupported chunk fourcc
    let mut bad_chunk = make_vp8_webp(100, 100);
    bad_chunk[12..16].copy_from_slice(b"VP8Z");
    assert!(matches!(
        parse_webp_dimensions(&bad_chunk),
        Err(MarketError::Validation(_))
    ));

    // VP8 not a keyframe
    let mut not_keyframe = make_vp8_webp(100, 100);
    not_keyframe[20] = 0x01; // bit 0 set
    assert!(matches!(
        parse_webp_dimensions(&not_keyframe),
        Err(MarketError::Validation(_))
    ));

    // VP8 wrong start code
    let mut bad_start = make_vp8_webp(100, 100);
    bad_start[23..26].copy_from_slice(&[0x00, 0x00, 0x00]);
    assert!(matches!(
        parse_webp_dimensions(&bad_start),
        Err(MarketError::Validation(_))
    ));

    // VP8L wrong signature
    let mut bad_vp8l = make_vp8l_webp(100, 100);
    bad_vp8l[20] = 0x30;
    assert!(matches!(
        parse_webp_dimensions(&bad_vp8l),
        Err(MarketError::Validation(_))
    ));

    // Zero dimensions (e.g. in VP8)
    let zero_vp8 = make_vp8_webp(0, 100);
    assert!(matches!(
        parse_webp_dimensions(&zero_vp8),
        Err(MarketError::Validation(_))
    ));

    let zero_vp8_h = make_vp8_webp(100, 0);
    assert!(matches!(
        parse_webp_dimensions(&zero_vp8_h),
        Err(MarketError::Validation(_))
    ));
}

#[test]
fn test_parse_jpeg_dimensions() {
    let jpeg = make_test_jpeg(640, 480);
    assert_eq!(parse_jpeg_dimensions(&jpeg).unwrap(), (640, 480));

    let avatar_jpeg = make_test_jpeg(96, 96);
    assert_eq!(parse_jpeg_dimensions(&avatar_jpeg).unwrap(), (96, 96));

    // Invalid SOI
    assert!(matches!(
        parse_jpeg_dimensions(&[0x00, 0x00, 0x00, 0x00]),
        Err(MarketError::Validation(_))
    ));

    // Truncated
    assert!(matches!(
        parse_jpeg_dimensions(&jpeg[0..10]),
        Err(MarketError::Validation(_))
    ));
}

#[test]
fn test_parse_png_dimensions() {
    let png = make_test_png(800, 600);
    assert_eq!(parse_png_dimensions(&png).unwrap(), (800, 600));

    let avatar_png = make_test_png(96, 96);
    assert_eq!(parse_png_dimensions(&avatar_png).unwrap(), (96, 96));

    // Invalid signature
    assert!(matches!(
        parse_png_dimensions(&[0x00; 30]),
        Err(MarketError::Validation(_))
    ));

    // Truncated (< 24 bytes)
    assert!(matches!(
        parse_png_dimensions(&png[0..20]),
        Err(MarketError::Validation(_))
    ));
}

#[test]
fn test_parse_image_dimensions_and_detect_prefix() {
    let webp = make_vp8_webp(100, 100);
    let jpeg = make_test_jpeg(100, 100);
    let png = make_test_png(100, 100);

    assert_eq!(parse_image_dimensions(&webp).unwrap(), (100, 100));
    assert_eq!(parse_image_dimensions(&jpeg).unwrap(), (100, 100));
    assert_eq!(parse_image_dimensions(&png).unwrap(), (100, 100));

    assert_eq!(detect_media_data_uri_prefix(&webp), MEDIA_DATA_URI_PREFIX);
    assert_eq!(detect_media_data_uri_prefix(&jpeg), JPEG_DATA_URI_PREFIX);
    assert_eq!(detect_media_data_uri_prefix(&png), PNG_DATA_URI_PREFIX);

    // Unsupported format
    assert!(matches!(
        parse_image_dimensions(b"GIF89a..."),
        Err(MarketError::Validation(_))
    ));
}

#[test]
fn test_validate_avatar_data_uri_all_formats() {
    let valid_webp = make_vp8l_webp(96, 96);
    let uri_webp = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&valid_webp));
    assert!(is_supported_media_data_uri(&uri_webp));
    assert_eq!(extract_media_bytes(&uri_webp).unwrap(), valid_webp);
    assert!(validate_avatar_data_uri(&uri_webp).is_ok());

    let valid_jpeg = make_test_jpeg(96, 96);
    let uri_jpeg = format!("{}{}", JPEG_DATA_URI_PREFIX, STANDARD.encode(&valid_jpeg));
    assert!(is_supported_media_data_uri(&uri_jpeg));
    assert_eq!(extract_media_bytes(&uri_jpeg).unwrap(), valid_jpeg);
    assert!(validate_avatar_data_uri(&uri_jpeg).is_ok());

    let valid_png = make_test_png(96, 96);
    let uri_png = format!("{}{}", PNG_DATA_URI_PREFIX, STANDARD.encode(&valid_png));
    assert!(is_supported_media_data_uri(&uri_png));
    assert_eq!(extract_media_bytes(&uri_png).unwrap(), valid_png);
    assert!(validate_avatar_data_uri(&uri_png).is_ok());

    // Wrong dimensions for JPEG / PNG
    let bad_jpeg = make_test_jpeg(120, 120);
    let bad_jpeg_uri = format!("{}{}", JPEG_DATA_URI_PREFIX, STANDARD.encode(&bad_jpeg));
    assert!(validate_avatar_data_uri(&bad_jpeg_uri).is_err());

    let bad_png = make_test_png(48, 48);
    let bad_png_uri = format!("{}{}", PNG_DATA_URI_PREFIX, STANDARD.encode(&bad_png));
    assert!(validate_avatar_data_uri(&bad_png_uri).is_err());
}

#[test]
fn test_validate_ad_media_all_formats() {
    let webp = make_vp8_webp(800, 600);
    assert_eq!(validate_ad_media(&webp).unwrap(), (800, 600));

    let jpeg = make_test_jpeg(800, 600);
    assert_eq!(validate_ad_media(&jpeg).unwrap(), (800, 600));

    let png = make_test_png(800, 600);
    assert_eq!(validate_ad_media(&png).unwrap(), (800, 600));

    // Dimension bomb on JPEG
    let bomb_jpeg = make_test_jpeg(2000, 2000);
    assert!(validate_ad_media(&bomb_jpeg).is_err());

    // Dimension bomb on PNG
    let bomb_png = make_test_png(1025, 1025);
    assert!(validate_ad_media(&bomb_png).is_err());
}

#[test]
fn test_decompression_bomb_rejection() {
    // WebP with dimensions > 1024x1024 (e.g. 1025x768 or 8000x8000)
    let bomb_vp8 = make_vp8_webp(1025, 768);
    assert!(matches!(
        validate_ad_webp(&bomb_vp8),
        Err(MarketError::Validation(_))
    ));

    let bomb_vp8x = make_vp8x_webp(4096, 4096);
    assert!(matches!(
        validate_ad_webp(&bomb_vp8x),
        Err(MarketError::Validation(_))
    ));

    // Exactly 1024x1024 is accepted
    let max_ok = make_vp8_webp(MAX_AD_IMAGE_DIM, MAX_AD_IMAGE_DIM);
    assert_eq!(
        validate_ad_webp(&max_ok).unwrap(),
        (MAX_AD_IMAGE_DIM, MAX_AD_IMAGE_DIM)
    );
}

#[test]
fn test_validate_avatar_data_uri() {
    let valid_96 = make_vp8l_webp(AVATAR_IMAGE_DIM, AVATAR_IMAGE_DIM);
    let valid_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&valid_96));
    assert!(validate_avatar_data_uri(&valid_uri).is_ok());

    // Non-96x96 dimensions rejected
    let bad_dim = make_vp8l_webp(95, 96);
    let bad_dim_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&bad_dim));
    assert!(matches!(
        validate_avatar_data_uri(&bad_dim_uri),
        Err(MarketError::Validation(_))
    ));

    // Wrong scheme / prefix
    let wrong_prefix = format!("data:image/png;base64,{}", STANDARD.encode(&valid_96));
    assert!(matches!(
        validate_avatar_data_uri(&wrong_prefix),
        Err(MarketError::Validation(_))
    ));

    // Invalid base64
    let bad_b64 = format!("{}!!!not_base64!!!", MEDIA_DATA_URI_PREFIX);
    assert!(matches!(
        validate_avatar_data_uri(&bad_b64),
        Err(MarketError::Validation(_))
    ));

    // Exceeds max length (4096)
    let mut large_webp = valid_96.clone();
    large_webp.extend(vec![0u8; 4000]);
    let large_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&large_webp));
    assert!(large_uri.len() > MAX_AVATAR_DATA_URI_CHARS);
    assert!(matches!(
        validate_avatar_data_uri(&large_uri),
        Err(MarketError::Validation(_))
    ));
}

#[test]
fn test_image_token_validation_and_derivations() {
    // Valid 32 lowercase hex chars
    assert!(is_valid_image_token("0123456789abcdef0123456789abcdef"));
    // 31 chars -> invalid
    assert!(!is_valid_image_token("0123456789abcdef0123456789abcde"));
    // 33 chars -> invalid
    assert!(!is_valid_image_token("0123456789abcdef0123456789abcdef0"));
    // Uppercase -> invalid
    assert!(!is_valid_image_token("0123456789ABCDEF0123456789abcdef"));
    // Non-hex -> invalid
    assert!(!is_valid_image_token("0123456789abcdef0123456789abcdeg"));

    let token = "a1b2c3d4e5f60718293a4b5c6d7e8f90";
    let blob_id = derive_media_blob_id(token).unwrap();
    assert_eq!(blob_id.len(), 64);
    assert!(blob_id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));

    let (key, nonce) = derive_media_key_and_nonce(token).unwrap();
    assert_eq!(key.len(), 32);
    assert_eq!(nonce.len(), 24);

    let sk = make_key(1);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    let webp = make_vp8_webp(200, 200);
    let computed_token = compute_image_token(&did, &webp).unwrap();
    assert!(is_valid_image_token(&computed_token));
}

#[test]
fn test_seal_open_media_blob_roundtrip_and_tampering() {
    let sk = make_key(42);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    let webp_bytes = make_vp8_webp(800, 600);

    let (token, blob_id, ciphertext) = seal_media_blob(&did, &webp_bytes).unwrap();
    assert_eq!(token.len(), 32);
    assert_eq!(blob_id.len(), 64);
    assert!(!ciphertext.is_empty());

    // Successful decrypt
    let decrypted = open_media_blob(&token, &ciphertext).unwrap();
    assert_eq!(decrypted, webp_bytes);

    // Tampered ciphertext fails
    let mut tampered = ciphertext.clone();
    let len = tampered.len();
    tampered[len - 1] ^= 0xff;
    assert!(matches!(
        open_media_blob(&token, &tampered),
        Err(MarketError::Crypto(_))
    ));

    // Wrong token fails
    let wrong_token = "00000000000000000000000000000000";
    assert!(matches!(
        open_media_blob(wrong_token, &ciphertext),
        Err(MarketError::Crypto(_))
    ));

    // Empty ciphertext fails
    assert!(matches!(
        open_media_blob(&token, &[]),
        Err(MarketError::Validation(_))
    ));

    // Oversized ciphertext fails
    let oversized = vec![0u8; MAX_AD_MEDIA_BYTES + 65];
    assert!(matches!(
        open_media_blob(&token, &oversized),
        Err(MarketError::Validation(_))
    ));
}

#[test]
fn test_resigning_card_preserves_image_token_and_decryptability() {
    let sk = make_key(7);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    let webp_bytes = make_vp8_webp(400, 300);

    let (token, blob_id, ciphertext) = seal_media_blob(&did, &webp_bytes).unwrap();

    let ad = Ad {
        id: "0123456789abcdef".to_string(),
        kind: AdKind::Offer,
        title: "Offer with photo".to_string(),
        text: Some("Details".to_string()),
        pricing: None,
        published_at: 1700000000,
        renewed_at: None,
        expires_at: 1700000000 + 30 * 86_400,
        allow_inquiries: true,
        is_remote: false,
        image_token: Some(token.clone()),
    };

    let payload1 = CardPayloadInput {
        did: did.clone(),
        v: 1,
        valid_until: 1750000000,
        max_hops: 2,
        ad_count: 1,
        allow_inquiries: true,
        drops: vec![],
    };

    let card1 = sign_sd_card(payload1, vec![(CardItem::Ad(ad.clone()), 2)], &sk).unwrap();
    let wire1 = render_wire(&card1, 1, WireFilter::Full).unwrap();
    let verified1 = verify_sd_card(&wire1).unwrap();
    let items1 = decoded_items(&verified1);
    let decoded_token1 = match &items1[0].0 {
        CardItem::Ad(a) => a.image_token.as_ref().unwrap(),
        _ => panic!("Expected Ad"),
    };
    assert_eq!(decoded_token1, &token);

    // Now re-sign the card with updated version, renewed_at, and different disclosures
    let mut ad_renewed = ad.clone();
    ad_renewed.renewed_at = Some(1705000000);
    ad_renewed.expires_at = 1705000000 + 30 * 86_400;

    let payload2 = CardPayloadInput {
        did: did.clone(),
        v: 2, // bumped version
        valid_until: 1760000000,
        max_hops: 2,
        ad_count: 1,
        allow_inquiries: true,
        drops: vec![],
    };

    let card2 = sign_sd_card(payload2, vec![(CardItem::Ad(ad_renewed), 2)], &sk).unwrap();
    let wire2 = render_wire(&card2, 1, WireFilter::Full).unwrap();
    let verified2 = verify_sd_card(&wire2).unwrap();
    let items2 = decoded_items(&verified2);
    let decoded_token2 = match &items2[0].0 {
        CardItem::Ad(a) => a.image_token.as_ref().unwrap(),
        _ => panic!("Expected Ad"),
    };
    assert_eq!(decoded_token2, &token);

    // The previously sealed blob is still 100% decryptable with the token from wire2!
    let decrypted = open_media_blob(decoded_token2, &ciphertext).unwrap();
    assert_eq!(decrypted, webp_bytes);
    assert_eq!(derive_media_blob_id(decoded_token2).unwrap(), blob_id);
}

#[test]
fn test_fit_bytes_wire_filter_excludes_picture_and_ad() {
    let sk = make_key(99);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();

    let avatar_webp = make_vp8l_webp(96, 96);
    let avatar_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&avatar_webp));

    let ad = Ad {
        id: "abcdef0123456789".to_string(),
        kind: AdKind::Offer,
        title: "Excluded Ad".to_string(),
        text: None,
        pricing: None,
        published_at: 1700000000,
        renewed_at: None,
        expires_at: 1700000000 + 30 * 86_400,
        allow_inquiries: false,
        is_remote: false,
        image_token: None,
    };

    let payload = CardPayloadInput {
        did: did.clone(),
        v: 1,
        valid_until: 1800000000,
        max_hops: 2,
        ad_count: 1,
        allow_inquiries: true,
        drops: vec![DropDescriptor {
            write_url: "https://drop.example.com/write".to_string(),
            read_url: "https://drop.example.com/read".to_string(),
            slot_id: None,
            drop_pub: hex::encode(sk.verifying_key().as_bytes()),
        }],
    };

    let items = vec![
        (
            CardItem::Name {
                first_name: Some("Alice".to_string()),
                last_name: Some("Tester".to_string()),
            },
            2,
        ),
        (
            CardItem::City {
                zip_code: Some("12345".to_string()),
                city: Some("City".to_string()),
                country: Some("DE".to_string()),
            },
            2,
        ),
        (CardItem::Picture(avatar_uri), 2),
        (CardItem::Ad(ad), 2),
    ];

    let card = sign_sd_card(payload, items, &sk).unwrap();

    // Render wire with FitBytes budget
    let qr_wire = render_wire(&card, 1, WireFilter::FitBytes(MAX_QR_SHARE_BYTES)).unwrap();
    assert!(qr_wire.len() <= MAX_QR_SHARE_BYTES);

    let verified = verify_sd_card(&qr_wire).unwrap();
    let decoded = decoded_items(&verified);

    // Guaranteed 100% picture-free and ad-free!
    assert!(!decoded
        .iter()
        .any(|(item, _)| matches!(item, CardItem::Picture(_))));
    assert!(!decoded
        .iter()
        .any(|(item, _)| matches!(item, CardItem::Ad(_))));

    // Name and City must be present
    assert!(decoded
        .iter()
        .any(|(item, _)| matches!(item, CardItem::Name { .. })));
    assert!(decoded
        .iter()
        .any(|(item, _)| matches!(item, CardItem::City { .. })));
}

#[test]
fn test_sign_media_request() {
    let sk = make_key(88);
    let blob_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let timestamp = 1700000000;
    let nonce = "a1b2c3d4";
    let body = b"test payload";

    let signed = sign_media_request(sk.as_bytes(), "PUT", blob_id, timestamp, nonce, body).unwrap();
    assert_eq!(
        signed.pubkey_hex,
        hex::encode(sk.verifying_key().as_bytes())
    );
    assert_eq!(signed.timestamp, timestamp);
    assert_eq!(signed.nonce_hex, nonce);
    assert_eq!(signed.signature_hex.len(), 128);

    // Invalid blob_id length or uppercase rejected
    assert!(matches!(
        sign_media_request(sk.as_bytes(), "PUT", "too_short", timestamp, nonce, body),
        Err(MarketError::Validation(_))
    ));
    let uppercase_blob = "0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(matches!(
        sign_media_request(sk.as_bytes(), "PUT", uppercase_blob, timestamp, nonce, body),
        Err(MarketError::Validation(_))
    ));
}
