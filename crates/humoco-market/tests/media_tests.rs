use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ed25519_dalek::SigningKey;
use human_money_core::crypto::create_user_id;
use humoco_market::error::MarketError;
use humoco_market::media::{
    compute_image_token, derive_media_blob_id, derive_media_key_and_nonce,
    detect_media_data_uri_prefix, extract_media_bytes, is_supported_media_data_uri,
    is_valid_image_token, open_media_blob, parse_image_dimensions, parse_jpeg_dimensions,
    parse_png_dimensions, parse_webp_dimensions, seal_media_blob, validate_ad_media,
    validate_avatar_data_uri, JPEG_DATA_URI_PREFIX, JPG_DATA_URI_PREFIX, MEDIA_DATA_URI_PREFIX,
    PNG_DATA_URI_PREFIX,
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

    // VP8L lossless (asymmetric so width/height swaps cannot pass)
    let vp8l = make_vp8l_webp(96, 128);
    assert_eq!(parse_webp_dimensions(&vp8l).unwrap(), (96, 128));

    // VP8X extended
    let vp8x = make_vp8x_webp(800, 600);
    assert_eq!(parse_webp_dimensions(&vp8x).unwrap(), (800, 600));
}

#[test]
fn test_parse_webp_dimensions_invalid_headers() {
    // Positive baselines prove valid headers pass before targeted header corruptions fail.
    let valid_vp8 = make_vp8_webp(100, 100);
    let valid_vp8l = make_vp8l_webp(100, 100);
    let valid_vp8x = make_vp8x_webp(100, 100);
    assert_eq!(parse_webp_dimensions(&valid_vp8).unwrap(), (100, 100));
    assert_eq!(parse_webp_dimensions(&valid_vp8l).unwrap(), (100, 100));
    assert_eq!(parse_webp_dimensions(&valid_vp8x).unwrap(), (100, 100));

    // Too short (< 30 bytes)
    let err = parse_webp_dimensions(&[0u8; 29]).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("WebP bytes too short: 29 < 30")),
        "expected WebP short-length error, got {err:?}"
    );

    // Missing RIFF
    let mut bad_riff = valid_vp8.clone();
    bad_riff[0..4].copy_from_slice(b"FORM");
    let err = parse_webp_dimensions(&bad_riff).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Missing RIFF header")),
        "expected Missing RIFF header error, got {err:?}"
    );

    // Missing WEBP
    let mut bad_webp = valid_vp8.clone();
    bad_webp[8..12].copy_from_slice(b"JPEG");
    let err = parse_webp_dimensions(&bad_webp).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Missing WEBP header")),
        "expected Missing WEBP header error, got {err:?}"
    );

    // Unsupported chunk fourcc
    let mut bad_chunk = valid_vp8.clone();
    bad_chunk[12..16].copy_from_slice(b"VP8Z");
    let err = parse_webp_dimensions(&bad_chunk).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Unsupported WebP chunk fourcc")),
        "expected unsupported fourcc error, got {err:?}"
    );

    // Chunk size rejections (VP8 < 10, VP8L < 5, VP8X != 10)
    let mut bad_vp8_size = valid_vp8.clone();
    bad_vp8_size[16..20].copy_from_slice(&(9u32).to_le_bytes());
    let err = parse_webp_dimensions(&bad_vp8_size).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid VP8 chunk size: 9 < 10")),
        "expected VP8 chunk size error, got {err:?}"
    );

    let mut bad_vp8l_size = valid_vp8l.clone();
    bad_vp8l_size[16..20].copy_from_slice(&(4u32).to_le_bytes());
    let err = parse_webp_dimensions(&bad_vp8l_size).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid VP8L chunk size: 4 < 5")),
        "expected VP8L chunk size error, got {err:?}"
    );

    let mut bad_vp8x_size = valid_vp8x;
    bad_vp8x_size[16..20].copy_from_slice(&(9u32).to_le_bytes());
    let err = parse_webp_dimensions(&bad_vp8x_size).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid VP8X chunk size: 9 != 10")),
        "expected VP8X chunk size error, got {err:?}"
    );

    // VP8 not a keyframe
    let mut not_keyframe = valid_vp8.clone();
    not_keyframe[20] = 0x01; // bit 0 set
    let err = parse_webp_dimensions(&not_keyframe).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("VP8 frame is not a keyframe")),
        "expected non-keyframe error, got {err:?}"
    );

    // VP8 wrong start code
    let mut bad_start = valid_vp8;
    bad_start[23..26].copy_from_slice(&[0x00, 0x00, 0x00]);
    let err = parse_webp_dimensions(&bad_start).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid VP8 start code")),
        "expected invalid VP8 start code error, got {err:?}"
    );

    // VP8L wrong signature
    let mut bad_vp8l = valid_vp8l;
    bad_vp8l[20] = 0x30;
    let err = parse_webp_dimensions(&bad_vp8l).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid VP8L signature byte")),
        "expected invalid VP8L signature error, got {err:?}"
    );

    // Zero dimensions (width or height in VP8)
    let err = parse_webp_dimensions(&make_vp8_webp(0, 100)).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid zero VP8 dimensions: 0x100")),
        "expected zero width error, got {err:?}"
    );

    let err = parse_webp_dimensions(&make_vp8_webp(100, 0)).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid zero VP8 dimensions: 100x0")),
        "expected zero height error, got {err:?}"
    );
}

#[test]
fn test_parse_jpeg_dimensions() {
    let jpeg = make_test_jpeg(640, 480);
    assert_eq!(parse_jpeg_dimensions(&jpeg).unwrap(), (640, 480));

    let avatar_jpeg = make_test_jpeg(96, 96);
    assert_eq!(parse_jpeg_dimensions(&avatar_jpeg).unwrap(), (96, 96));

    // Too short (< 4 bytes)
    let err = parse_jpeg_dimensions(&[0xFF, 0xD8, 0xFF]).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("JPEG bytes too short: 3 < 4")),
        "expected JPEG short-length error, got {err:?}"
    );

    // Invalid SOI
    let err = parse_jpeg_dimensions(&[0x00, 0x00, 0x00, 0x00]).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Missing JPEG SOI marker")),
        "expected Missing JPEG SOI marker error, got {err:?}"
    );

    // Truncated marker length
    let err = parse_jpeg_dimensions(&jpeg[0..10]).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid or truncated marker length in JPEG")),
        "expected truncated JPEG marker error, got {err:?}"
    );
}

#[test]
fn test_parse_png_dimensions() {
    let png = make_test_png(800, 600);
    assert_eq!(parse_png_dimensions(&png).unwrap(), (800, 600));

    let avatar_png = make_test_png(96, 96);
    assert_eq!(parse_png_dimensions(&avatar_png).unwrap(), (96, 96));

    // Exact u16 max boundary (65535x65535) passes cleanly.
    let max_u16_png = make_test_png(65535, 65535);
    assert_eq!(parse_png_dimensions(&max_u16_png).unwrap(), (65535, 65535));

    // Invalid signature on full 33-byte PNG (previously &[0u8; 30] hit the length gate first!)
    let mut bad_sig = avatar_png.clone();
    bad_sig[0] = 0x00;
    let err = parse_png_dimensions(&bad_sig).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Missing PNG signature header")),
        "expected Missing PNG signature header error, got {err:?}"
    );

    // Truncated boundary (< 33 bytes -> 32 bytes)
    let err = parse_png_dimensions(&png[0..32]).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("PNG bytes too short: 32 < 33")),
        "expected PNG short-length error, got {err:?}"
    );

    // Bad IHDR length
    let mut bad_ihdr_len = avatar_png.clone();
    bad_ihdr_len[11] = 12;
    let err = parse_png_dimensions(&bad_ihdr_len).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid IHDR chunk length in PNG header: 12 != 13")),
        "expected bad IHDR length error, got {err:?}"
    );

    // Bad IHDR chunk type
    let mut bad_ihdr_fourcc = avatar_png;
    bad_ihdr_fourcc[12..16].copy_from_slice(b"IDAT");
    let err = parse_png_dimensions(&bad_ihdr_fourcc).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Expected IHDR chunk in PNG header")),
        "expected missing IHDR chunk error, got {err:?}"
    );

    // Zero dimensions (width and height)
    let err = parse_png_dimensions(&make_test_png(0, 100)).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid zero PNG dimensions: 0x100")),
        "expected zero width PNG error, got {err:?}"
    );
    let err = parse_png_dimensions(&make_test_png(100, 0)).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid zero PNG dimensions: 100x0")),
        "expected zero height PNG error, got {err:?}"
    );

    // u16 overflow (width > 65535 and height > 65535)
    let err = parse_png_dimensions(&make_test_png(65536, 100)).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("PNG dimensions exceed u16 limit")),
        "expected PNG u16 width overflow error, got {err:?}"
    );
    let err = parse_png_dimensions(&make_test_png(100, 65536)).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("PNG dimensions exceed u16 limit")),
        "expected PNG u16 height overflow error, got {err:?}"
    );
}

#[test]
fn test_parse_image_dimensions_and_detect_prefix() {
    let webp = make_vp8_webp(120, 80);
    let jpeg = make_test_jpeg(120, 80);
    let png = make_test_png(120, 80);

    assert_eq!(parse_image_dimensions(&webp).unwrap(), (120, 80));
    assert_eq!(parse_image_dimensions(&jpeg).unwrap(), (120, 80));
    assert_eq!(parse_image_dimensions(&png).unwrap(), (120, 80));

    assert_eq!(detect_media_data_uri_prefix(&webp), MEDIA_DATA_URI_PREFIX);
    assert_eq!(detect_media_data_uri_prefix(&jpeg), JPEG_DATA_URI_PREFIX);
    assert_eq!(detect_media_data_uri_prefix(&png), PNG_DATA_URI_PREFIX);
    assert_eq!(
        detect_media_data_uri_prefix(b"unknown"),
        MEDIA_DATA_URI_PREFIX
    );

    // Unsupported format
    let err = parse_image_dimensions(b"GIF89a...").unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Unsupported image format")),
        "expected unsupported image format error, got {err:?}"
    );
}

#[test]
fn test_validate_avatar_data_uri_all_formats() {
    let valid_webp = make_vp8l_webp(96, 96);
    let uri_webp = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&valid_webp));
    assert!(is_supported_media_data_uri(&uri_webp));
    assert_eq!(extract_media_bytes(&uri_webp).unwrap(), valid_webp);
    validate_avatar_data_uri(&uri_webp).expect("valid 96x96 WebP avatar URI must pass");

    let valid_jpeg = make_test_jpeg(96, 96);
    let uri_jpeg = format!("{}{}", JPEG_DATA_URI_PREFIX, STANDARD.encode(&valid_jpeg));
    assert!(is_supported_media_data_uri(&uri_jpeg));
    assert_eq!(extract_media_bytes(&uri_jpeg).unwrap(), valid_jpeg);
    validate_avatar_data_uri(&uri_jpeg).expect("valid 96x96 JPEG avatar URI must pass");

    let uri_jpg = format!("{}{}", JPG_DATA_URI_PREFIX, STANDARD.encode(&valid_jpeg));
    assert!(is_supported_media_data_uri(&uri_jpg));
    assert_eq!(extract_media_bytes(&uri_jpg).unwrap(), valid_jpeg);
    validate_avatar_data_uri(&uri_jpg).expect("valid 96x96 JPG alias avatar URI must pass");

    let valid_png = make_test_png(96, 96);
    let uri_png = format!("{}{}", PNG_DATA_URI_PREFIX, STANDARD.encode(&valid_png));
    assert!(is_supported_media_data_uri(&uri_png));
    assert_eq!(extract_media_bytes(&uri_png).unwrap(), valid_png);
    validate_avatar_data_uri(&uri_png).expect("valid 96x96 PNG avatar URI must pass");

    // Wrong dimensions for JPEG / PNG (exact error variant and dimensions message)
    let bad_jpeg = make_test_jpeg(120, 120);
    let bad_jpeg_uri = format!("{}{}", JPEG_DATA_URI_PREFIX, STANDARD.encode(&bad_jpeg));
    let err = validate_avatar_data_uri(&bad_jpeg_uri).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Avatar image dimensions must be 96x96, got 120x120")),
        "expected 120x120 avatar dimension rejection, got {err:?}"
    );

    let bad_png = make_test_png(48, 48);
    let bad_png_uri = format!("{}{}", PNG_DATA_URI_PREFIX, STANDARD.encode(&bad_png));
    let err = validate_avatar_data_uri(&bad_png_uri).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Avatar image dimensions must be 96x96, got 48x48")),
        "expected 48x48 avatar dimension rejection, got {err:?}"
    );
}

#[test]
fn test_validate_ad_media_all_formats() {
    let webp = make_vp8_webp(800, 600);
    assert_eq!(validate_ad_media(&webp).unwrap(), (800, 600));

    let jpeg = make_test_jpeg(800, 600);
    assert_eq!(validate_ad_media(&jpeg).unwrap(), (800, 600));

    let png = make_test_png(800, 600);
    assert_eq!(validate_ad_media(&png).unwrap(), (800, 600));

    // Empty bytes
    let err = validate_ad_media(&[]).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Ad image bytes cannot be empty")),
        "expected empty ad image error, got {err:?}"
    );

    // Exact byte limit boundary (MAX_AD_MEDIA_BYTES accepted, +1 rejected)
    let mut exact_max_bytes = webp;
    exact_max_bytes.resize(MAX_AD_MEDIA_BYTES, 0);
    assert_eq!(validate_ad_media(&exact_max_bytes).unwrap(), (800, 600));

    let mut over_max_bytes = exact_max_bytes;
    over_max_bytes.push(0);
    let err = validate_ad_media(&over_max_bytes).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("exceeds maximum")),
        "expected ad byte size limit error, got {err:?}"
    );

    // Dimension bomb on JPEG
    let bomb_jpeg = make_test_jpeg(2000, 2000);
    let err = validate_ad_media(&bomb_jpeg).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Ad image dimensions 2000x2000 exceed maximum 1024x1024")),
        "expected JPEG dimension bomb error, got {err:?}"
    );

    // Dimension bomb on PNG
    let bomb_png = make_test_png(1025, 1025);
    let err = validate_ad_media(&bomb_png).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Ad image dimensions 1025x1025 exceed maximum 1024x1024")),
        "expected PNG dimension bomb error, got {err:?}"
    );
}

#[test]
fn test_decompression_bomb_rejection() {
    // WebP width > 1024 and height > 1024 (catches || -> && mutation)
    let bomb_vp8_w = make_vp8_webp(1025, 768);
    let err = validate_ad_media(&bomb_vp8_w).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Ad image dimensions 1025x768 exceed maximum 1024x1024")),
        "expected WebP width bomb error, got {err:?}"
    );

    let bomb_vp8_h = make_vp8_webp(768, 1025);
    let err = validate_ad_media(&bomb_vp8_h).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Ad image dimensions 768x1025 exceed maximum 1024x1024")),
        "expected WebP height bomb error, got {err:?}"
    );

    let bomb_vp8x = make_vp8x_webp(4096, 4096);
    let err = validate_ad_media(&bomb_vp8x).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Ad image dimensions 4096x4096 exceed maximum 1024x1024")),
        "expected VP8X dimension bomb error, got {err:?}"
    );

    // Exactly 1024x1024 is accepted
    let max_ok = make_vp8_webp(MAX_AD_IMAGE_DIM, MAX_AD_IMAGE_DIM);
    assert_eq!(
        validate_ad_media(&max_ok).unwrap(),
        (MAX_AD_IMAGE_DIM, MAX_AD_IMAGE_DIM)
    );
}

#[test]
fn test_validate_avatar_data_uri() {
    let valid_96 = make_vp8l_webp(AVATAR_IMAGE_DIM, AVATAR_IMAGE_DIM);
    let valid_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&valid_96));
    validate_avatar_data_uri(&valid_uri).expect("valid 96x96 WebP avatar URI must succeed");

    // Whitespace or control characters rejected before further checks
    let err = validate_avatar_data_uri(&format!("{valid_uri} ")).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("must not contain whitespace or control characters")),
        "expected whitespace error, got {err:?}"
    );
    let err = validate_avatar_data_uri(&format!("{valid_uri}\x07")).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("must not contain whitespace or control characters")),
        "expected control character error, got {err:?}"
    );

    // Non-96x96 dimensions rejected (both width and height mismatches)
    let bad_w_uri = format!(
        "{}{}",
        MEDIA_DATA_URI_PREFIX,
        STANDARD.encode(make_vp8l_webp(95, 96))
    );
    let err = validate_avatar_data_uri(&bad_w_uri).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Avatar image dimensions must be 96x96, got 95x96")),
        "expected 95x96 dimension error, got {err:?}"
    );

    let bad_h_uri = format!(
        "{}{}",
        MEDIA_DATA_URI_PREFIX,
        STANDARD.encode(make_vp8l_webp(96, 95))
    );
    let err = validate_avatar_data_uri(&bad_h_uri).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Avatar image dimensions must be 96x96, got 96x95")),
        "expected 96x95 dimension error, got {err:?}"
    );

    // Truly unsupported scheme / prefix (previously PNG failed later in PNG length check!)
    let wrong_prefix = format!("data:image/gif;base64,{}", STANDARD.encode(&valid_96));
    let err = validate_avatar_data_uri(&wrong_prefix).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Avatar data URI must start with")),
        "expected unsupported avatar prefix error, got {err:?}"
    );

    // Declared MIME mismatch: valid PNG bytes under WebP prefix fails RIFF parser
    let valid_png_96 = make_test_png(96, 96);
    let mismatched = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&valid_png_96));
    let err = validate_avatar_data_uri(&mismatched).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Missing RIFF header")),
        "expected WebP parser failure on PNG bytes, got {err:?}"
    );

    // Invalid base64
    let bad_b64 = format!("{}!!!not_base64!!!", MEDIA_DATA_URI_PREFIX);
    let err = validate_avatar_data_uri(&bad_b64).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid base64 in media data URI")),
        "expected invalid base64 error, got {err:?}"
    );

    // Exact MAX_AVATAR_DATA_URI_CHARS (4096) boundary:
    // - 3054 raw bytes -> 4072 base64 chars -> 4095 total chars (accepted)
    // - exactly 4096 total chars (invalid base64 length) must pass length guard and fail base64 decode
    // - 3055 raw bytes -> 4099 total chars (> 4096, rejected by length guard)
    let mut within_webp = valid_96.clone();
    within_webp.resize(3054, 0);
    let within_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&within_webp));
    assert_eq!(within_uri.chars().count(), 4095);
    validate_avatar_data_uri(&within_uri).expect("4095-char valid WebP avatar URI must pass");

    let exact_4096_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, "A".repeat(4073));
    assert_eq!(exact_4096_uri.chars().count(), MAX_AVATAR_DATA_URI_CHARS);
    let err = validate_avatar_data_uri(&exact_4096_uri).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid base64 in media data URI")),
        "expected 4096-char URI to pass length check and fail base64 decode, got {err:?}"
    );

    let mut large_webp = valid_96;
    large_webp.resize(3055, 0);
    let large_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&large_webp));
    assert!(large_uri.chars().count() > MAX_AVATAR_DATA_URI_CHARS);
    let err = validate_avatar_data_uri(&large_uri).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("exceeds maximum 4096")),
        "expected max length 4096 error, got {err:?}"
    );
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
    assert_eq!(derive_media_blob_id(token).unwrap(), blob_id);

    let (key, nonce) = derive_media_key_and_nonce(token).unwrap();
    assert_eq!(key.len(), 32);
    assert_eq!(nonce.len(), 24);
    assert_ne!(key, [0u8; 32]);
    assert_ne!(nonce, [0u8; 24]);
    assert_eq!(derive_media_key_and_nonce(token).unwrap(), (key, nonce));

    let err = derive_media_blob_id("short").unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid image token")),
        "expected invalid image token error, got {err:?}"
    );
    let err = derive_media_key_and_nonce("short").unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid image token")),
        "expected invalid image token error, got {err:?}"
    );

    let sk = make_key(1);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    let shop_did = create_user_id(&sk.verifying_key(), Some("shop")).unwrap();
    let webp = make_vp8_webp(200, 200);
    let computed_token = compute_image_token(&did, &webp).unwrap();
    assert!(is_valid_image_token(&computed_token));
    assert_eq!(
        compute_image_token(&shop_did, &webp).unwrap(),
        computed_token
    );
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
    let decrypted = open_media_blob(&did, &token, &ciphertext).unwrap();
    assert_eq!(decrypted, webp_bytes);

    // Tampered ciphertext fails AEAD decryption
    let mut tampered = ciphertext.clone();
    let len = tampered.len();
    tampered[len - 1] ^= 0xff;
    let err = open_media_blob(&did, &token, &tampered).unwrap_err();
    assert!(
        matches!(&err, MarketError::Crypto(msg) if msg.contains("Decryption failed")),
        "expected AEAD decryption error, got {err:?}"
    );

    // Wrong owner DID fails content-owner token binding
    let other_did = create_user_id(&make_key(43).verifying_key(), None).unwrap();
    let err = open_media_blob(&other_did, &token, &ciphertext).unwrap_err();
    assert!(
        matches!(&err, MarketError::Crypto(msg) if msg.contains("Media image_token does not match owner DID and content")),
        "expected owner DID mismatch error, got {err:?}"
    );

    // Wrong token fails decryption
    let wrong_token = "00000000000000000000000000000000";
    let err = open_media_blob(&did, wrong_token, &ciphertext).unwrap_err();
    assert!(
        matches!(&err, MarketError::Crypto(msg) if msg.contains("Decryption failed")),
        "expected wrong-token decryption error, got {err:?}"
    );

    // Empty ciphertext fails length guard
    let err = open_media_blob(&did, &token, &[]).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid media ciphertext length: 0")),
        "expected empty ciphertext error, got {err:?}"
    );

    // Exact MAX_AD_MEDIA_BYTES + 64 passes length guard and fails AEAD decryption
    let boundary_ciphertext = vec![0u8; MAX_AD_MEDIA_BYTES + 64];
    let err = open_media_blob(&did, &token, &boundary_ciphertext).unwrap_err();
    assert!(
        matches!(&err, MarketError::Crypto(msg) if msg.contains("Decryption failed")),
        "expected boundary ciphertext to pass length check and fail decryption, got {err:?}"
    );

    // Oversized (MAX_AD_MEDIA_BYTES + 65) fails length guard before decryption
    let oversized = vec![0u8; MAX_AD_MEDIA_BYTES + 65];
    let err = open_media_blob(&did, &token, &oversized).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid media ciphertext length:")),
        "expected oversized ciphertext length error, got {err:?}"
    );
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
        event_at: None,
        event_all_day: false,
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
    let decrypted = open_media_blob(&did, decoded_token2, &ciphertext).unwrap();
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
        event_at: None,
        event_all_day: false,
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

    // Positive baseline: Full wire retains Picture and Ad before FitBytes trims them
    let full_wire = render_wire(&card, 1, WireFilter::Full).unwrap();
    let full_decoded = decoded_items(&verify_sd_card(&full_wire).unwrap());
    assert!(full_decoded
        .iter()
        .any(|(item, _)| matches!(item, CardItem::Picture(_))));
    assert!(full_decoded
        .iter()
        .any(|(item, _)| matches!(item, CardItem::Ad(_))));

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
fn s11_x5_extract_media_bytes_rejects_oversized_base64_before_decode() {
    use humoco_market::MAX_AD_MEDIA_BYTES;

    let max_b64_chars = (MAX_AD_MEDIA_BYTES * 4).div_ceil(3) + 64;
    // Deliberately use invalid base64 characters ('!') beyond max_b64_chars so we prove
    // the length guard rejects with PayloadTooLarge BEFORE invoking the Base64 decoder.
    let oversized_invalid_b64 = "!".repeat(max_b64_chars + 1);
    let oversized_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, oversized_invalid_b64);
    let err = extract_media_bytes(&oversized_uri).unwrap_err();
    assert!(
        matches!(&err, MarketError::PayloadTooLarge(msg) if msg.contains("exceeds maximum")),
        "expected PayloadTooLarge for >max_b64_chars, got {err:?}"
    );

    // Exact boundary (max_b64_chars invalid base64 chars) passes the length check and hits base64 decode
    let boundary_invalid_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, "!".repeat(max_b64_chars));
    let err = extract_media_bytes(&boundary_invalid_uri).unwrap_err();
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains("Invalid base64")),
        "expected exact boundary to pass length guard and fail base64 decode, got {err:?}"
    );

    // Within limit (exact MAX_AD_MEDIA_BYTES encoded) still succeeds cleanly.
    let exact_bytes = vec![0xABu8; MAX_AD_MEDIA_BYTES];
    let exact_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, STANDARD.encode(&exact_bytes));
    assert_eq!(
        extract_media_bytes(&exact_uri).unwrap().len(),
        MAX_AD_MEDIA_BYTES
    );
}
