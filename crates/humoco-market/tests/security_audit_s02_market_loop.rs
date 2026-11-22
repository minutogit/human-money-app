use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ed25519_dalek::SigningKey;
use human_money_core::crypto::{create_user_id, sign_ed25519};
use human_money_core::models::profile::PublicProfile;
use human_money_core::models::voucher::{Voucher, VoucherStandard};
use humoco_market::media::{
    parse_jpeg_dimensions, parse_png_dimensions, parse_webp_dimensions, validate_avatar_data_uri,
    MEDIA_DATA_URI_PREFIX,
};
use humoco_market::sd_card::{
    compute_digest128, is_loopback_http, merge, validate_sd_card, verify_sd_card, CardItem,
    CardJwsHeader, CardPayload, SdCard, SD_CARD_ALG, SD_CARD_TYP,
};
use humoco_market::{
    accept_envelope, build_market_view, derive_effective_hops, encode_qr_svg, forward_envelope,
    mark_card_deleted, render_wire, sign_sd_card, AcceptOutcome, Ad, AdKind, CardEnvelope,
    CardPayloadInput, CardStore, DropDescriptor, EntrySource, MarketError, Skill, StoredCard,
    TrustTier, WireFilter, CARD_SD_PADDED_LEN, MAX_AD_TEXT_CHARS, MAX_AD_TITLE_CHARS,
    MAX_SKILL_TEXT_CHARS, MAX_SKILL_TITLE_CHARS,
};

fn make_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn sign_raw_card_jws(header_json: &str, payload_json: &str, sk: &SigningKey) -> String {
    let header_b64 = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let sig = sign_ed25519(sk, signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig.to_bytes());
    format!("{}.{}.{}", header_b64, payload_b64, sig_b64)
}

fn parse_qr_svg(svg: &str) -> (usize, Vec<Vec<bool>>) {
    let vb_start = svg.find("viewBox=\"0 0 ").expect("viewBox") + "viewBox=\"0 0 ".len();
    let vb_end = svg[vb_start..].find('"').expect("viewBox quote") + vb_start;
    let total_dim: usize = svg[vb_start..vb_end]
        .split_whitespace()
        .next()
        .expect("viewBox width")
        .parse()
        .expect("numeric viewBox");
    let quiet_zone = 4usize;
    let dim = total_dim - 2 * quiet_zone;
    let mut grid = vec![vec![false; dim]; dim];

    let d_start = svg.find(" d=\"").expect("path d") + " d=\"".len();
    let d_end = svg[d_start..].find('"').expect("d quote") + d_start;
    for token in svg[d_start..d_end].split_whitespace() {
        let coords = token
            .strip_prefix('M')
            .and_then(|s| s.strip_suffix("h1v1h-1z"))
            .expect("path token format");
        let (x_str, y_str) = coords.split_once(',').expect("x,y");
        let x: usize = x_str.parse().expect("x");
        let y: usize = y_str.parse().expect("y");
        grid[y - quiet_zone][x - quiet_zone] = true;
    }
    (dim, grid)
}

fn assert_alignment_pattern(grid: &[Vec<bool>], center_r: usize, center_c: usize) {
    for dr in -2isize..=2 {
        for dc in -2isize..=2 {
            let r = (center_r as isize + dr) as usize;
            let c = (center_c as isize + dc) as usize;
            let expected = dr == -2 || dr == 2 || dc == -2 || dc == 2 || (dr == 0 && dc == 0);
            assert_eq!(
                grid[r][c], expected,
                "alignment pattern mismatch at center ({}, {}), offset ({}, {})",
                center_r, center_c, dr, dc
            );
        }
    }
}

#[test]
fn s02_f1_qr_alignment_patterns_present_on_timing_axis_for_v7_to_v40() {
    // v7 (135 B -> dim 45, centers [6, 22, 38])
    let svg_v7 = encode_qr_svg(&[b'A'; 135]).unwrap();
    let (dim_v7, grid_v7) = parse_qr_svg(&svg_v7);
    assert_eq!(dim_v7, 45);
    assert_alignment_pattern(&grid_v7, 6, 22);
    assert_alignment_pattern(&grid_v7, 22, 6);
    assert_alignment_pattern(&grid_v7, 22, 22);
    assert_alignment_pattern(&grid_v7, 22, 38);
    assert_alignment_pattern(&grid_v7, 38, 22);
    assert_alignment_pattern(&grid_v7, 38, 38);

    // v14 (458 B -> dim 73, centers [6, 26, 46, 66])
    let svg_v14 = encode_qr_svg(&[b'B'; 458]).unwrap();
    let (dim_v14, grid_v14) = parse_qr_svg(&svg_v14);
    assert_eq!(dim_v14, 73);
    for &(r, c) in &[(6, 26), (6, 46), (26, 6), (46, 6)] {
        assert_alignment_pattern(&grid_v14, r, c);
    }

    // v40 (2900 B -> dim 177, centers [6, 30, 58, 86, 114, 142, 170])
    let svg_v40 = encode_qr_svg(&[b'C'; 2900]).unwrap();
    let (dim_v40, grid_v40) = parse_qr_svg(&svg_v40);
    assert_eq!(dim_v40, 177);
    for &pos in &[30usize, 58, 86, 114, 142] {
        assert_alignment_pattern(&grid_v40, 6, pos);
        assert_alignment_pattern(&grid_v40, pos, 6);
    }
}

#[test]
fn s02_f2_qr_v34_capacity_and_v35_rollover() {
    // v34 ISO/IEC 18004 ECC-L has 13*115 + 6*116 = 2191 data codewords -> max 2188 byte-mode payload bytes
    let svg_2188 = encode_qr_svg(&[b'A'; 2188]).unwrap();
    let (dim_2188, _) = parse_qr_svg(&svg_2188);
    assert_eq!(dim_2188, 4 * 34 + 17);

    let svg_2189 = encode_qr_svg(&[b'A'; 2189]).unwrap();
    let (dim_2189, _) = parse_qr_svg(&svg_2189);
    assert_eq!(dim_2189, 4 * 35 + 17);

    let svg_2200 = encode_qr_svg(&[b'A'; 2200]).unwrap();
    let (dim_2200, _) = parse_qr_svg(&svg_2200);
    assert_eq!(dim_2200, 4 * 35 + 17);
}

#[test]
fn s02_f6_drop_urls_and_loopback_validation() {
    // 1. is_loopback_http valid cases
    assert!(is_loopback_http("http://localhost/sync"));
    assert!(is_loopback_http("http://localhost:8080/sync"));
    assert!(is_loopback_http("http://127.0.0.1:9000/sync"));
    assert!(is_loopback_http("http://[::1]:8443/sync"));

    // 2. is_loopback_http rejected malicious/malformed cases
    for bad in [
        "http://localhost:0/sync",
        "http://localhost:99999/sync",
        "http://user@localhost/sync",
        "http://localhost/\r\n",
        "http://127.0.0.1\\foo",
        "http://0x7f000001/sync",
    ] {
        assert!(!is_loopback_http(bad), "expected reject for {}", bad);
    }

    // 3. sign_sd_card and verify_sd_card reject malformed DropDescriptor URLs
    let sk = make_key(61);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    let valid_drop_pub = hex::encode(make_key(62).verifying_key().as_bytes());
    let too_long_https = format!("https://example.com/{}", "a".repeat(500));

    for bad_url in [
        "https://",
        "https://user:pass@evil.example/sync.php",
        "https://evil.example:0/sync",
        "https://evil.example\\@127.0.0.1/",
        "https://evil.example/a b",
        "https://evil.example/\r\n",
        too_long_https.as_str(),
    ] {
        let mut payload = CardPayloadInput {
            did: did.clone(),
            v: 100,
            valid_until: 1_800_000_000,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: true,
            drops: vec![DropDescriptor {
                write_url: bad_url.to_string(),
                read_url: "https://sync.example.com/read".to_string(),
                slot_id: None,
                drop_pub: valid_drop_pub.clone(),
            }],
        };
        assert!(
            matches!(
                sign_sd_card(payload.clone(), vec![], &sk),
                Err(MarketError::Validation(_))
            ),
            "sign_sd_card must reject bad write_url '{}'",
            bad_url
        );

        payload.drops[0].write_url = "https://sync.example.com/write".to_string();
        payload.drops[0].read_url = bad_url.to_string();
        assert!(
            matches!(
                sign_sd_card(payload, vec![], &sk),
                Err(MarketError::Validation(_))
            ),
            "sign_sd_card must reject bad read_url '{}'",
            bad_url
        );

        // Also check verify_sd_card on a manually signed JWS containing the bad DropDescriptor
        let bad_payload_struct = CardPayload {
            did: did.clone(),
            recipient_did: None,
            v: 100,
            valid_until: 1_800_000_000,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![DropDescriptor {
                write_url: bad_url.to_string(),
                read_url: "https://sync.example.com/read".to_string(),
                slot_id: None,
                drop_pub: valid_drop_pub.clone(),
            }],
            _sd: (0..CARD_SD_PADDED_LEN)
                .map(|i| URL_SAFE_NO_PAD.encode([i as u8; 16]))
                .collect(),
        };
        let header_json = serde_json::to_string(&CardJwsHeader {
            alg: SD_CARD_ALG.to_string(),
            typ: SD_CARD_TYP.to_string(),
        })
        .unwrap();
        let payload_json = serde_json::to_string(&bad_payload_struct).unwrap();
        let wire = format!("{}~", sign_raw_card_jws(&header_json, &payload_json, &sk));
        assert!(
            verify_sd_card(&wire).is_err(),
            "verify_sd_card must reject bad drop URL '{}'",
            bad_url
        );
    }
}

#[test]
fn s02_f7_sd_card_schema_digests_url_lengths_and_wire_framing() {
    let sk = make_key(71);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();

    // 1. Unknown JSON fields rejected on CardJwsHeader, CardPayload, and CardItem
    assert!(serde_json::from_str::<CardJwsHeader>(
        r#"{"alg":"EdDSA","typ":"humoco-card+sd-jwt","crit":["exp"]}"#
    )
    .is_err());
    assert!(serde_json::from_str::<CardPayload>(
        r#"{"did":"did:key:z1","v":1,"valid_until":2,"max_hops":1,"ad_count":0,"_sd":[],"extra":1}"#
    )
    .is_err());
    assert!(serde_json::from_str::<CardItem>(r#"{"k":"email","v":"a@b.org","extra":1}"#).is_err());
    assert!(serde_json::from_str::<CardItem>(
        r#"{"k":"name","v":{"first_name":"Alice","extra":1}}"#
    )
    .is_err());
    assert!(
        serde_json::from_str::<CardItem>(r#"{"k":"city","v":{"city":"Berlin","extra":1}}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<CardItem>(r#"{"k":"street","v":{"street":"Main","extra":1}}"#)
            .is_err()
    );

    // 2. _sd digests must be strict 22-char base64url decoding to 16 bytes
    let mut bad_sd: Vec<String> = (0..CARD_SD_PADDED_LEN)
        .map(|i| URL_SAFE_NO_PAD.encode([i as u8; 16]))
        .collect();
    bad_sd[0] = "0000_not_16_bytes".to_string(); // invalid length / padding
    bad_sd.sort();
    let bad_sd_payload = CardPayload {
        did: did.clone(),
        recipient_did: None,
        v: 100,
        valid_until: 1_800_000_000,
        max_hops: 2,
        ad_count: 0,
        allow_inquiries: false,
        drops: vec![],
        _sd: bad_sd,
    };
    let hdr = serde_json::to_string(&CardJwsHeader {
        alg: SD_CARD_ALG.to_string(),
        typ: SD_CARD_TYP.to_string(),
    })
    .unwrap();
    let pl = serde_json::to_string(&bad_sd_payload).unwrap();
    let jws_bad_sd = sign_raw_card_jws(&hdr, &pl, &sk);
    let wire_bad_sd = format!("{}~", jws_bad_sd);
    assert!(verify_sd_card(&wire_bad_sd).is_err());
    let parsed_bad_sd = SdCard {
        jws: jws_bad_sd,
        payload: bad_sd_payload,
        disclosures: Default::default(),
    };
    assert!(validate_sd_card(&parsed_bad_sd, wire_bad_sd.len()).is_err());

    // 3. CardItem::Url validation: reject dangerous schemes/userinfo/port0/whitespace, accept valid web URLs
    let base_input = CardPayloadInput {
        did: did.clone(),
        v: 100,
        valid_until: 1_800_000_000,
        max_hops: 2,
        ad_count: 0,
        allow_inquiries: true,
        drops: vec![],
    };
    for bad_profile_url in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "data:text/html,<script>",
        "ftp://example.com",
        "https://user:pass@example.com",
        "https://example.com:0",
        "https://example.com/with space",
        "https://example.com/\n",
    ] {
        assert!(
            sign_sd_card(
                base_input.clone(),
                vec![(CardItem::Url(bad_profile_url.to_string()), 1)],
                &sk
            )
            .is_err(),
            "CardItem::Url must reject '{}'",
            bad_profile_url
        );
    }
    for good_profile_url in [
        "https://example.org/about",
        "http://example.org",
        "www.example.org/shop",
        "example.org:8080/shop",
        "münchen.de/bäckerei",
    ] {
        assert!(
            sign_sd_card(
                base_input.clone(),
                vec![(CardItem::Url(good_profile_url.to_string()), 1)],
                &sk
            )
            .is_ok(),
            "CardItem::Url must accept '{}'",
            good_profile_url
        );
    }

    // 4. Raw length checks reject whitespace-padded strings exceeding field limits
    let pad = " ".repeat(150);
    let padded_items: Vec<CardItem> = vec![
        CardItem::Organization(format!("{}Org{}", pad, pad)),
        CardItem::Community(format!("{}Com{}", pad, pad)),
        CardItem::Street {
            street: format!("{}Main{}", pad, pad),
            house_number: None,
        },
        CardItem::Coordinates(format!("{}52.52,13.40{}", pad, pad)),
        CardItem::Email(format!("{}a@b.org{}", pad, pad)),
        CardItem::Phone(format!("{}+491234{}", pad, pad)),
        CardItem::Url(format!("{}https://example.org{}", pad, pad)),
        CardItem::Gender(format!("{}1{}", pad, pad)),
        CardItem::Skill(Skill {
            title: format!(
                "{}{}{}",
                " ".repeat(MAX_SKILL_TITLE_CHARS),
                "Skill",
                " ".repeat(10)
            ),
            text: None,
            pricing: None,
            is_remote: false,
        }),
        CardItem::Ad(Ad {
            id: "0123456789abcdef".to_string(),
            kind: AdKind::Offer,
            title: format!(
                "{}{}{}",
                " ".repeat(MAX_AD_TITLE_CHARS),
                "Ad",
                " ".repeat(10)
            ),
            text: None,
            pricing: None,
            published_at: 1_700_000_000,
            renewed_at: None,
            expires_at: 1_700_000_000 + 86_400,
            allow_inquiries: false,
            is_remote: false,
            image_token: None,
            event_at: None,
            event_all_day: false,
        }),
    ];
    for item in padded_items {
        assert!(
            sign_sd_card(base_input.clone(), vec![(item.clone(), 1)], &sk).is_err(),
            "raw char limit must reject whitespace-padded item {:?}",
            item
        );
    }

    // 5. Strict SD-JWT wire framing: require trailing '~' and forbid empty interior segments
    let ok_card = sign_sd_card(
        base_input,
        vec![(CardItem::Email("alice@example.org".to_string()), 1)],
        &sk,
    )
    .unwrap();
    let ok_wire = render_wire(&ok_card, 1, WireFilter::Full).unwrap();
    let without_trailing_tilde = ok_wire.strip_suffix('~').unwrap();
    assert!(verify_sd_card(without_trailing_tilde).is_err());
    assert!(verify_sd_card(&ok_card.jws).is_err());
    let disc = ok_card.disclosures.values().next().unwrap();
    assert!(verify_sd_card(&format!("{}~~{}~", ok_card.jws, disc)).is_err());
    assert!(verify_sd_card(&format!("{}~~", ok_card.jws)).is_err());
}

fn build_split_signed_wires(
    sk: &SigningKey,
    v: u64,
    valid_until: u64,
    groups: &[Vec<(CardItem, u8)>],
) -> Vec<String> {
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    let mut sd_digests = Vec::new();
    let mut raw_groups: Vec<Vec<String>> = Vec::with_capacity(groups.len());
    let mut counter = 1u8;

    for group in groups {
        let mut raw_list = Vec::with_capacity(group.len());
        for (item, dist) in group {
            let salt_b64 = URL_SAFE_NO_PAD.encode([counter; 16]);
            counter = counter.wrapping_add(1);
            let tuple_json = serde_json::to_string(&(&salt_b64, dist, item)).unwrap();
            let raw_b64 = URL_SAFE_NO_PAD.encode(tuple_json.as_bytes());
            sd_digests.push(compute_digest128(&raw_b64));
            raw_list.push(raw_b64);
        }
        raw_groups.push(raw_list);
    }

    let mut decoys = 100u8;
    while sd_digests.len() < CARD_SD_PADDED_LEN {
        sd_digests.push(URL_SAFE_NO_PAD.encode([decoys; 16]));
        decoys = decoys.wrapping_add(1);
    }
    sd_digests.sort();

    let payload = CardPayload {
        did,
        recipient_did: None,
        v,
        valid_until,
        max_hops: 2,
        ad_count: 0,
        allow_inquiries: false,
        drops: vec![],
        _sd: sd_digests,
    };
    let header_json = serde_json::to_string(&CardJwsHeader {
        alg: SD_CARD_ALG.to_string(),
        typ: SD_CARD_TYP.to_string(),
    })
    .unwrap();
    let payload_json = serde_json::to_string(&payload).unwrap();
    let jws = sign_raw_card_jws(&header_json, &payload_json, sk);

    raw_groups
        .into_iter()
        .map(|discs| {
            if discs.is_empty() {
                format!("{}~", jws)
            } else {
                format!("{}~{}~", jws, discs.join("~"))
            }
        })
        .collect()
}

fn test_ad(id: &str, title: &str, now: u64) -> Ad {
    Ad {
        id: id.to_string(),
        kind: AdKind::Offer,
        title: title.to_string(),
        text: None,
        pricing: None,
        published_at: now - 10,
        renewed_at: None,
        expires_at: now + 86_400,
        allow_inquiries: false,
        is_remote: false,
        image_token: None,
        event_at: None,
        event_all_day: false,
    }
}

#[test]
fn s02_f3_merge_enforces_card_invariants_and_wire_cap() {
    let now = 1_700_000_000u64;
    let valid_until = now + 30 * 86_400;
    let sk = make_key(81);

    // Case (a): 11 distinct Ads split 6 + 5 across two valid partial wires
    let ads_a: Vec<(CardItem, u8)> = (0..6)
        .map(|i| {
            (
                CardItem::Ad(test_ad(&format!("{:016x}", i + 1), "AdA", now)),
                2,
            )
        })
        .collect();
    let ads_b: Vec<(CardItem, u8)> = (6..11)
        .map(|i| {
            (
                CardItem::Ad(test_ad(&format!("{:016x}", i + 1), "AdB", now)),
                2,
            )
        })
        .collect();

    // Case (b): 9 Skills split 5 + 4
    let skills_a: Vec<(CardItem, u8)> = (0..5)
        .map(|i| {
            (
                CardItem::Skill(Skill {
                    title: format!("Skill {}", i),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            )
        })
        .collect();
    let skills_b: Vec<(CardItem, u8)> = (5..9)
        .map(|i| {
            (
                CardItem::Skill(Skill {
                    title: format!("Skill {}", i),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            )
        })
        .collect();

    // Case (c): 2 conflicting Name singletons split 1 + 1
    let name_a = vec![(
        CardItem::Name {
            first_name: Some("Alice".to_string()),
            last_name: None,
        },
        2,
    )];
    let name_b = vec![(
        CardItem::Name {
            first_name: Some("Bob".to_string()),
            last_name: None,
        },
        2,
    )];

    // Case (d): 2 Ads sharing the same ad.id split 1 + 1
    let dup_ad_a = vec![(CardItem::Ad(test_ad("aaaaaaaaaaaaaaaa", "First", now)), 2)];
    let dup_ad_b = vec![(CardItem::Ad(test_ad("aaaaaaaaaaaaaaaa", "Second", now)), 2)];

    // Case (e): > MAX_CARD_WIRE_BYTES (16,384) combined wire across 2 wires <= 16,384 B each
    let mut big_webp = vec![
        b'R', b'I', b'F', b'F', 22, 0, 0, 0, b'W', b'E', b'B', b'P', b'V', b'P', b'8', b' ', 10, 0,
        0, 0, 0x00, 0x00, 0x00, 0x9d, 0x01, 0x2a, 96, 0, 96, 0,
    ];
    big_webp.resize(2900, 0);
    let pic_uri = format!(
        "data:image/webp;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&big_webp)
    );
    assert!(pic_uri.len() <= 4096);
    let mut big_group_1 = vec![
        (CardItem::Picture(pic_uri), 2),
        (
            CardItem::Name {
                first_name: Some("A".repeat(80)),
                last_name: Some("B".repeat(80)),
            },
            2,
        ),
        (CardItem::Organization("O".repeat(100)), 2),
        (CardItem::Community("C".repeat(100)), 2),
        (
            CardItem::City {
                zip_code: Some("1".repeat(20)),
                city: Some("Y".repeat(80)),
                country: Some("Z".repeat(80)),
            },
            2,
        ),
        (
            CardItem::Street {
                street: "S".repeat(100),
                house_number: Some("9".repeat(20)),
            },
            2,
        ),
    ];
    for i in 0..4 {
        big_group_1.push((
            CardItem::Skill(Skill {
                title: format!("Skill{} {}", i, "K".repeat(MAX_SKILL_TITLE_CHARS - 8)),
                text: Some("T".repeat(MAX_SKILL_TEXT_CHARS)),
                pricing: Some("P".repeat(40)),
                is_remote: false,
            }),
            2,
        ));
    }
    for i in 0..5 {
        let mut ad = test_ad(
            &format!("{:016x}", i + 10),
            &"A".repeat(MAX_AD_TITLE_CHARS),
            now,
        );
        ad.text = Some("D".repeat(MAX_AD_TEXT_CHARS));
        ad.pricing = Some("P".repeat(40));
        big_group_1.push((CardItem::Ad(ad), 2));
    }
    let mut big_group_2 = vec![
        (CardItem::Email("E".repeat(100)), 2),
        (
            CardItem::Url(format!("https://example.org/{}", "u".repeat(180))),
            2,
        ),
    ];
    for i in 4..8 {
        big_group_2.push((
            CardItem::Skill(Skill {
                title: format!("Skill{} {}", i, "K".repeat(MAX_SKILL_TITLE_CHARS - 8)),
                text: Some("T".repeat(MAX_SKILL_TEXT_CHARS)),
                pricing: Some("P".repeat(40)),
                is_remote: false,
            }),
            2,
        ));
    }
    for i in 5..10 {
        let mut ad = test_ad(
            &format!("{:016x}", i + 10),
            &"A".repeat(MAX_AD_TITLE_CHARS),
            now,
        );
        ad.text = Some("D".repeat(MAX_AD_TEXT_CHARS));
        ad.pricing = Some("P".repeat(40));
        big_group_2.push((CardItem::Ad(ad), 2));
    }

    for (label, g1, g2) in [
        ("11-ads", ads_a, ads_b),
        ("9-skills", skills_a, skills_b),
        ("dup-name", name_a, name_b),
        ("dup-ad-id", dup_ad_a, dup_ad_b),
        ("wire-cap", big_group_1, big_group_2),
    ] {
        let wires = build_split_signed_wires(&sk, 100, valid_until, &[g1, g2]);
        let mut c1 = verify_sd_card(&wires[0]).expect(label);
        let c1_snapshot = c1.clone();
        let c2 = verify_sd_card(&wires[1]).expect(label);

        assert!(
            !merge(&mut c1, c2),
            "merge must fail on invariant violation: {}",
            label
        );
        assert_eq!(
            c1, c1_snapshot,
            "failed merge mutated stored card: {}",
            label
        );

        // Same check through CardStore::accept_envelope
        let mut store = CardStore::default();
        assert!(matches!(
            accept_envelope(
                &mut store,
                &CardEnvelope {
                    card_wire: wires[0].clone(),
                    hops: 1
                },
                now,
                100,
                None,
                &|_| false
            ),
            AcceptOutcome::Accepted { .. }
        ));
        let card_key = c1_snapshot.payload.recipient_id().to_string();
        let stored_before = store.cards.get(&card_key).unwrap().clone();
        let out2 = accept_envelope(
            &mut store,
            &CardEnvelope {
                card_wire: wires[1].clone(),
                hops: 1,
            },
            now,
            100,
            None,
            &|_| false,
        );
        assert!(
            matches!(out2, AcceptOutcome::Invalid(_)),
            "accept_envelope must return Invalid on merged violation ({}), got {:?}",
            label,
            out2
        );
        assert_eq!(store.cards.get(&card_key).unwrap(), &stored_before);
    }

    // Recovery of pre-existing corrupted hops == 0 during valid same-JWS merge
    let valid_wires = build_split_signed_wires(
        &sk,
        200,
        valid_until,
        &[
            vec![(
                CardItem::Name {
                    first_name: Some("Alice".to_string()),
                    last_name: None,
                },
                2,
            )],
            vec![(CardItem::Email("alice@example.org".to_string()), 2)],
        ],
    );
    let mut store = CardStore::default();
    let card0 = verify_sd_card(&valid_wires[0]).unwrap();
    let key = card0.payload.recipient_id().to_string();
    store.cards.insert(
        key.clone(),
        StoredCard {
            card: card0,
            card_wire: valid_wires[0].clone(),
            hops: 0, // pre-existing corruption
            received_at: now,
        },
    );
    assert!(matches!(
        accept_envelope(
            &mut store,
            &CardEnvelope {
                card_wire: valid_wires[1].clone(),
                hops: 2
            },
            now,
            100,
            None,
            &|_| false
        ),
        AcceptOutcome::Accepted { .. }
    ));
    assert_eq!(store.cards.get(&key).unwrap().hops, 2);
}

#[test]
fn s02_f4_evicted_incoming_card_returns_ignored_stale_and_preserves_tombstone() {
    let now = 1_700_000_000u64;
    let valid_until = now + 30 * 86_400;

    // 1. Global max_stored_cards eviction preserves incoming card's deletion tombstone
    let mut store = CardStore::default();
    for seed in [91u8, 92u8] {
        let sk = make_key(seed);
        let did = create_user_id(&sk.verifying_key(), None).unwrap();
        let card = sign_sd_card(
            CardPayloadInput {
                did,
                v: 100,
                valid_until,
                max_hops: 2,
                ad_count: 0,
                allow_inquiries: false,
                drops: vec![],
            },
            vec![(
                CardItem::Name {
                    first_name: Some("Protected".to_string()),
                    last_name: None,
                },
                1,
            )],
            &sk,
        )
        .unwrap();
        let wire = render_wire(&card, 1, WireFilter::Full).unwrap();
        assert!(matches!(
            accept_envelope(
                &mut store,
                &CardEnvelope {
                    card_wire: wire,
                    hops: 1
                },
                now,
                2,
                None,
                &|_| false
            ),
            AcceptOutcome::Accepted { .. }
        ));
    }

    let sk_c = make_key(93);
    let did_c = create_user_id(&sk_c.verifying_key(), None).unwrap();
    mark_card_deleted(&mut store, &did_c, 50);
    let card_c = sign_sd_card(
        CardPayloadInput {
            did: did_c.clone(),
            v: 60,
            valid_until,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![],
        },
        vec![(
            CardItem::Name {
                first_name: Some("Evicted".to_string()),
                last_name: None,
            },
            2,
        )],
        &sk_c,
    )
    .unwrap();
    let wire_c = render_wire(&card_c, 2, WireFilter::Full).unwrap();
    let out_global = accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire_c,
            hops: 2,
        },
        now,
        2,
        None,
        &|_| false,
    );
    assert_eq!(out_global, AcceptOutcome::IgnoredStale);
    assert!(!store.cards.contains_key(&did_c));
    assert_eq!(store.deleted_card_versions.get(&did_c), Some(&50));

    // 2. Per-pubkey MAX_CARDS_PER_PUBKEY eviction preserves incoming card's deletion tombstone
    let mut pk_store = CardStore::default();
    let sk_multi = make_key(94);
    let vk_multi = sk_multi.verifying_key();
    for (idx, pfx) in ["one", "two", "three"].iter().enumerate() {
        let did = create_user_id(&vk_multi, Some(pfx)).unwrap();
        let card = sign_sd_card(
            CardPayloadInput {
                did,
                v: 100 + idx as u64 * 10,
                valid_until,
                max_hops: 2,
                ad_count: 0,
                allow_inquiries: false,
                drops: vec![],
            },
            vec![(
                CardItem::Name {
                    first_name: Some((*pfx).to_string()),
                    last_name: None,
                },
                1,
            )],
            &sk_multi,
        )
        .unwrap();
        let wire = render_wire(&card, 1, WireFilter::Full).unwrap();
        assert!(matches!(
            accept_envelope(
                &mut pk_store,
                &CardEnvelope {
                    card_wire: wire,
                    hops: 1
                },
                now,
                100,
                None,
                &|_| false
            ),
            AcceptOutcome::Accepted { .. }
        ));
    }

    let did_d4 = create_user_id(&vk_multi, Some("four")).unwrap();
    mark_card_deleted(&mut pk_store, &did_d4, 50);
    let card_d4 = sign_sd_card(
        CardPayloadInput {
            did: did_d4.clone(),
            v: 55,
            valid_until,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![],
        },
        vec![(
            CardItem::Name {
                first_name: Some("Four".to_string()),
                last_name: None,
            },
            1,
        )],
        &sk_multi,
    )
    .unwrap();
    let wire_d4 = render_wire(&card_d4, 1, WireFilter::Full).unwrap();
    let out_pubkey = accept_envelope(
        &mut pk_store,
        &CardEnvelope {
            card_wire: wire_d4,
            hops: 1,
        },
        now,
        100,
        None,
        &|_| false,
    );
    assert_eq!(out_pubkey, AcceptOutcome::IgnoredStale);
    assert!(!pk_store.cards.contains_key(&did_d4));
    assert_eq!(pk_store.deleted_card_versions.get(&did_d4), Some(&50));
}

#[test]
fn s02_f5_forward_envelope_hop_bounds_and_subaccount_writer_matching() {
    let sk = make_key(95);
    let vk = sk.verifying_key();
    let root_did = create_user_id(&vk, None).unwrap();
    let prefixed_did = create_user_id(&vk, Some("laden")).unwrap();
    let other_did = create_user_id(&make_key(96).verifying_key(), None).unwrap();

    let card = sign_sd_card(
        CardPayloadInput {
            did: root_did.clone(),
            v: 100,
            valid_until: 1_800_000_000,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![],
        },
        vec![(
            CardItem::Name {
                first_name: Some("Alice".to_string()),
                last_name: None,
            },
            2,
        )],
        &sk,
    )
    .unwrap();
    let wire = render_wire(&card, 1, WireFilter::Full).unwrap();

    // forward_envelope must return None (without panicking) on hops == u8::MAX and hops == 0
    let stored_max = StoredCard {
        card: card.clone(),
        card_wire: wire.clone(),
        hops: u8::MAX,
        received_at: 1_700_000_000,
    };
    assert!(forward_envelope(&stored_max).is_none());

    let stored_zero = StoredCard {
        card,
        card_wire: wire,
        hops: 0,
        received_at: 1_700_000_000,
    };
    assert!(forward_envelope(&stored_zero).is_none());

    // derive_effective_hops treats same-keypair root & subaccount DIDs as same author (1)
    assert_eq!(derive_effective_hops(2, &root_did, Some(&prefixed_did)), 1);
    assert_eq!(derive_effective_hops(2, &prefixed_did, Some(&root_did)), 1);
    assert_eq!(derive_effective_hops(1, &root_did, Some(&other_did)), 2);
}

#[test]
fn s02_f9_market_view_filters_expired_cards_and_respects_newer_tombstone() {
    let now = 1_700_000_000u64;
    let sk = make_key(97);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();

    // 1. Expired card ignored when no voucher exists
    let expired_card = sign_sd_card(
        CardPayloadInput {
            did: did.clone(),
            v: 50,
            valid_until: now,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![],
        },
        vec![(
            CardItem::Name {
                first_name: Some("Expired".to_string()),
                last_name: None,
            },
            2,
        )],
        &sk,
    )
    .unwrap();
    let expired_stored = StoredCard {
        card_wire: render_wire(&expired_card, 1, WireFilter::Full).unwrap(),
        card: expired_card,
        hops: 1,
        received_at: now - 100,
    };
    assert!(build_market_view(
        &[],
        &[expired_stored],
        "did:key:other",
        &|_| TrustTier::Trusted,
        now
    )
    .is_empty());

    // 2. Active v1 + Tombstone v2 in either slice order -> card completely suppressed
    let active_v1 = sign_sd_card(
        CardPayloadInput {
            did: did.clone(),
            v: 100,
            valid_until: now + 30 * 86_400,
            max_hops: 2,
            ad_count: 1,
            allow_inquiries: true,
            drops: vec![],
        },
        vec![
            (
                CardItem::Name {
                    first_name: Some("Alice".to_string()),
                    last_name: None,
                },
                2,
            ),
            (
                CardItem::Skill(Skill {
                    title: "Revoked Skill".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            ),
            (
                CardItem::Ad(test_ad("0000000000000022", "Revoked Ad", now)),
                2,
            ),
        ],
        &sk,
    )
    .unwrap();
    let active_stored = StoredCard {
        card_wire: render_wire(&active_v1, 1, WireFilter::Full).unwrap(),
        card: active_v1,
        hops: 1,
        received_at: now - 10,
    };

    let tomb_v2 = sign_sd_card(
        CardPayloadInput {
            did: did.clone(),
            v: 200,
            valid_until: now + 30 * 86_400,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![],
        },
        vec![],
        &sk,
    )
    .unwrap();
    let tomb_stored = StoredCard {
        card_wire: format!("{}~", tomb_v2.jws),
        card: tomb_v2,
        hops: 1,
        received_at: now,
    };

    for cards in [
        [active_stored.clone(), tomb_stored.clone()],
        [tomb_stored, active_stored],
    ] {
        assert!(
            build_market_view(&[], &cards, "did:key:other", &|_| TrustTier::Trusted, now)
                .is_empty()
        );

        let voucher = Voucher {
            voucher_id: "v-1".to_string(),
            creation_date: "2026-01-01T10:00:00Z".to_string(),
            non_redeemable_test_voucher: false,
            voucher_standard: VoucherStandard {
                name: "Minuto".to_string(),
                uuid: "std-1".to_string(),
                standard_definition_hash: "h".to_string(),
            },
            creator_profile: PublicProfile {
                id: Some(did.clone()),
                service_offer: Some("Voucher Service".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let persons = build_market_view(
            &[voucher],
            &cards,
            "did:key:other",
            &|_| TrustTier::Trusted,
            now,
        );
        assert_eq!(persons.len(), 1);
        assert_eq!(persons[0].sources, vec![EntrySource::Voucher]);
        assert!(persons[0].active_ads.is_empty());
        assert_eq!(persons[0].skills.len(), 1);
        assert_eq!(persons[0].skills[0].title, "Voucher Service");
    }
}

fn make_jpeg_with_sof(marker: u8, width: u16, height: u16) -> Vec<u8> {
    let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
    b.extend_from_slice(&[0xFF, marker, 0x00, 0x0B, 0x08]);
    b.extend_from_slice(&height.to_be_bytes());
    b.extend_from_slice(&width.to_be_bytes());
    b.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
    b.extend_from_slice(&[0xFF, 0xD9]);
    b
}

fn make_vp8_chunk(width: u16, height: u16) -> Vec<u8> {
    let mut p = vec![0x00, 0x00, 0x00, 0x9d, 0x01, 0x2a];
    p.extend_from_slice(&(width & 0x3fff).to_le_bytes());
    p.extend_from_slice(&(height & 0x3fff).to_le_bytes());
    p
}

fn make_vp8l_chunk(width: u16, height: u16) -> Vec<u8> {
    let mut p = vec![0x2f];
    let bits = ((u32::from(width.saturating_sub(1))) & 0x3fff)
        | (((u32::from(height.saturating_sub(1))) & 0x3fff) << 14);
    p.extend_from_slice(&bits.to_le_bytes());
    p
}

fn wrap_riff_webp(
    fourcc: &[u8; 4],
    chunk_size: u32,
    payload: &[u8],
    extra_subchunks: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&chunk_size.to_le_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(extra_subchunks);
    out
}

fn make_subchunk(fourcc: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
    out
}

#[test]
fn s02_f8_jpeg_webp_png_parser_and_avatar_uri_hardening() {
    // 1. JPEG parser strict validation
    assert_eq!(
        parse_jpeg_dimensions(&make_jpeg_with_sof(0xC0, 96, 96)).unwrap(),
        (96, 96)
    );
    assert_eq!(
        parse_jpeg_dimensions(&make_jpeg_with_sof(0xC2, 320, 240)).unwrap(),
        (320, 240)
    );

    // Short SOF (length=2, length=7, mismatched ncomp=2 with length=11)
    let short_sof_len2 = vec![
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x02, 0x08, 0x00, 0x60, 0x00, 0x60,
    ];
    assert!(parse_jpeg_dimensions(&short_sof_len2).is_err());

    let short_sof_len7 = vec![
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x07, 0x08, 0x00, 0x60, 0x00, 0x60,
    ];
    assert!(parse_jpeg_dimensions(&short_sof_len7).is_err());

    let mut bad_ncomp = make_jpeg_with_sof(0xC0, 96, 96);
    bad_ncomp[17] = 2; // length=11 expects ncomp=1, not 2
    assert!(parse_jpeg_dimensions(&bad_ncomp).is_err());

    // Truncated SOF (pos + length > len)
    let trunc_sof = vec![
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x20, 0x08, 0x00, 0x60, 0x00, 0x60, 0x01, 0x01, 0x11, 0x00,
    ];
    assert!(parse_jpeg_dimensions(&trunc_sof).is_err());

    // Non-0xFF header garbage and 0xFF 0x00 in header
    let garbage_hdr = vec![
        0xFF, 0xD8, 0x41, 0x42, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x60, 0x00, 0x60, 0x01, 0x01,
        0x11, 0x00, 0xFF, 0xD9,
    ];
    assert!(parse_jpeg_dimensions(&garbage_hdr).is_err());

    let ff00_hdr = vec![
        0xFF, 0xD8, 0xFF, 0x00, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x60, 0x00, 0x60, 0x01, 0x01,
        0x11, 0x00, 0xFF, 0xD9,
    ];
    assert!(parse_jpeg_dimensions(&ff00_hdr).is_err());

    // Duplicate SOF
    let mut dup_sof = vec![0xFF, 0xD8];
    dup_sof.extend_from_slice(&[
        0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x60, 0x00, 0x60, 0x01, 0x01, 0x11, 0x00,
    ]);
    dup_sof.extend_from_slice(&[
        0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x08, 0x00, 0x08, 0x00, 0x01, 0x01, 0x11, 0x00,
    ]);
    dup_sof.extend_from_slice(&[0xFF, 0xD9]);
    assert!(parse_jpeg_dimensions(&dup_sof).is_err());

    // DHP (0xDE), DNL (0xDC), nested SOI (0xD8)
    for bad_marker in [0xDEu8, 0xDCu8] {
        let mut with_bad = vec![0xFF, 0xD8, 0xFF, bad_marker, 0x00, 0x04, 0x00, 0x01];
        with_bad.extend_from_slice(&[
            0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x60, 0x00, 0x60, 0x01, 0x01, 0x11, 0x00, 0xFF,
            0xD9,
        ]);
        assert!(parse_jpeg_dimensions(&with_bad).is_err());
    }
    let nested_soi = vec![
        0xFF, 0xD8, 0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x60, 0x00, 0x60, 0x01, 0x01,
        0x11, 0x00, 0xFF, 0xD9,
    ];
    assert!(parse_jpeg_dimensions(&nested_soi).is_err());

    // 2. WebP parser strict chunk sizes and VP8X sub-chunk validation
    let bad_vp8_size = wrap_riff_webp(b"VP8 ", 9, &make_vp8_chunk(96, 96), &[]);
    assert!(parse_webp_dimensions(&bad_vp8_size).is_err());

    let bad_vp8l_size = wrap_riff_webp(b"VP8L", 4, &make_vp8l_chunk(96, 96), &[]);
    assert!(parse_webp_dimensions(&bad_vp8l_size).is_err());

    let mut vp8x_payload_96 = vec![0u8; 4]; // flags
    vp8x_payload_96.extend_from_slice(&[95, 0, 0, 95, 0, 0]);
    let bad_vp8x_size = wrap_riff_webp(b"VP8X", 12, &vp8x_payload_96, &[]);
    assert!(parse_webp_dimensions(&bad_vp8x_size).is_err());

    // VP8X (96x96) containing oversized VP8, VP8L, or ANMF sub-chunk
    let sub_big_vp8 = make_subchunk(b"VP8 ", &make_vp8_chunk(2048, 2048));
    let bomb_vp8x_vp8 = wrap_riff_webp(b"VP8X", 10, &vp8x_payload_96, &sub_big_vp8);
    assert!(parse_webp_dimensions(&bomb_vp8x_vp8).is_err());

    let sub_big_vp8l = make_subchunk(b"VP8L", &make_vp8l_chunk(2048, 2048));
    let bomb_vp8x_vp8l = wrap_riff_webp(b"VP8X", 10, &vp8x_payload_96, &sub_big_vp8l);
    assert!(parse_webp_dimensions(&bomb_vp8x_vp8l).is_err());

    let mut anmf_big = vec![0u8; 16];
    anmf_big[6..9].copy_from_slice(&2047u32.to_le_bytes()[..3]);
    anmf_big[9..12].copy_from_slice(&2047u32.to_le_bytes()[..3]);
    let sub_big_anmf = make_subchunk(b"ANMF", &anmf_big);
    let bomb_vp8x_anmf = wrap_riff_webp(b"VP8X", 10, &vp8x_payload_96, &sub_big_anmf);
    assert!(parse_webp_dimensions(&bomb_vp8x_anmf).is_err());

    // Truncated VP8X sub-chunk
    let mut trunc_sub = make_subchunk(b"ALPH", &[1, 2, 3, 4]);
    trunc_sub.pop();
    let bad_trunc_vp8x = wrap_riff_webp(b"VP8X", 10, &vp8x_payload_96, &trunc_sub);
    assert!(parse_webp_dimensions(&bad_trunc_vp8x).is_err());

    // Valid VP8X with ALPH (odd-sized -> padded) + VP8L + ANMF within 96x96 canvas
    let mut valid_subs = make_subchunk(b"ALPH", &[0u8; 3]);
    valid_subs.extend_from_slice(&make_subchunk(b"VP8L", &make_vp8l_chunk(96, 96)));
    let mut anmf_ok = vec![0u8; 16];
    anmf_ok[6..9].copy_from_slice(&95u32.to_le_bytes()[..3]);
    anmf_ok[9..12].copy_from_slice(&95u32.to_le_bytes()[..3]);
    valid_subs.extend_from_slice(&make_subchunk(b"ANMF", &anmf_ok));
    let valid_vp8x = wrap_riff_webp(b"VP8X", 10, &vp8x_payload_96, &valid_subs);
    assert_eq!(parse_webp_dimensions(&valid_vp8x).unwrap(), (96, 96));

    // 3. PNG parser strict IHDR length (13) and minimum 33-byte length
    let mut valid_png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    valid_png.extend_from_slice(&13u32.to_be_bytes());
    valid_png.extend_from_slice(b"IHDR");
    valid_png.extend_from_slice(&96u32.to_be_bytes());
    valid_png.extend_from_slice(&96u32.to_be_bytes());
    valid_png.extend_from_slice(&[8, 6, 0, 0, 0]);
    valid_png.extend_from_slice(&[0u8; 4]);
    assert_eq!(valid_png.len(), 33);
    assert_eq!(parse_png_dimensions(&valid_png).unwrap(), (96, 96));

    assert!(parse_png_dimensions(&valid_png[..28]).is_err());
    let mut bad_ihdr_len = valid_png.clone();
    bad_ihdr_len[8..12].copy_from_slice(&12u32.to_be_bytes());
    assert!(parse_png_dimensions(&bad_ihdr_len).is_err());

    // 4. Avatar data URI forbids leading/internal/trailing whitespace
    let good_webp = wrap_riff_webp(b"VP8 ", 10, &make_vp8_chunk(96, 96), &[]);
    let b64 = STANDARD.encode(&good_webp);
    let good_uri = format!("{}{}", MEDIA_DATA_URI_PREFIX, b64);
    assert!(validate_avatar_data_uri(&good_uri).is_ok());
    assert!(validate_avatar_data_uri(&format!(" {}", good_uri)).is_err());
    assert!(validate_avatar_data_uri(&format!("{} {}", MEDIA_DATA_URI_PREFIX, b64)).is_err());
    assert!(validate_avatar_data_uri(&format!("{}\n", good_uri)).is_err());
}
