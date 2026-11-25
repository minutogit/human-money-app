use super::http::*;
use super::lock_api::{next_pow_start_nonce, verify_ed25519_hex_strict};
use super::ssrf::*;
use super::*;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use human_money_core::humoco_proto::{
    causal::{LockEntry, LOCK_ENTRY_WIRE_LEN},
    crypto::{MAX_LOCK_POW_BITS},
    decode_hex_fixed, GenesisTicket,
};
use human_money_core::models::layer2_api::{SyncLockBatchRequest, SyncLockBatchResponse};
use humoco_market::SignedDropHeaders;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

fn parse_http_headers<'a>(lines: impl Iterator<Item = &'a str>) -> HashMap<String, String> {
    lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect()
}

async fn read_request_non_empty(stream: &mut tokio::net::TcpStream) -> String {
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).await.unwrap();
    assert!(n > 0, "expected non-empty HTTP request from client");
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

async fn write_allow_early_close(stream: &mut tokio::net::TcpStream, data: &[u8]) -> bool {
    use tokio::io::AsyncWriteExt;
    match stream.write_all(data).await {
        Ok(()) => true,
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
            ) =>
        {
            false
        }
        Err(e) => panic!("unexpected mock server write failure: {}", e),
    }
}

fn assert_local_validation_contains<T: std::fmt::Debug>(
    res: Result<T, SyncClientError>,
    expected: &str,
    ctx: &str,
) {
    let err = res.expect_err(ctx);
    assert!(
        matches!(err, SyncClientError::LocalValidation(ref m) if m.contains(expected)),
        "{}: expected LocalValidation containing '{}', got {:?}",
        ctx,
        expected,
        err
    );
}

fn assert_invalid_payload_contains<T: std::fmt::Debug>(
    res: Result<T, SyncClientError>,
    expected: &str,
    ctx: &str,
) {
    let err = res.expect_err(ctx);
    assert!(
        matches!(err, SyncClientError::InvalidPayload(ref m) if m.contains(expected)),
        "{}: expected InvalidPayload containing '{}', got {:?}",
        ctx,
        expected,
        err
    );
}

#[test]
fn test_parse_retry_after_header() {
    let mut headers = reqwest::header::HeaderMap::new();
    // Missing header -> 60s fallback
    assert_eq!(parse_retry_after(&headers), 60);

    // Valid integer seconds
    headers.insert(reqwest::header::RETRY_AFTER, "45".parse().unwrap());
    assert_eq!(parse_retry_after(&headers), 45);

    // Invalid string -> 60s fallback
    headers.insert(
        reqwest::header::RETRY_AFTER,
        "invalid_seconds".parse().unwrap(),
    );
    assert_eq!(parse_retry_after(&headers), 60);
}

#[test]
fn test_sync_client_error_helpers_and_display() {
    let rate_err = SyncClientError::RateLimited {
        status: 429,
        retry_after_secs: 45,
        context: "by sync server".to_string(),
        snippet: String::new(),
    };
    assert_eq!(rate_err.retry_after_secs(), Some(45));
    assert_eq!(rate_err.http_status(), Some(429));
    assert!(rate_err.is_rate_limited());
    assert!(!rate_err.is_local_validation());
    assert_eq!(
        rate_err.to_string(),
        "Rate limited by sync server (HTTP 429). Retry-After: 45s"
    );

    let clamped_zero = SyncClientError::RateLimited {
        status: 429,
        retry_after_secs: 0,
        context: "fetching index".to_string(),
        snippet: String::new(),
    };
    assert_eq!(clamped_zero.retry_after_secs(), Some(1));
    assert_eq!(
        clamped_zero.to_string(),
        "Rate limited fetching index (HTTP 429). Retry-After: 1s"
    );

    let clamped_max = SyncClientError::RateLimited {
        status: 429,
        retry_after_secs: 999_999,
        context: "fetching index".to_string(),
        snippet: String::new(),
    };
    assert_eq!(clamped_max.retry_after_secs(), Some(3600));

    let http_500 = SyncClientError::HttpStatus {
        status: 500,
        retry_after_secs: None,
        context: "fetching index".to_string(),
        snippet: "Retry-After: 9999 HTTP 429".to_string(),
    };
    assert_eq!(http_500.retry_after_secs(), None);
    assert_eq!(http_500.http_status(), Some(500));
    assert!(!http_500.is_rate_limited());
    assert!(!http_500.is_local_validation());

    let local_err = SyncClientError::LocalValidation("bad input".to_string());
    assert!(local_err.is_local_validation());
    assert!(!local_err.is_rate_limited());
    assert_eq!(local_err.http_status(), None);
    assert_eq!(local_err.retry_after_secs(), None);

    let transport_err = SyncClientError::Transport("connection refused".to_string());
    assert!(!transport_err.is_local_validation());
    assert_eq!(transport_err.retry_after_secs(), None);
    assert_eq!(transport_err.http_status(), None);
}

#[tokio::test]
async fn test_get_media_blob_id_validation() {
    let client = create_sync_http_client();
    // Invalid length
    let err_short = get_media_blob(&client, "https://example.com/sync", "abc")
        .await
        .unwrap_err();
    assert!(err_short.is_local_validation());
    assert!(err_short.to_string().contains("Invalid blob_id"));

    // Invalid characters (uppercase or non-hex)
    let err_chars = get_media_blob(&client, "https://example.com/sync", &"G".repeat(64))
        .await
        .unwrap_err();
    assert!(err_chars.is_local_validation());
    assert!(err_chars.to_string().contains("Invalid blob_id"));
}

#[test]
fn test_sync_index_entry_serde_prefix() {
    // Without prefix
    let entry_no_prefix = SyncIndexEntry {
        id: "abc".to_string(),
        prefix: None,
        channel: "card".to_string(),
        mtime: 1234,
        size: 56,
        etag: Some("tag1".to_string()),
    };
    let json_no_pfx = serde_json::to_string(&entry_no_prefix).unwrap();
    assert!(!json_no_pfx.contains("\"prefix\""));
    let de_no_pfx: SyncIndexEntry = serde_json::from_str(&json_no_pfx).unwrap();
    assert_eq!(entry_no_prefix, de_no_pfx);

    // With prefix
    let entry_with_prefix = SyncIndexEntry {
        id: "abc".to_string(),
        prefix: Some("alice".to_string()),
        channel: "card".to_string(),
        mtime: 1234,
        size: 56,
        etag: None,
    };
    let json_with_pfx = serde_json::to_string(&entry_with_prefix).unwrap();
    assert!(json_with_pfx.contains("\"prefix\":\"alice\""));
    let de_with_pfx: SyncIndexEntry = serde_json::from_str(&json_with_pfx).unwrap();
    assert_eq!(entry_with_prefix, de_with_pfx);

    // Deserializing JSON without prefix field defaults to None
    let raw_json = r#"{"id":"pub123","channel":"post","mtime":999,"size":100}"#;
    let de_raw: SyncIndexEntry = serde_json::from_str(raw_json).unwrap();
    assert_eq!(de_raw.prefix, None);
    assert_eq!(de_raw.etag, None);
}

#[test]
fn test_sign_and_verify_revocation_notice() {
    let sk = SigningKey::from_bytes(&[42u8; 32]);
    let drop_pub = hex::encode(sk.verifying_key().as_bytes());
    let rep_drop_pub = hex::encode(
        SigningKey::from_bytes(&[7u8; 32])
            .verifying_key()
            .as_bytes(),
    );
    let rep = SyncReplacementTarget {
        write_url: "https://new.example.org/sync.php".to_string(),
        read_url: "https://new.example.org/d-1234567890abcdef".to_string(),
        drop_pub: rep_drop_pub.clone(),
    };
    let notice = sign_revocation_notice(
        &sk,
        1_700_000_000,
        1_700_864_000,
        Some("Serverumzug".to_string()),
        Some(rep),
    )
    .unwrap();

    assert!(verify_revocation_notice(&notice, &drop_pub).is_ok());

    // Wrong expected drop_pub (valid Ed25519 point from distinct key)
    let wrong_pub = hex::encode(
        SigningKey::from_bytes(&[99u8; 32])
            .verifying_key()
            .as_bytes(),
    );
    assert_invalid_payload_contains(
        verify_revocation_notice(&notice, &wrong_pub),
        "drop_pub mismatch",
        "wrong expected_drop_pub",
    );

    // Tampered replacement URL breaks Ed25519 signature
    let mut tampered = notice.clone();
    tampered.replacement.as_mut().unwrap().write_url =
        "https://evil.example.org/sync.php".to_string();
    assert_invalid_payload_contains(
        verify_revocation_notice(&tampered, &drop_pub),
        "Invalid revocation notice Ed25519 signature",
        "tampered replacement URL",
    );

    // Insecure http:// replacement URL rejected
    let bad_rep = SyncReplacementTarget {
        write_url: "http://insecure.example.org/sync.php".to_string(),
        read_url: "https://new.example.org/d-1234".to_string(),
        drop_pub: rep_drop_pub,
    };
    assert_local_validation_contains(
        sign_revocation_notice(&sk, 100, 200, None, Some(bad_rep)),
        "must use HTTPS",
        "http:// replacement write_url",
    );

    // Newline injection in reason rejected
    assert_local_validation_contains(
        sign_revocation_notice(&sk, 100, 200, Some("bad\nline".to_string()), None),
        "Invalid revocation reason",
        "newline in reason",
    );
}

#[tokio::test]
async fn test_get_slot_etag_304_not_modified() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen_if_none_match = Arc::new(AtomicUsize::new(0));
    let seen_clone = Arc::clone(&seen_if_none_match);

    let alice_id = "aa".repeat(32);
    let missing_id = "bb".repeat(32);
    let missing_suffix = format!("{}.card.bin", missing_id);

    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let seen_inner = Arc::clone(&seen_clone);
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            let mut lines = req.lines();
            let req_line = lines.next().unwrap_or("");
            let path = req_line.split_whitespace().nth(1).unwrap_or("");
            let inm = lines
                .filter_map(|l| l.split_once(':'))
                .find(|(k, _)| k.eq_ignore_ascii_case("if-none-match"))
                .map(|(_, v)| v.trim().to_string());

            let resp: Vec<u8> = if path.ends_with(&missing_suffix) {
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
            } else if inm.as_deref() == Some("\"slot-v1\"") {
                seen_inner.fetch_add(1, Ordering::SeqCst);
                b"HTTP/1.1 304 Not Modified\r\nETag: \"slot-v1\"\r\nContent-Length: 0\r\n\r\n"
                    .to_vec()
            } else {
                let payload = b"opaque-slot-bytes";
                let mut out = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nETag: \"slot-v1\"\r\nContent-Length: {}\r\n\r\n",
                    payload.len()
                )
                .into_bytes();
                out.extend_from_slice(payload);
                out
            };
            stream.write_all(&resp).await.unwrap();
        }
    });

    let client = create_sync_http_client();
    let read_url = format!("http://127.0.0.1:{}/d-test", addr.port());

    // 1. Cold fetch without cached ETag -> 200 Modified with ETag
    let first = get_slot(&client, &read_url, &alice_id, None, "card", None)
        .await
        .unwrap();
    assert_eq!(
        first,
        SlotFetchResult::Modified {
            bytes: b"opaque-slot-bytes".to_vec(),
            etag: Some("slot-v1".to_string()),
        }
    );

    // 2. Conditional fetch with cached ETag -> 304 NotModified
    let second = get_slot(&client, &read_url, &alice_id, None, "card", Some("slot-v1"))
        .await
        .unwrap();
    assert_eq!(second, SlotFetchResult::NotModified);
    assert_eq!(seen_if_none_match.load(Ordering::SeqCst), 1);

    // 3. Missing slot -> NotFound
    let third = get_slot(&client, &read_url, &missing_id, None, "card", None)
        .await
        .unwrap();
    assert_eq!(third, SlotFetchResult::NotFound);

    server.await.unwrap();
}

#[test]
fn test_ssrf_and_production_https_validation() {
    validate_sync_url("https://sync.example.com/api.php").unwrap();
    for (url, expected) in [
        (
            "https://10.0.0.1/api.php",
            "blocked internal/private address",
        ),
        (
            "https://192.168.1.50/api.php",
            "blocked internal/private address",
        ),
        (
            "https://169.254.169.254/latest/meta-data",
            "blocked internal/private address",
        ),
        ("https://printer.local/sync", "internal hostname"),
        (
            "https://user:pass@sync.example.com/api.php",
            "credentials or fragment",
        ),
    ] {
        assert_local_validation_contains(validate_sync_url(url), expected, url);
    }

    // Strict production check (allow_loopback = false):
    assert_local_validation_contains(
        validate_sync_url_strict("http://sync.example.com/api.php", false),
        "must use HTTPS",
        "http production",
    );
    assert_local_validation_contains(
        validate_sync_url_strict("http://127.0.0.1:8080/api.php", false),
        "blocked internal/private address",
        "loopback production",
    );
    validate_sync_url_strict("https://sync.example.com/api.php", false).unwrap();

    // Dev loopback allowed:
    validate_sync_url_strict("http://127.0.0.1:8080/api.php", true).unwrap();
    assert_eq!(
        normalize_lock_read_base("https://sync.example.com/data/locks/"),
        "https://sync.example.com/data"
    );
}

#[test]
fn test_verify_lock_delegation_selftest_and_probe() {
    let admin_sk = SigningKey::from_bytes(&[11u8; 32]);
    let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
    let wrong_admin_hex = hex::encode(
        SigningKey::from_bytes(&[99u8; 32])
            .verifying_key()
            .as_bytes(),
    );
    let lock_sk = SigningKey::from_bytes(&[22u8; 32]);
    let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());

    let write_u = "https://sync.example/sync.php";
    let read_u = "https://sync.example/d";
    let del_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_hex, &lock_hex, write_u, read_u, 1700000000,
    );
    let del = SyncLockDelegationDto {
        admin_pub: admin_hex.clone(),
        lock_pub: lock_hex.clone(),
        write_url: write_u.to_string(),
        read_url: read_u.to_string(),
        issued_at: 1700000000,
        sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
    };
    assert!(verify_lock_delegation(&lock_hex, &del, Some(&admin_hex)));
    assert!(!verify_lock_delegation(
        &lock_hex,
        &del,
        Some(&wrong_admin_hex)
    ));
    let mut bad_url_del = del.clone();
    bad_url_del.write_url = "ftp://invalid.example/sync".to_string();
    assert!(!verify_lock_delegation(
        &lock_hex,
        &bad_url_del,
        Some(&admin_hex)
    ));

    let checks = SyncLockSelftestChecksDto {
        ext_sodium: "pass".into(),
        sha3_256: "pass".into(),
        atomic_link_eexist: "pass".into(),
        fsync: "pass".into(),
        ignore_user_abort: "pass".into(),
        storage_writable: "pass".into(),
        inode_budget: "pass".into(),
        fastcgi_finish_request: "pass".into(),
    };
    let line = format!(
        "{},{},{},{},{},{},{},{},{}",
        checks.ext_sodium,
        checks.sha3_256,
        checks.atomic_link_eexist,
        checks.fsync,
        checks.ignore_user_abort,
        checks.storage_writable,
        checks.inode_budget,
        checks.fastcgi_finish_request,
        50000u64
    );
    let digest = hex::encode(Sha256::digest(line.as_bytes()));
    let st_msg = format!(
        "HUMOCO-LOCK-SELFTEST-V1\nrust\n1700000010\n{}\n1\n{}",
        lock_hex, digest
    );
    let report = SyncLockSelftestReportDto {
        runtime: "rust".into(),
        checked_at: 1700000010,
        lock_pub: lock_hex.clone(),
        php_version: None,
        checks,
        free_inodes: 50000,
        all_passed: true,
        sig: hex::encode(lock_sk.sign(st_msg.as_bytes()).to_bytes()),
    };
    assert!(verify_lock_selftest(&lock_hex, &report));
    assert!(!verify_lock_selftest(&wrong_admin_hex, &report));

    let mut failed_report = report.clone();
    failed_report.all_passed = false;
    assert!(!verify_lock_selftest(&lock_hex, &failed_report));

    let mut python_report = report.clone();
    python_report.runtime = "python".into();
    assert!(!verify_lock_selftest(&lock_hex, &python_report));

    let mut tampered_inodes = report;
    tampered_inodes.free_inodes = 49999;
    assert!(!verify_lock_selftest(&lock_hex, &tampered_inodes));

    let nonce = "ab".repeat(32);
    let probe_msg = format!("HUMOCO-LOCK-PROBE-V1\n{}\n{}\n1700000020", lock_hex, nonce);
    let probe_resp = SyncLockProbeResponseDto {
        probe_ok: true,
        lock_pub: lock_hex.clone(),
        client_nonce: nonce.clone(),
        server_time: 1700000020,
        sig: hex::encode(lock_sk.sign(probe_msg.as_bytes()).to_bytes()),
    };
    assert!(verify_lock_probe_response(&lock_hex, &nonce, &probe_resp));
    assert!(!verify_lock_probe_response(
        &wrong_admin_hex,
        &nonce,
        &probe_resp
    ));
    assert!(!verify_lock_probe_response(
        &lock_hex,
        &"cd".repeat(32),
        &probe_resp
    ));

    let mut bad_ok_probe = probe_resp.clone();
    bad_ok_probe.probe_ok = false;
    assert!(!verify_lock_probe_response(&lock_hex, &nonce, &bad_ok_probe));

    let mut tampered_ts_probe = probe_resp;
    tampered_ts_probe.server_time += 1;
    assert!(!verify_lock_probe_response(
        &lock_hex,
        &nonce,
        &tampered_ts_probe
    ));
}

#[tokio::test]
async fn test_fetch_static_lock_entry_404_and_200() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let entry = LockEntry {
        slot: [1u8; 32],
        parent_slot: [0u8; 32],
        output_branch: 0,
        depth: 0,
        child_lock: [2u8; 32],
        sender_pubkey: [3u8; 32],
        receiver_pubkey_hash: [4u8; 32],
        change_pubkey_hash: None,
        causal_sig: [5u8; 64],
        root_mig_sig: Some([6u8; 64]),
        encrypted_payload: [7u8; 265],
        pow_nonce: 1,
    };
    let wire = entry.to_wire_bytes().unwrap();

    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .unwrap_or("")
                .split_whitespace()
                .nth(1)
                .unwrap_or("");
            let resp: Vec<u8> = if path.contains("missing.lock") {
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
            } else {
                let mut out = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                    wire.len()
                )
                .into_bytes();
                out.extend_from_slice(&wire);
                out
            };
            stream.write_all(&resp).await.unwrap();
        }
    });

    let client = create_sync_http_client();
    let read_url = format!("http://127.0.0.1:{}/d-test", addr.port());

    let miss = fetch_static_lock_entry(&client, &read_url, "2027-Q4/aa/missing.lock")
        .await
        .unwrap();
    assert!(miss.is_none());

    let hit = fetch_static_lock_entry(&client, &read_url, "2027-Q4/aa/present.lock")
        .await
        .unwrap();
    assert_eq!(hit, Some(entry));

    server.await.unwrap();
}

#[test]
fn test_append_sync_query_and_path_helpers() {
    assert_eq!(
        append_sync_query(" https://sync.example.com/api.php ", "info").unwrap(),
        "https://sync.example.com/api.php?info"
    );
    assert_eq!(
        append_sync_query("https://sync.example.com/index.php?drop=1", "lock_ticket").unwrap(),
        "https://sync.example.com/index.php?drop=1&lock_ticket"
    );
    assert_local_validation_contains(
        append_sync_query("https://10.0.0.1/api.php", "info"),
        "blocked internal/private address",
        "SSRF query URL",
    );

    assert_eq!(
        append_sync_path("https://sync.example.com/d-123/", "/index.json").unwrap(),
        "https://sync.example.com/d-123/index.json"
    );
    assert_local_validation_contains(
        append_sync_path("https://192.168.1.10/d-123", "index.json"),
        "blocked internal/private address",
        "SSRF read URL",
    );
}

#[test]
fn test_rate_limit_error_and_header_helpers() {
    let mut hdrs = reqwest::header::HeaderMap::new();
    let default_err = rate_limit_error("fetching index", &hdrs);
    assert_eq!(default_err.retry_after_secs(), Some(60));
    assert_eq!(
        default_err.to_string(),
        "Rate limited fetching index (HTTP 429). Retry-After: 60s"
    );
    hdrs.insert(reqwest::header::RETRY_AFTER, "42".parse().unwrap());
    let parsed_err = rate_limit_error("fetching index", &hdrs);
    assert_eq!(parsed_err.retry_after_secs(), Some(42));
    assert!(parsed_err.is_rate_limited());
    assert_eq!(
        parsed_err.to_string(),
        "Rate limited fetching index (HTTP 429). Retry-After: 42s"
    );

    let client = create_sync_http_client();
    let signed = SignedDropHeaders {
        pubkey_hex: "aa".repeat(32),
        prefix: Some("delta".to_string()),
        timestamp: 1_700_000_000,
        nonce_hex: "bb".repeat(16),
        signature_hex: "cc".repeat(64),
    };
    let req_with_pfx = apply_signed_headers(
        client.put("https://sync.example.com/api.php"),
        &signed,
        true,
    )
    .build()
    .unwrap();
    assert_eq!(
        req_with_pfx.headers().get("X-Drop-Prefix").unwrap(),
        "delta"
    );
    assert_eq!(
        req_with_pfx.headers().get("X-Drop-Timestamp").unwrap(),
        "1700000000"
    );

    let req_without_pfx = apply_signed_headers(
        client.put("https://sync.example.com/api.php"),
        &signed,
        false,
    )
    .build()
    .unwrap();
    assert!(req_without_pfx.headers().get("X-Drop-Prefix").is_none());

    let admin_sk = SigningKey::from_bytes(&[33u8; 32]);
    let admin_req = apply_admin_headers(
        client.get("https://sync.example.com/api.php?status"),
        &admin_sk,
        "STATUS",
        "",
        b"",
    )
    .build()
    .unwrap();
    assert_eq!(
        admin_req.headers().get("X-Drop-Pubkey").unwrap(),
        hex::encode(admin_sk.verifying_key().as_bytes()).as_str()
    );
    assert!(admin_req.headers().get("X-Drop-Signature").is_some());
    assert!(admin_req.headers().get("X-Drop-Prefix").is_none());
}

#[tokio::test]
async fn test_put_slot_special_status_codes() {
    use tokio::io::AsyncWriteExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let responses = vec![
        "HTTP/1.1 412 Precondition Failed\r\nETag: \"remote-v2\"\r\nContent-Length: 0\r\n\r\n",
        "HTTP/1.1 413 Payload Too Large\r\nContent-Length: 0\r\n\r\n",
        "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 17\r\nContent-Length: 0\r\n\r\n",
        "HTTP/1.1 507 Insufficient Storage\r\nContent-Length: 0\r\n\r\n",
        "HTTP/1.1 204 No Content\r\nETag: \"saved-v3\"\r\nContent-Length: 0\r\n\r\n",
    ];

    let server = tokio::spawn(async move {
        for raw in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let req = read_request_non_empty(&mut stream).await;
            assert!(
                req.starts_with("PUT /sync.php "),
                "unexpected PUT request line: {}",
                req.lines().next().unwrap_or("")
            );
            stream.write_all(raw.as_bytes()).await.unwrap();
        }
    });

    let client = create_sync_http_client();
    let write_url = format!("http://127.0.0.1:{}/sync.php", addr.port());
    let signed = SignedDropHeaders {
        pubkey_hex: "aa".repeat(32),
        prefix: None,
        timestamp: 1_700_000_000,
        nonce_hex: "bb".repeat(16),
        signature_hex: "cc".repeat(64),
    };

    let r412 = put_slot(&client, &write_url, &signed, "card", b"abc", Some("old"))
        .await
        .unwrap();
    assert_eq!(
        r412,
        PutSlotOutcome::PreconditionFailed {
            current_etag: Some("remote-v2".to_string())
        }
    );

    let r413 = put_slot(&client, &write_url, &signed, "card", b"abc", None)
        .await
        .unwrap_err();
    assert_eq!(r413.http_status(), Some(413));
    assert!(r413.to_string().contains("HTTP 413"));

    let r429 = put_slot(&client, &write_url, &signed, "card", b"abc", None)
        .await
        .unwrap_err();
    assert!(r429.is_rate_limited());
    assert_eq!(r429.retry_after_secs(), Some(17));
    assert!(r429.to_string().contains("(HTTP 429). Retry-After: 17s"));

    let r507 = put_slot(&client, &write_url, &signed, "card", b"abc", None)
        .await
        .unwrap_err();
    assert_eq!(r507.http_status(), Some(507));
    assert!(r507.to_string().contains("HTTP 507"));

    let r204 = put_slot(&client, &write_url, &signed, "card", b"abc", None)
        .await
        .unwrap();
    assert_eq!(
        r204,
        PutSlotOutcome::Saved {
            etag: Some("saved-v3".to_string()),
            slot_ack: None,
        }
    );

    server.await.unwrap();
}

#[tokio::test]
async fn test_push_lock_delegation_signs_put_lock_delegate() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let admin_sk = SigningKey::from_bytes(&[44u8; 32]);
    let admin_pub = hex::encode(admin_sk.verifying_key().as_bytes());
    let lock_sk = SigningKey::from_bytes(&[55u8; 32]);
    let lock_pub = hex::encode(lock_sk.verifying_key().as_bytes());
    let issued_at = 1_760_123_456u64;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let admin_pub_expected = admin_pub.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap();
        let raw = String::from_utf8_lossy(&buf[..n]);
        let (head, body) = raw.split_once("\r\n\r\n").unwrap();
        let mut lines = head.lines();
        let req_line = lines.next().unwrap();
        assert!(
            req_line.starts_with("PUT /sync.php?lock_delegate"),
            "Unexpected request line: {}",
            req_line
        );

        let hdrs = parse_http_headers(lines);
        let hdr_pub = hdrs.get("x-drop-pubkey").expect("missing X-Drop-Pubkey").clone();
        let hdr_sig = hdrs
            .get("x-drop-signature")
            .expect("missing X-Drop-Signature")
            .clone();
        let hdr_ts: u64 = hdrs
            .get("x-drop-timestamp")
            .expect("missing X-Drop-Timestamp")
            .parse()
            .unwrap();
        let hdr_nonce = hdrs.get("x-drop-nonce").expect("missing X-Drop-Nonce").clone();

        assert_eq!(hdr_pub, admin_pub_expected);
        let canonical = humoco_market::format_admin_request_message(
            "PUT_LOCK_DELEGATE",
            "",
            hdr_ts,
            &hdr_nonce,
            body.as_bytes(),
        );
        assert!(verify_ed25519_hex_strict(
            &admin_pub_expected,
            &hdr_sig,
            canonical.as_bytes()
        ));

        let dto: SyncLockDelegationDto = serde_json::from_str(body).unwrap();
        assert!(verify_lock_delegation(
            &dto.lock_pub,
            &dto,
            Some(&admin_pub_expected)
        ));

        let resp_body = serde_json::to_string(&dto).unwrap();
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            resp_body.len(),
            resp_body
        );
        stream.write_all(resp.as_bytes()).await.unwrap();
    });

    let client = create_sync_http_client();
    let write_url = format!("http://127.0.0.1:{}/sync.php", addr.port());
    let read_url = format!("http://127.0.0.1:{}/d", addr.port());
    let returned = push_lock_delegation(
        &client, &write_url, &read_url, &admin_sk, &lock_pub, issued_at,
    )
    .await
    .unwrap();
    assert_eq!(returned.admin_pub, admin_pub);
    assert_eq!(returned.lock_pub, lock_pub);
    assert_eq!(returned.write_url, write_url);
    assert_eq!(returned.read_url, read_url);
    assert_eq!(returned.issued_at, issued_at);
    assert!(verify_lock_delegation(
        &lock_pub,
        &returned,
        Some(&admin_pub)
    ));

    server.await.unwrap();
}

#[tokio::test]
async fn test_request_genesis_ticket_direct_and_stealth_vs_pow_rejected() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let admin_sk = SigningKey::from_bytes(&[61u8; 32]);
    let admin_pub_hex = hex::encode(admin_sk.verifying_key().as_bytes());
    let wallet_ed_sk = SigningKey::from_bytes(&[62u8; 32]);
    let stealth_p_hex =
        humoco_market::derive_member_stealth_pubkey(&wallet_ed_sk, &admin_pub_hex).unwrap();
    let lock_sk = SigningKey::from_bytes(&[63u8; 32]);
    let lock_pub = lock_sk.verifying_key().to_bytes();
    let anchor_id = [0x42u8; 32];
    let retain_q = human_money_core::humoco_proto::encode_quarter(2035, 4).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let exp_admin = admin_pub_hex.clone();
    let exp_stealth = stealth_p_hex.clone();

    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).await.unwrap();
            let raw = String::from_utf8_lossy(&buf[..n]);
            let (head, body) = raw.split_once("\r\n\r\n").unwrap();
            let mut lines = head.lines();
            let req_line = lines.next().unwrap();
            assert!(req_line.starts_with("POST /sync.php?lock_ticket"));

            let hdrs = parse_http_headers(lines);
            let hdr_pub = hdrs
                .get("x-drop-pubkey")
                .map(|v| v.to_ascii_lowercase())
                .unwrap_or_default();
            let hdr_sig = hdrs.get("x-drop-signature").cloned().unwrap_or_default();
            let hdr_ts: Option<u64> = hdrs
                .get("x-drop-timestamp")
                .and_then(|v| v.parse().ok());
            let hdr_nonce = hdrs.get("x-drop-nonce").cloned().unwrap_or_default();

            let authorized_key = hdr_pub == exp_admin || hdr_pub == exp_stealth;
            let valid_sig = hdr_ts.is_some_and(|ts| {
                let msg = humoco_market::format_member_request_message(
                    "POST",
                    "lock_ticket",
                    None,
                    ts,
                    &hdr_nonce,
                    body.as_bytes(),
                );
                verify_ed25519_hex_strict(&hdr_pub, &hdr_sig, msg.as_bytes())
            });

            let (status_line, resp_body) = if authorized_key && valid_sig {
                let req_json: serde_json::Value = serde_json::from_str(body).unwrap();
                let req_aid =
                    decode_hex_fixed::<32>(req_json["anchor_id"].as_str().unwrap()).unwrap();
                let req_q = req_json["retain_until_quarter"].as_u64().unwrap() as u16;
                let mut ticket = GenesisTicket {
                    anchor_id: req_aid,
                    retain_until_quarter: req_q,
                    issued_at: 1_760_000_000,
                    lock_pub,
                    sig: [0u8; 64],
                };
                ticket.sig = lock_sk.sign(&ticket.digest()).to_bytes();
                (
                    "200 OK",
                    serde_json::json!({ "ticket": ticket }).to_string(),
                )
            } else {
                ("403 Forbidden", r#"{"error":"pow_disabled"}"#.to_string())
            };
            let resp = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status_line,
                resp_body.len(),
                resp_body
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
        }
    });

    let client = create_sync_http_client();
    let write_url = format!("http://127.0.0.1:{}/sync.php", addr.port());

    // 1. Direct admin auth succeeds without PoW
    let t_direct = request_genesis_ticket(
        &client,
        &write_url,
        lock_pub,
        &anchor_id,
        retain_q,
        LockTicketAuth::Direct(admin_sk),
    )
    .await
    .unwrap();
    assert!(t_direct.verify());
    assert_eq!(t_direct.lock_pub, lock_pub);

    // 2. Stealth member auth succeeds without PoW
    let t_stealth = request_genesis_ticket(
        &client,
        &write_url,
        lock_pub,
        &anchor_id,
        retain_q,
        LockTicketAuth::Stealth {
            wallet_ed_sk,
            drop_pub_hex: admin_pub_hex,
        },
    )
    .await
    .unwrap();
    assert!(t_stealth.verify());
    assert_eq!(t_stealth.lock_pub, lock_pub);

    // 3. PoW request is rejected with 403 when allow_pow_tickets = false
    let err_pow = request_genesis_ticket(
        &client,
        &write_url,
        lock_pub,
        &anchor_id,
        retain_q,
        LockTicketAuth::Pow { pow_bits: 1 },
    )
    .await
    .unwrap_err();
    assert_eq!(err_pow.http_status(), Some(403));
    assert!(
        err_pow.to_string().contains("403"),
        "expected HTTP 403, got: {}",
        err_pow
    );

    server.await.unwrap();
}

#[test]
fn test_h11_ssrf_blocks_ipv4_mapped_v6_nat64_and_filters_resolved_ips() {
    let blocked_ips = [
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "::ffff:169.254.169.254",
        "::ffff:100.64.0.1",
        "64:ff9b::7f00:1",
        "64:ff9b::a00:1",
        "::7f00:1",
        "0.0.0.1",
        "100.64.0.1",
        "198.18.0.1",
        "240.0.0.1",
        "fc00::1",
        "fe80::1",
        "2001:db8::1",
    ];
    for ip_str in blocked_ips {
        let ip: std::net::IpAddr = ip_str.parse().unwrap();
        assert!(
            is_blocked_ip(ip, false),
            "expected {} to be blocked when allow_loopback=false",
            ip_str
        );
    }

    let allowed_ips = [
        "93.184.216.34",
        "::ffff:93.184.216.34",
        "2606:2800:220:1:248:1893:25c8:1946",
    ];
    for ip_str in allowed_ips {
        let ip: std::net::IpAddr = ip_str.parse().unwrap();
        assert!(
            !is_blocked_ip(ip, false),
            "expected {} to be allowed when allow_loopback=false",
            ip_str
        );
    }

    let mapped_loopback: std::net::IpAddr = "::ffff:127.0.0.1".parse().unwrap();
    assert!(is_blocked_ip(mapped_loopback, false));
    assert!(!is_blocked_ip(mapped_loopback, true));

    let bad_urls = [
        (
            "https://[::ffff:127.0.0.1]/sync.php",
            "blocked internal/private address",
        ),
        (
            "https://[::ffff:169.254.169.254]/sync.php",
            "blocked internal/private address",
        ),
        (
            "https://[64:ff9b::7f00:1]/sync.php",
            "blocked internal/private address",
        ),
        (
            "https://user:pass@sync.example.org/sync.php",
            "credentials or fragment",
        ),
        (
            "https://sync.example.org/sync.php#frag",
            "credentials or fragment",
        ),
        ("https://service.local/sync.php", "internal hostname"),
        (
            "http://localhost:8080/sync.php",
            "loopback is forbidden in production",
        ),
    ];
    for (url, expected) in bad_urls {
        assert_local_validation_contains(
            validate_sync_url_strict(url, false),
            expected,
            url,
        );
    }
    validate_sync_url_strict("https://sync.example.org/sync.php", false).unwrap();

    let empty_err = filter_resolved_addrs(Vec::new(), "dual.example.org", false).unwrap_err();
    assert_eq!(empty_err.kind(), std::io::ErrorKind::NotFound);

    let public_v4: std::net::SocketAddr = "93.184.216.34:443".parse().unwrap();
    let private_mapped_v6: std::net::SocketAddr = "[::ffff:169.254.169.254]:443".parse().unwrap();
    let mixed_err = filter_resolved_addrs(
        vec![public_v4, private_mapped_v6],
        "dual.example.org",
        false,
    )
    .unwrap_err();
    assert_eq!(mixed_err.kind(), std::io::ErrorKind::PermissionDenied);

    let clean = filter_resolved_addrs(vec![public_v4], "dual.example.org", false).unwrap();
    assert_eq!(clean, vec![public_v4]);
}

#[test]
fn test_s08_f3_retry_after_clamping_and_strict_429_extraction() {
    let mut hdrs = reqwest::header::HeaderMap::new();
    hdrs.insert(reqwest::header::RETRY_AFTER, "0".parse().unwrap());
    assert_eq!(parse_retry_after(&hdrs), 1);

    hdrs.insert(reqwest::header::RETRY_AFTER, "999999999".parse().unwrap());
    assert_eq!(parse_retry_after(&headers_with("999999999")), 3600);
    assert_eq!(parse_retry_after(&hdrs), 3600);

    // HTTP 500 embedding attacker-controlled Retry-After / HTTP 429 text must NOT trigger cooldown
    let spoofed_500 = SyncClientError::HttpStatus {
        status: 500,
        retry_after_secs: None,
        context: "fetching index".to_string(),
        snippet: "Retry-After: 9999 HTTP 429".to_string(),
    };
    assert_eq!(spoofed_500.retry_after_secs(), None);
    assert!(!spoofed_500.is_rate_limited());

    // Network errors containing "429" in hex slot ID or port must NOT trigger false-positive cooldown
    let hex_with_429 = format!("{}429{}", "ab".repeat(15), "cd".repeat(16));
    let net_err = SyncClientError::Transport(format!(
        "Network error fetching slot {} at https://sync.example.org:4290/d",
        hex_with_429
    ));
    assert_eq!(net_err.retry_after_secs(), None);
    assert!(!net_err.is_rate_limited());

    // Legitimate RateLimited variants clamp to 1..=3600 and preserve Display markers
    let rl_high = SyncClientError::RateLimited {
        status: 429,
        retry_after_secs: 999_999_999,
        context: "fetching index".to_string(),
        snippet: String::new(),
    };
    assert_eq!(rl_high.retry_after_secs(), Some(3600));
    assert!(rl_high.to_string().contains("HTTP 429"));
    assert!(rl_high.to_string().contains("Retry-After: 3600s"));

    let rl_zero = SyncClientError::RateLimited {
        status: 429,
        retry_after_secs: 0,
        context: "fetching index".to_string(),
        snippet: String::new(),
    };
    assert_eq!(rl_zero.retry_after_secs(), Some(1));
    assert!(rl_zero.to_string().contains("Retry-After: 1s"));
}

fn headers_with(val: &str) -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(reqwest::header::RETRY_AFTER, val.parse().unwrap());
    h
}

#[tokio::test]
async fn test_s08_f1_bounded_http_body_and_sanitized_error_body() {
    use tokio::io::AsyncWriteExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        // 1. Static lock with Content-Length: 606 (> 605)
        {
            let (mut stream, _) = listener.accept().await.unwrap();
            let req = read_request_non_empty(&mut stream).await;
            assert!(req.starts_with("GET "));
            let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 606\r\n\r\n";
            stream.write_all(resp.as_bytes()).await.unwrap();
        }
        // 2. Static lock with chunked streaming > 605 bytes (no Content-Length)
        {
            let (mut stream, _) = listener.accept().await.unwrap();
            let req = read_request_non_empty(&mut stream).await;
            assert!(req.starts_with("GET "));
            let chunk = vec![b'A'; 700];
            let chunk_hdr = format!("{:x}\r\n", chunk.len());
            if write_allow_early_close(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
            )
            .await
                && write_allow_early_close(&mut stream, chunk_hdr.as_bytes()).await
                && write_allow_early_close(&mut stream, &chunk).await
            {
                let finished_tail = write_allow_early_close(&mut stream, b"\r\n0\r\n\r\n").await;
                if finished_tail {
                    stream.flush().await.unwrap_or(());
                }
            }
        }
        // 3. fetch_sync_info with Content-Length > 256 KiB
        {
            let (mut stream, _) = listener.accept().await.unwrap();
            let req = read_request_non_empty(&mut stream).await;
            assert!(req.starts_with("GET "));
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                300 * 1024
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
        }
        // 4. Error 500 with > 4 KiB body and control characters / newlines
        {
            let (mut stream, _) = listener.accept().await.unwrap();
            let req = read_request_non_empty(&mut stream).await;
            assert!(req.starts_with("GET "));
            let malicious_body = format!("bad\r\n\t\x1b[31merror {}", "X".repeat(8192));
            let resp = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\n\r\n{}",
                malicious_body.len(),
                malicious_body
            );
            let sent_all = write_allow_early_close(&mut stream, resp.as_bytes()).await;
            if sent_all {
                stream.flush().await.unwrap_or(());
            }
        }
    });

    let client = create_sync_http_client();
    let read_url = format!("http://127.0.0.1:{}/d-test", addr.port());
    let write_url = format!("http://127.0.0.1:{}/sync.php", addr.port());

    // 1. Content-Length 606 rejected upfront
    let err1 = fetch_static_lock_entry(&client, &read_url, "2027-Q4/aa/oversized.lock")
        .await
        .unwrap_err();
    assert!(matches!(
        err1,
        SyncClientError::ResponseTooLarge {
            max_bytes: LOCK_ENTRY_WIRE_LEN,
            ..
        }
    ));
    assert!(
        err1.to_string().contains("exceeds maximum allowed size"),
        "unexpected error for Content-Length 606: {}",
        err1
    );

    // 2. Chunked stream > 605 rejected
    let err2 = fetch_static_lock_entry(&client, &read_url, "2027-Q4/aa/chunked.lock")
        .await
        .unwrap_err();
    assert!(matches!(
        err2,
        SyncClientError::ResponseTooLarge {
            max_bytes: LOCK_ENTRY_WIRE_LEN,
            ..
        }
    ));
    assert!(
        err2.to_string().contains("exceeds maximum allowed size"),
        "unexpected error for chunked > 605: {}",
        err2
    );

    // 3. fetch_sync_info > 256 KiB rejected
    let err3 = fetch_sync_info(&client, &write_url).await.unwrap_err();
    assert!(matches!(
        err3,
        SyncClientError::ResponseTooLarge {
            max_bytes: MAX_JSON_META_BODY_BYTES,
            ..
        }
    ));
    assert!(
        err3.to_string().contains("exceeds maximum allowed size"),
        "unexpected error for oversized ?info: {}",
        err3
    );

    // 4. Error body truncated and sanitized of control chars
    let err4 = fetch_sync_info(&client, &write_url).await.unwrap_err();
    assert_eq!(err4.http_status(), Some(500));
    let err4_str = err4.to_string();
    assert!(
        err4_str.contains("HTTP 500"),
        "unexpected error: {}",
        err4_str
    );
    assert!(
        !err4_str.chars().any(|c| c.is_control()),
        "error leaked control chars: {:?}",
        err4_str
    );
    assert!(
        err4_str.len() <= 400,
        "error body was not truncated (len={}): {}",
        err4_str.len(),
        err4_str
    );

    server.await.unwrap();
}

#[tokio::test]
async fn test_s08_f2_path_traversal_query_and_read_url_guards() {
    let client = create_sync_http_client();
    let valid_id = "aa".repeat(32);
    let read_url = "https://sync.example.org/d-test";
    let write_url = "https://sync.example.org/sync.php";
    let admin_sk = SigningKey::from_bytes(&[77u8; 32]);
    let signed = SignedDropHeaders {
        pubkey_hex: valid_id.clone(),
        prefix: None,
        timestamp: 1_700_000_000,
        nonce_hex: "bb".repeat(16),
        signature_hex: "cc".repeat(64),
    };

    fn assert_validation_err<T: std::fmt::Debug>(res: Result<T, SyncClientError>, ctx: &str) {
        let err = res.expect_err(ctx);
        assert!(
            err.is_local_validation(),
            "{}: expected LocalValidation error, got '{}'",
            ctx,
            err
        );
    }

    // get_slot rejects path traversal, invalid id, invalid prefix, invalid channel, and query in read_url
    assert_validation_err(
        get_slot(&client, read_url, "../../etc?x=#", None, "card", None).await,
        "get_slot traversal id",
    );
    assert_validation_err(
        get_slot(&client, read_url, &valid_id, Some("../foo"), "card", None).await,
        "get_slot invalid prefix",
    );
    assert_validation_err(
        get_slot(&client, read_url, &valid_id, None, "card?x=1", None).await,
        "get_slot invalid channel",
    );
    assert_validation_err(
        get_slot(
            &client,
            "https://sync.example.org/d-test?q=1",
            &valid_id,
            None,
            "card",
            None,
        )
        .await,
        "get_slot read_url query",
    );

    // append_sync_path rejects query string in read_url
    assert_local_validation_contains(
        append_sync_path("https://sync.example.org/d-test?q=1", "index.json"),
        "must not contain a query string",
        "append_sync_path read_url query",
    );

    // fetch_static_lock_entry rejects query in read_url and traversal / encoded / query paths
    assert_validation_err(
        fetch_static_lock_entry(
            &client,
            "https://sync.example.org/d-test?q=1",
            "2027-Q4/aa/present.lock",
        )
        .await,
        "fetch_static_lock_entry read_url query",
    );
    for bad_rel in [
        "2027-Q4/%2e%2e/present.lock",
        "2027-Q4\\aa\\present.lock",
        "2027-Q4/aa/present.lock?x=1",
        "2027-Q4/aa/present.lock#frag",
        "2027-Q4/../present.lock",
        "2027-Q4/aa/present.txt",
    ] {
        assert_validation_err(
            fetch_static_lock_entry(&client, read_url, bad_rel).await,
            bad_rel,
        );
    }

    // get_lock_freeze_status rejects non-64-hex anchor_id
    assert_validation_err(
        get_lock_freeze_status(&client, write_url, "abc&x=1").await,
        "get_lock_freeze_status anchor_id",
    );

    // push_members rejects invalid channel query injection
    assert_validation_err(
        push_members(&client, write_url, &admin_sk, "card&evil=1", &[]).await,
        "push_members channel",
    );

    // put_media_blob and delete_media_blob validate 64 lowercase hex blob_id
    assert_validation_err(
        put_media_blob(&client, write_url, &signed, "BAD_ID\r\n", b"x").await,
        "put_media_blob blob_id",
    );
    assert_validation_err(
        delete_media_blob(&client, write_url, &signed, Some("not-hex")).await,
        "delete_media_blob blob_id",
    );

    // anchor_manifest_candidate_urls strips query strings on write URLs and rejects invalid anchor hex
    let cands =
        anchor_manifest_candidate_urls("https://sync.example.org/sub/sync.php?drop=1", &valid_id)
            .unwrap();
    assert_eq!(
        cands,
        vec![format!(
            "https://sync.example.org/sub/anchors/{}.bin",
            valid_id
        )]
    );
    assert_local_validation_contains(
        anchor_manifest_candidate_urls(write_url, "not-64-hex"),
        "Invalid anchor_hex",
        "anchor_manifest_candidate_urls invalid anchor hex",
    );
}

#[tokio::test]
async fn test_s08_f4_etag_normalization_and_list_index_conditional_statuses() {
    use tokio::io::AsyncWriteExt;

    // 1. Weak/quoted ETag normalization & rejection of empty/quote/control tokens
    let mut weak_hdrs = reqwest::header::HeaderMap::new();
    weak_hdrs.insert(reqwest::header::ETAG, "W/\"abc123\"".parse().unwrap());
    assert_eq!(extract_etag(&weak_hdrs), Some("abc123".to_string()));
    weak_hdrs.insert(reqwest::header::ETAG, "w/\"xyz789\"".parse().unwrap());
    assert_eq!(extract_etag(&weak_hdrs), Some("xyz789".to_string()));
    assert_eq!(normalize_etag_token("\"\""), None);
    assert_eq!(normalize_etag_token("bad\"inner"), None);

    // 2. list_index sends quoted If-None-Match + If-Modified-Since and handles 200 / 304 / 404 / 429
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ok_json = serde_json::to_string(&vec![SyncIndexEntry {
        id: "aa".repeat(32),
        prefix: None,
        channel: "card".to_string(),
        mtime: 1_700_000_000,
        size: 128,
        etag: Some("row-etag".to_string()),
    }])
    .unwrap();

    let server = tokio::spawn(async move {
        for idx in 0..4 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let req = read_request_non_empty(&mut stream).await;
            let hdrs = parse_http_headers(req.lines().skip(1));
            let inm = hdrs.get("if-none-match").map(String::as_str);
            let ims = hdrs.get("if-modified-since").map(String::as_str);
            let resp = match idx {
                0 => {
                    assert_eq!(inm, Some("\"etag-v1\""));
                    assert_eq!(ims, Some("Wed, 21 Oct 2015 07:28:00 GMT"));
                    "HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\n\r\n".to_string()
                }
                1 => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string(),
                2 => {
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 12\r\nContent-Length: 0\r\n\r\n"
                        .to_string()
                }
                _ => format!(
                    "HTTP/1.1 200 OK\r\nETag: W/\"idx-v2\"\r\nLast-Modified: Thu, 22 Oct 2015 08:00:00 GMT\r\nContent-Length: {}\r\n\r\n{}",
                    ok_json.len(),
                    ok_json
                ),
            };
            stream.write_all(resp.as_bytes()).await.unwrap();
        }
    });

    let client = create_sync_http_client();
    let read_url = format!("http://127.0.0.1:{}/d-test", addr.port());

    let r304 = list_index(
        &client,
        &read_url,
        Some("etag-v1"),
        Some("Wed, 21 Oct 2015 07:28:00 GMT"),
    )
    .await
    .unwrap();
    assert_eq!(r304, IndexFetchResult::NotModified);

    let r404 = list_index(&client, &read_url, None, None).await.unwrap();
    assert_eq!(
        r404,
        IndexFetchResult::Modified {
            entries: Vec::new(),
            etag: None,
            last_modified: None
        }
    );

    let r429 = list_index(&client, &read_url, None, None)
        .await
        .unwrap_err();
    assert!(r429.is_rate_limited());
    assert_eq!(r429.retry_after_secs(), Some(12));

    let r200 = list_index(&client, &read_url, None, None).await.unwrap();
    assert!(matches!(
        r200,
        IndexFetchResult::Modified {
            ref entries,
            etag: Some(ref e),
            last_modified: Some(ref lm),
        } if entries.len() == 1 && e == "idx-v2" && lm == "Thu, 22 Oct 2015 08:00:00 GMT"
    ));
    server.await.unwrap();
}

#[test]
fn test_s08_f4_slot_ack_receipt_verification_invariants() {
    let lock_sk = SigningKey::from_bytes(&[15u8; 32]);
    let lock_pub = hex::encode(lock_sk.verifying_key().as_bytes());
    let other_lock_pub = hex::encode(
        SigningKey::from_bytes(&[19u8; 32])
            .verifying_key()
            .as_bytes(),
    );
    let slot_pk = "aa".repeat(32);

    let zero_ts_msg = format!(
        "HUMOCO-SLOT-ACK-V1\n{}\n{}\n\nbackup\netag1\n0",
        lock_pub, slot_pk
    );
    let zero_ts_ack = SlotAckReceipt {
        lock_pub: lock_pub.clone(),
        pubkey: slot_pk.clone(),
        prefix: None,
        channel: "backup".to_string(),
        etag: "etag1".to_string(),
        timestamp: 0,
        sig: hex::encode(lock_sk.sign(zero_ts_msg.as_bytes()).to_bytes()),
    };
    assert!(!zero_ts_ack.verify(&lock_pub, &slot_pk, None, "backup", "etag1"));

    let valid_ack_msg = format!(
        "HUMOCO-SLOT-ACK-V1\n{}\n{}\nwork\nbackup\netag1\n1700000000",
        lock_pub, slot_pk
    );
    let valid_ack = SlotAckReceipt {
        lock_pub: lock_pub.clone(),
        pubkey: slot_pk.clone(),
        prefix: Some("work".to_string()),
        channel: "backup".to_string(),
        etag: "etag1".to_string(),
        timestamp: 1_700_000_000,
        sig: hex::encode(lock_sk.sign(valid_ack_msg.as_bytes()).to_bytes()),
    };
    assert!(valid_ack.verify(&lock_pub, &slot_pk, Some("work"), "backup", "etag1"));
    assert!(!valid_ack.verify(&other_lock_pub, &slot_pk, Some("work"), "backup", "etag1"));
    assert!(!valid_ack.verify(&lock_pub, &slot_pk, Some("work"), "backup", "etag2"));
    assert!(!valid_ack.verify(&lock_pub, &slot_pk, Some("other"), "backup", "etag1"));
    assert!(!valid_ack.verify(&lock_pub, &slot_pk, Some("work"), "post", "etag1"));

    let mut ws_lock_ack = valid_ack;
    ws_lock_ack.lock_pub = format!(" {}", lock_pub);
    assert!(!ws_lock_ack.verify(&lock_pub, &slot_pk, Some("work"), "backup", "etag1"));
}

#[test]
fn test_s08_f5_revocation_notice_strict_signing_and_verification() {
    let admin_sk = SigningKey::from_bytes(&[16u8; 32]);
    let admin_pub = hex::encode(admin_sk.verifying_key().as_bytes());
    let rep_sk = SigningKey::from_bytes(&[17u8; 32]);
    let rep_pub = hex::encode(rep_sk.verifying_key().as_bytes());

    let valid_notice = sign_revocation_notice(
        &admin_sk,
        100,
        200,
        Some("server migration".to_string()),
        Some(SyncReplacementTarget {
            write_url: "https://new.example.org/sync.php".to_string(),
            read_url: "https://new.example.org/d".to_string(),
            drop_pub: rep_pub.clone(),
        }),
    )
    .expect("valid revocation notice must sign");
    verify_revocation_notice(&valid_notice, &admin_pub)
        .expect("valid revocation notice must verify");
    assert_eq!(valid_notice.drop_pub, admin_pub);
    assert_eq!(valid_notice.reason.as_deref(), Some("server migration"));

    // Blank reason normalizes to None; 200-char reason succeeds on both sign & verify; 201 fails both
    let blank_reason_notice =
        sign_revocation_notice(&admin_sk, 100, 200, Some("   ".to_string()), None)
            .expect("whitespace reason must normalize cleanly");
    assert_eq!(blank_reason_notice.reason, None);
    verify_revocation_notice(&blank_reason_notice, &admin_pub).unwrap();

    let max_reason_notice =
        sign_revocation_notice(&admin_sk, 100, 200, Some("r".repeat(200)), None)
            .expect("200-char reason must succeed on sign");
    verify_revocation_notice(&max_reason_notice, &admin_pub)
        .expect("200-char reason must succeed on verify");

    assert_local_validation_contains(
        sign_revocation_notice(&admin_sk, 100, 200, Some("r".repeat(201)), None),
        "Invalid revocation reason",
        "201-char reason on sign",
    );

    assert_local_validation_contains(
        sign_revocation_notice(&admin_sk, 100, 0, None, None),
        "non-zero issued_at and effective_at",
        "effective_at == 0",
    );
    assert_local_validation_contains(
        sign_revocation_notice(&admin_sk, 0, 100, None, None),
        "non-zero issued_at and effective_at",
        "issued_at == 0",
    );
    assert_local_validation_contains(
        sign_revocation_notice(&admin_sk, 100, 200, Some("bad\tctrl".to_string()), None),
        "Invalid revocation reason",
        "control char in reason",
    );
    assert_local_validation_contains(
        sign_revocation_notice(
            &admin_sk,
            100,
            200,
            None,
            Some(SyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: "https://new.example.org/d?q=1".to_string(),
                drop_pub: rep_pub.clone(),
            }),
        ),
        "must not contain a query string",
        "query in replacement read_url",
    );

    let non_curve_byte = (1u8..=255)
        .find(|&b| VerifyingKey::from_bytes(&[b; 32]).is_err())
        .unwrap();
    let non_curve_hex = hex::encode([non_curve_byte; 32]);
    assert_local_validation_contains(
        sign_revocation_notice(
            &admin_sk,
            100,
            200,
            None,
            Some(SyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: "https://new.example.org/d".to_string(),
                drop_pub: non_curve_hex,
            }),
        ),
        "Invalid Ed25519 public key",
        "non-curve replacement drop_pub",
    );
}

#[test]
fn test_s08_f5_revocation_notice_verification_gates() {
    let admin_sk = SigningKey::from_bytes(&[16u8; 32]);
    let admin_pub = hex::encode(admin_sk.verifying_key().as_bytes());
    let rep_sk = SigningKey::from_bytes(&[17u8; 32]);
    let rep_pub = hex::encode(rep_sk.verifying_key().as_bytes());
    let max_reason_notice =
        sign_revocation_notice(&admin_sk, 100, 200, Some("r".repeat(200)), None)
            .expect("200-char reason must sign cleanly");
    assert_eq!(max_reason_notice.drop_pub, admin_pub);

    let mut bad_drop_ws = max_reason_notice.clone();
    bad_drop_ws.drop_pub = format!("{} ", admin_pub);
    assert_invalid_payload_contains(
        verify_revocation_notice(&bad_drop_ws, &admin_pub),
        "invalid whitespace or control chars",
        "outer ws on drop_pub",
    );

    let mut bad_sig_ws = max_reason_notice.clone();
    bad_sig_ws.signature = format!(" {}", bad_sig_ws.signature);
    assert_invalid_payload_contains(
        verify_revocation_notice(&bad_sig_ws, &admin_pub),
        "invalid whitespace or control chars",
        "outer ws on signature",
    );

    for (label, issued_at, effective_at) in [("issued_at == 0", 0, 200), ("effective_at == 0", 100, 0)]
    {
        let mut zero_ts = max_reason_notice.clone();
        zero_ts.issued_at = issued_at;
        zero_ts.effective_at = effective_at;
        assert_invalid_payload_contains(
            verify_revocation_notice(&zero_ts, &admin_pub),
            "missing timestamps",
            label,
        );
    }

    for bad_reason in ["r".repeat(201), "bad\x07reason".to_string()] {
        let mut bad_r_notice = max_reason_notice.clone();
        bad_r_notice.reason = Some(bad_reason);
        assert_invalid_payload_contains(
            verify_revocation_notice(&bad_r_notice, &admin_pub),
            "invalid reason",
            "verify invalid reason",
        );
    }

    // Newline/whitespace-padded replacement write_url, read_url, or drop_pub rejected on verify
    for (label, padded_rep) in [
        (
            "padded write_url",
            SyncReplacementTarget {
                write_url: "https://new.example.org/sync.php\n".to_string(),
                read_url: "https://new.example.org/d".to_string(),
                drop_pub: rep_pub.clone(),
            },
        ),
        (
            "padded read_url",
            SyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: "https://new.example.org/d ".to_string(),
                drop_pub: rep_pub.clone(),
            },
        ),
        (
            "padded drop_pub",
            SyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: "https://new.example.org/d".to_string(),
                drop_pub: format!(" {}", rep_pub),
            },
        ),
    ] {
        let padded_msg = canonical_revocation_message(&admin_pub, 100, 200, Some(&padded_rep), None);
        let padded_notice = SyncRevocationNotice {
            drop_pub: admin_pub.clone(),
            issued_at: 100,
            effective_at: 200,
            reason: None,
            replacement: Some(padded_rep),
            signature: hex::encode(admin_sk.sign(padded_msg.as_bytes()).to_bytes()),
        };
        assert_invalid_payload_contains(
            verify_revocation_notice(&padded_notice, &admin_pub),
            "invalid whitespace or control chars",
            label,
        );
    }
}

#[tokio::test]
async fn test_s08_f5_lock_delegation_strict_verification_and_push() {
    let client = create_sync_http_client();
    let admin_sk = SigningKey::from_bytes(&[16u8; 32]);
    let admin_pub = hex::encode(admin_sk.verifying_key().as_bytes());
    let lock_sk = SigningKey::from_bytes(&[15u8; 32]);
    let lock_pub = hex::encode(lock_sk.verifying_key().as_bytes());
    let non_curve_byte = (1u8..=255)
        .find(|&b| VerifyingKey::from_bytes(&[b; 32]).is_err())
        .unwrap();
    let non_curve_hex = hex::encode([non_curve_byte; 32]);

    let valid_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_pub,
        &lock_pub,
        "https://sync.example.org/sync.php",
        "https://sync.example.org/d",
        100,
    );
    let valid_del = SyncLockDelegationDto {
        admin_pub: admin_pub.clone(),
        lock_pub: lock_pub.clone(),
        write_url: "https://sync.example.org/sync.php".to_string(),
        read_url: "https://sync.example.org/d".to_string(),
        issued_at: 100,
        sig: hex::encode(admin_sk.sign(valid_msg.as_bytes()).to_bytes()),
    };
    assert!(verify_lock_delegation(
        &lock_pub,
        &valid_del,
        Some(&admin_pub)
    ));

    let mut zero_ts_del = valid_del;
    zero_ts_del.issued_at = 0;
    assert!(!verify_lock_delegation(
        &lock_pub,
        &zero_ts_del,
        Some(&admin_pub)
    ));

    let query_read_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_pub,
        &lock_pub,
        "https://sync.example.org/sync.php",
        "https://sync.example.org/d?q=1",
        100,
    );
    let query_del = SyncLockDelegationDto {
        admin_pub: admin_pub.clone(),
        lock_pub: lock_pub.clone(),
        write_url: "https://sync.example.org/sync.php".to_string(),
        read_url: "https://sync.example.org/d?q=1".to_string(),
        issued_at: 100,
        sig: hex::encode(admin_sk.sign(query_read_msg.as_bytes()).to_bytes()),
    };
    assert!(!verify_lock_delegation(
        &lock_pub,
        &query_del,
        Some(&admin_pub)
    ));

    let ws_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_pub,
        &lock_pub,
        " https://sync.example.org/sync.php",
        "https://sync.example.org/d",
        100,
    );
    let ws_del = SyncLockDelegationDto {
        admin_pub: admin_pub.clone(),
        lock_pub: lock_pub.clone(),
        write_url: " https://sync.example.org/sync.php".to_string(),
        read_url: "https://sync.example.org/d".to_string(),
        issued_at: 100,
        sig: hex::encode(admin_sk.sign(ws_msg.as_bytes()).to_bytes()),
    };
    assert!(!verify_lock_delegation(
        &lock_pub,
        &ws_del,
        Some(&admin_pub)
    ));

    let bad_lock_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_pub,
        &non_curve_hex,
        "https://sync.example.org/sync.php",
        "https://sync.example.org/d",
        100,
    );
    let bad_lock_del = SyncLockDelegationDto {
        admin_pub: admin_pub.clone(),
        lock_pub: non_curve_hex.clone(),
        write_url: "https://sync.example.org/sync.php".to_string(),
        read_url: "https://sync.example.org/d".to_string(),
        issued_at: 100,
        sig: hex::encode(admin_sk.sign(bad_lock_msg.as_bytes()).to_bytes()),
    };
    assert!(!verify_lock_delegation(
        &non_curve_hex,
        &bad_lock_del,
        Some(&admin_pub)
    ));

    assert_local_validation_contains(
        push_lock_delegation(
            &client,
            "https://sync.example.org/sync.php",
            "https://sync.example.org/d",
            &admin_sk,
            &non_curve_hex,
            100,
        )
        .await,
        "Invalid Ed25519 lock_pub",
        "push_lock_delegation non-curve lock_pub",
    );
}

#[test]
fn test_s08_f7_ipv6_global_unicast_allowlist_and_reserved_ranges() {
    for ip_str in [
        "5f00::1",
        "2001:1::1",
        "2001:3::1",
        "2002::1",
        "3fff::1",
        "fe00::1",
        "4000::1",
        "8000::1",
        "::1",
        "192.0.0.1",
        "192.0.2.1",
        "192.88.99.1",
        "198.51.100.1",
        "203.0.113.1",
    ] {
        let ip: std::net::IpAddr = ip_str.parse().unwrap();
        assert!(
            is_blocked_ip(ip, false),
            "expected {} to be blocked when allow_loopback=false",
            ip_str
        );
    }
    let v6_loopback: std::net::IpAddr = "::1".parse().unwrap();
    assert!(!is_blocked_ip(v6_loopback, true));

    for ip_str in ["2606:4700:4700::1111", "2a01:4f8::1"] {
        let ip: std::net::IpAddr = ip_str.parse().unwrap();
        assert!(
            !is_blocked_ip(ip, false),
            "expected {} to be allowed",
            ip_str
        );
    }
}

#[tokio::test]
async fn test_s08_f10_post_lock_batch_status_outcomes() {
    let admin_sk = SigningKey::from_bytes(&[81u8; 32]);
    let valid_rev = sign_revocation_notice(
        &admin_sk,
        1_700_000_000,
        1_700_000_100,
        Some("eol".to_string()),
        None,
    )
    .unwrap();

    let batch_200_json = serde_json::to_string(&SyncLockBatchResponse {
        status: "ok".to_string(),
        bucket: [0x11u8; 32],
        retain_until_quarter: 8000,
        receipts: Vec::new(),
        conflict: None,
    })
    .unwrap();
    let batch_409_frozen_json = r#"{"error":"epoch_frozen"}"#.to_string();
    let batch_409_other_json = r#"{"error":"double_spend_conflict"}"#.to_string();
    let batch_410_json = serde_json::json!({ "revocation": valid_rev }).to_string();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let responses = [
            (200, "", batch_200_json),
            (409, "", batch_409_frozen_json),
            (409, "", batch_409_other_json),
            (410, "", batch_410_json),
            (429, "Retry-After: 23\r\n", String::new()),
            (500, "", "lock db error".to_string()),
        ];
        for (code, extra_hdrs, body) in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let req = read_request_non_empty(&mut stream).await;
            assert!(
                req.starts_with("POST /sync.php?lock "),
                "unexpected batch request line: {}",
                req.lines().next().unwrap_or("")
            );
            let resp = format!(
                "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\n\r\n{}",
                code,
                extra_hdrs,
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
        }
    });

    let client = create_sync_http_client();
    let write_url = format!("http://127.0.0.1:{}/sync.php", addr.port());
    let batch_req = SyncLockBatchRequest {
        anchor_id: [0x22u8; 32],
        bucket: [0x11u8; 32],
        recipient_nonce: [0x33u8; 32],
        entries: vec!["00".repeat(605)],
        manifest: None,
        genesis_ticket: None,
        known_head_receipt: None,
    };

    assert!(matches!(
        post_lock_batch(&client, &write_url, &batch_req)
            .await
            .unwrap(),
        SyncLockBatchOutcome::Response {
            status_code: 200,
            ..
        }
    ));
    assert!(matches!(
        post_lock_batch(&client, &write_url, &batch_req)
            .await
            .unwrap(),
        SyncLockBatchOutcome::EpochFrozen {
            status_code: 409,
            ..
        }
    ));
    assert!(matches!(
        post_lock_batch(&client, &write_url, &batch_req)
            .await
            .unwrap(),
        SyncLockBatchOutcome::Rejected {
            status_code: 409,
            ref error
        } if error == "double_spend_conflict"
    ));
    assert!(matches!(
        post_lock_batch(&client, &write_url, &batch_req)
            .await
            .unwrap(),
        SyncLockBatchOutcome::Revoked {
            status_code: 410,
            notice: Some(_)
        }
    ));
    let err_429 = post_lock_batch(&client, &write_url, &batch_req)
        .await
        .unwrap_err();
    assert_eq!(err_429.retry_after_secs(), Some(23));
    assert!(err_429.is_rate_limited());
    let err_500 = post_lock_batch(&client, &write_url, &batch_req)
        .await
        .unwrap_err();
    assert_eq!(err_500.http_status(), Some(500));

    server.await.unwrap();
}

#[tokio::test]
async fn test_s08_f10_revocation_fetch_and_delete_account_paths() {
    let admin_sk = SigningKey::from_bytes(&[81u8; 32]);
    let admin_pub = hex::encode(admin_sk.verifying_key().as_bytes());
    let valid_rev = sign_revocation_notice(
        &admin_sk,
        1_700_000_000,
        1_700_000_100,
        Some("eol".to_string()),
        None,
    )
    .unwrap();
    let mut forged_rev = valid_rev.clone();
    forged_rev.signature = "00".repeat(64);
    let valid_rev_json = serde_json::to_string(&valid_rev).unwrap();
    let forged_rev_json = serde_json::to_string(&forged_rev).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let responses = [
            (404, String::new()),
            (200, valid_rev_json),
            (200, forged_rev_json),
            (204, String::new()),
            (404, String::new()),
            (500, "delete failed".to_string()),
        ];
        for (code, body) in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let req = read_request_non_empty(&mut stream).await;
            assert!(
                req.starts_with("GET ") || req.starts_with("DELETE "),
                "unexpected request line: {}",
                req.lines().next().unwrap_or("")
            );
            let resp = format!(
                "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                code,
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
        }
    });

    let client = create_sync_http_client();
    let write_url = format!("http://127.0.0.1:{}/sync.php", addr.port());
    let read_url = format!("http://127.0.0.1:{}/d-test", addr.port());

    assert!(fetch_revocation_notice(&client, &read_url, &admin_pub)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        fetch_revocation_notice(&client, &read_url, &admin_pub)
            .await
            .unwrap()
            .unwrap()
            .drop_pub,
        admin_pub
    );
    assert_invalid_payload_contains(
        fetch_revocation_notice(&client, &read_url, &admin_pub).await,
        "Invalid revocation notice Ed25519 signature",
        "forged revocation.json signature",
    );

    let signed = SignedDropHeaders {
        pubkey_hex: admin_pub,
        prefix: None,
        timestamp: 1_700_000_000,
        nonce_hex: "aa".repeat(16),
        signature_hex: "bb".repeat(64),
    };
    delete_account(&client, &write_url, &signed).await.unwrap();
    delete_account(&client, &write_url, &signed).await.unwrap();
    assert_eq!(
        delete_account(&client, &write_url, &signed)
            .await
            .unwrap_err()
            .http_status(),
        Some(500)
    );

    server.await.unwrap();
}

#[tokio::test]
async fn test_s08_f6_pow_start_nonce_and_genesis_ticket_pow_within_i64() {
    use tokio::io::AsyncWriteExt;

    for _ in 0..64 {
        assert!(next_pow_start_nonce() <= POW_START_NONCE_MASK);
    }

    let client = create_sync_http_client();
    let lock_sk = SigningKey::from_bytes(&[82u8; 32]);
    let lock_pub = lock_sk.verifying_key().to_bytes();
    let anchor_id = [0x44u8; 32];
    let retain_q = human_money_core::humoco_proto::encode_quarter(2035, 4).unwrap();

    assert_local_validation_contains(
        request_genesis_ticket(
            &client,
            "https://sync.example.org/sync.php",
            lock_pub,
            &anchor_id,
            retain_q,
            LockTicketAuth::Pow {
                pow_bits: MAX_LOCK_POW_BITS + 1,
            },
        )
        .await,
        "Unsupported GenesisTicket PoW bits",
        "pow_bits above MAX_LOCK_POW_BITS",
    );

    let pow_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pow_addr = pow_listener.local_addr().unwrap();
    let pow_server = tokio::spawn(async move {
        let (mut stream, _) = pow_listener.accept().await.unwrap();
        let raw = read_request_non_empty(&mut stream).await;
        let (_, body) = raw.split_once("\r\n\r\n").unwrap();
        let req_json: serde_json::Value = serde_json::from_str(body).unwrap();
        let pn = req_json["pow_nonce"].as_u64().unwrap();
        assert!(
            pn <= i64::MAX as u64,
            "pow_nonce {} must fit signed i64::MAX for PHP json_decode",
            pn
        );
        let mut ticket = GenesisTicket {
            anchor_id,
            retain_until_quarter: retain_q,
            issued_at: 1_760_000_000,
            lock_pub,
            sig: [0u8; 64],
        };
        ticket.sig = lock_sk.sign(&ticket.digest()).to_bytes();
        let resp_body = serde_json::json!({ "ticket": ticket }).to_string();
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            resp_body.len(),
            resp_body
        );
        stream.write_all(resp.as_bytes()).await.unwrap();
    });

    let pow_write_url = format!("http://127.0.0.1:{}/sync.php", pow_addr.port());
    let pow_ticket = request_genesis_ticket(
        &client,
        &pow_write_url,
        lock_pub,
        &anchor_id,
        retain_q,
        LockTicketAuth::Pow { pow_bits: 1 },
    )
    .await
    .unwrap();
    assert!(pow_ticket.verify());
    pow_server.await.unwrap();
}

#[test]
fn s08_followups_ticket_pow_bits_required_and_origin_key_canonical() {
    // S08-R1: SyncLockInfoDto rejects missing or null ticket_pow_bits (no pre-SYNC-011 default).
    let missing = serde_json::json!({
        "lock_pub": "aa".repeat(32),
        "max_retention_quarter": 8159,
        "pow_base_bits": 8,
        "allow_pow_tickets": true
    });
    let err_missing = serde_json::from_value::<SyncLockInfoDto>(missing)
        .expect_err("missing ticket_pow_bits must fail deserialization")
        .to_string();
    assert!(
        err_missing.contains("missing field `ticket_pow_bits`"),
        "unexpected serde error for missing field: {}",
        err_missing
    );

    let null_val = serde_json::json!({
        "lock_pub": "aa".repeat(32),
        "max_retention_quarter": 8159,
        "pow_base_bits": 8,
        "allow_pow_tickets": true,
        "ticket_pow_bits": null
    });
    let err_null = serde_json::from_value::<SyncLockInfoDto>(null_val)
        .expect_err("null ticket_pow_bits must fail deserialization")
        .to_string();
    assert!(
        err_null.contains("invalid type: null"),
        "unexpected serde error for null field: {}",
        err_null
    );

    let valid = serde_json::json!({
        "lock_pub": "aa".repeat(32),
        "max_retention_quarter": 8159,
        "pow_base_bits": 8,
        "allow_pow_tickets": true,
        "ticket_pow_bits": 16
    });
    let dto = serde_json::from_value::<SyncLockInfoDto>(valid).unwrap();
    assert_eq!(dto.ticket_pow_bits, 16);

    // S08-Q3: canonical origin extraction groups paths/ports accurately and ignores invalid URLs.
    assert_eq!(
        sync_origin_key("HTTPS://Sync.Example.ORG:443/a/sync.php"),
        Some("https://sync.example.org:443".to_string())
    );
    assert_eq!(
        sync_origin_key("https://sync.example.org/b/sync.php"),
        Some("https://sync.example.org:443".to_string())
    );
    assert_eq!(
        sync_origin_key("http://127.0.0.1:8080/sync.php"),
        Some("http://127.0.0.1:8080".to_string())
    );
    assert_eq!(sync_origin_key("ftp://sync.example.org/sync.php"), None);
    assert_eq!(sync_origin_key("not a url"), None);
}

#[tokio::test]
async fn s08_q1_sync_clients_disable_proxies_and_block_ssrf_targets() {
    let http_client = create_sync_http_client();
    let probe_client = create_sync_probe_client();

    for client in [&http_client, &probe_client] {
        assert_local_validation_contains(
            fetch_sync_info(client, "http://10.255.255.1/sync.php").await,
            "blocked internal/private address",
            "RFC1918 SSRF target",
        );
        assert_local_validation_contains(
            fetch_sync_info(client, "https://169.254.169.254/latest/meta-data").await,
            "blocked internal/private address",
            "link-local metadata SSRF target",
        );
    }

    // Strict SsrfSafeDnsResolver (allow_loopback = false) blocks resolved loopback and RFC1918 IPs
    let err_loopback = reqwest::dns::Resolve::resolve(
        &SsrfSafeDnsResolver::new(false),
        "localhost".parse().unwrap(),
    )
    .await
    .err()
    .expect("loopback host must be blocked when allow_loopback=false");
    assert!(
        err_loopback.to_string().contains("SSRF guard blocked"),
        "Unexpected DNS resolver error: {}",
        err_loopback
    );

    let err_private = filter_resolved_addrs(
        vec!["10.255.255.1:443".parse().unwrap()],
        "evil.example.org",
        false,
    )
    .unwrap_err();
    assert_eq!(err_private.kind(), std::io::ErrorKind::PermissionDenied);
}

#[tokio::test]
async fn s11_x4_get_media_blob_aborts_oversized_chunked_response() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let req = read_request_non_empty(&mut stream).await;
        assert!(req.starts_with("GET "));
        let chunk = vec![0x5Au8; 16 * 1024];
        let chunk_hdr = format!("{:x}\r\n", chunk.len());
        if write_allow_early_close(
            &mut stream,
            b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nTransfer-Encoding: chunked\r\n\r\n",
        )
        .await
        {
            for _ in 0..13 {
                if !write_allow_early_close(&mut stream, chunk_hdr.as_bytes()).await
                    || !write_allow_early_close(&mut stream, &chunk).await
                    || !write_allow_early_close(&mut stream, b"\r\n").await
                {
                    break;
                }
            }
            if write_allow_early_close(&mut stream, b"0\r\n\r\n").await {
                use tokio::io::AsyncWriteExt;
                stream.flush().await.unwrap_or(());
            }
        }
    });

    let client = create_sync_http_client();
    let read_url = format!("http://127.0.0.1:{}/d-test", addr.port());
    let blob_id = "cc".repeat(32);
    let err = get_media_blob(&client, &read_url, &blob_id)
        .await
        .expect_err("oversized chunked media response (> MAX_MEDIA_BLOB_BODY_BYTES) must abort");
    assert!(
        matches!(
            err,
            SyncClientError::ResponseTooLarge {
                max_bytes: MAX_MEDIA_BLOB_BODY_BYTES,
                ..
            }
        ),
        "expected ResponseTooLarge with MAX_MEDIA_BLOB_BODY_BYTES, got: {:?}",
        err
    );
    server.await.unwrap();
}

#[test]
fn s11_x4_worst_case_sealed_media_fits_within_media_slot_limit() {
    let sk = SigningKey::from_bytes(&[91u8; 32]);
    let owner_did = human_money_core::crypto::create_user_id(&sk.verifying_key(), None).unwrap();
    let mut max_png = vec![0u8; humoco_market::MAX_AD_MEDIA_BYTES];
    // Minimal valid 1x1 PNG header + IHDR chunk (33 bytes) padded to MAX_AD_MEDIA_BYTES.
    max_png[0..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    max_png[8..12].copy_from_slice(&13u32.to_be_bytes());
    max_png[12..16].copy_from_slice(b"IHDR");
    max_png[16..20].copy_from_slice(&1u32.to_be_bytes());
    max_png[20..24].copy_from_slice(&1u32.to_be_bytes());
    let (_token, _blob_id, sealed) = humoco_market::seal_media_blob(&owner_did, &max_png).unwrap();
    assert!(
        sealed.len() <= MAX_MEDIA_BLOB_BODY_BYTES,
        "sealed max-size media blob ({} B) exceeds MAX_MEDIA_BLOB_BODY_BYTES ({} B)",
        sealed.len(),
        MAX_MEDIA_BLOB_BODY_BYTES
    );
}

#[tokio::test]
async fn test_fetch_text_bounded_handles_success_ssrf_http_size_and_utf8() {
    use tokio::io::AsyncWriteExt;

    let client = create_sync_http_client();

    // 1. SSRF / LocalValidation
    assert_local_validation_contains(
        fetch_text_bounded(
            &client,
            "http://10.0.0.1/provider.json",
            1024,
            "provider filter",
        )
        .await,
        "blocked internal/private address",
        "SSRF provider filter URL",
    );

    // 2. Local server responses: 200 UTF-8, 429, chunked > max_bytes, invalid UTF-8
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let responses: Vec<(bool, Vec<u8>)> = vec![
            (
                false,
                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}".to_vec(),
            ),
            (
                false,
                b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 45\r\nContent-Length: 4\r\n\r\nslow"
                    .to_vec(),
            ),
            (
                true,
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10\r\n0123456789abcdef\r\n10\r\n0123456789abcdef\r\n0\r\n\r\n"
                    .to_vec(),
            ),
            (
                false,
                b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\n\xff\xfe\xfd".to_vec(),
            ),
        ];
        for (allow_close, body) in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let req = read_request_non_empty(&mut stream).await;
            assert!(req.starts_with("GET "));
            if allow_close {
                if write_allow_early_close(&mut stream, &body).await {
                    stream.flush().await.unwrap_or(());
                }
            } else {
                stream.write_all(&body).await.unwrap();
            }
        }
    });

    let url = format!("http://127.0.0.1:{}/provider.json", addr.port());

    let ok_body = fetch_text_bounded(&client, &url, 20, "provider filter")
        .await
        .unwrap();
    assert_eq!(ok_body, "{\"ok\":true}");

    let err_429 = fetch_text_bounded(&client, &url, 20, "provider filter")
        .await
        .unwrap_err();
    assert!(err_429.is_rate_limited());
    assert_eq!(err_429.retry_after_secs(), Some(45));

    let err_large = fetch_text_bounded(&client, &url, 20, "provider filter")
        .await
        .unwrap_err();
    assert!(matches!(
        err_large,
        SyncClientError::ResponseTooLarge { max_bytes: 20, .. }
    ));

    let err_utf8 = fetch_text_bounded(&client, &url, 20, "provider filter")
        .await
        .unwrap_err();
    assert!(matches!(err_utf8, SyncClientError::InvalidPayload(_)));

    server.await.unwrap();
}

#[tokio::test]
async fn test_weak_small_order_ed25519_keys_rejected_in_revocation_and_delegation() {
    let admin_sk = SigningKey::from_bytes(&[71u8; 32]);
    let admin_pub = hex::encode(admin_sk.verifying_key().as_bytes());
    let valid_target_pub = hex::encode(SigningKey::from_bytes(&[72u8; 32]).verifying_key().as_bytes());
    let zero_key = "00".repeat(32);
    let order1_key = {
        let mut b = [0u8; 32];
        b[0] = 1;
        hex::encode(b)
    };
    let client = create_sync_http_client();
    let write_url = "https://sync.example.org/sync.php";
    let read_url = "https://sync.example.org/d";

    let valid_rep = SyncReplacementTarget {
        write_url: "https://new.example.org/sync.php".to_string(),
        read_url: "https://new.example.org/d".to_string(),
        drop_pub: valid_target_pub.clone(),
    };
    let valid_notice = sign_revocation_notice(
        &admin_sk,
        1_700_000_000,
        1_700_000_100,
        None,
        Some(valid_rep),
    )
    .expect("valid replacement drop_pub must succeed");
    verify_revocation_notice(&valid_notice, &admin_pub)
        .expect("valid revocation notice must verify");
    assert_eq!(valid_notice.drop_pub, admin_pub);

    let valid_del_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_pub,
        &valid_target_pub,
        write_url,
        read_url,
        1_700_000_000,
    );
    let valid_del = SyncLockDelegationDto {
        admin_pub: admin_pub.clone(),
        lock_pub: valid_target_pub.clone(),
        write_url: write_url.to_string(),
        read_url: read_url.to_string(),
        issued_at: 1_700_000_000,
        sig: hex::encode(admin_sk.sign(valid_del_msg.as_bytes()).to_bytes()),
    };
    assert!(verify_lock_delegation(
        &valid_target_pub,
        &valid_del,
        Some(&admin_pub)
    ));

    for weak_pub in [&zero_key, &order1_key] {
        assert_local_validation_contains(
            sign_revocation_notice(
                &admin_sk,
                1_700_000_000,
                1_700_000_100,
                None,
                Some(SyncReplacementTarget {
                    write_url: "https://new.example.org/sync.php".to_string(),
                    read_url: "https://new.example.org/d".to_string(),
                    drop_pub: weak_pub.clone(),
                }),
            ),
            "weak small-order",
            "sign_revocation_notice with weak drop_pub",
        );

        let weak_rep = SyncReplacementTarget {
            write_url: "https://new.example.org/sync.php".to_string(),
            read_url: "https://new.example.org/d".to_string(),
            drop_pub: weak_pub.clone(),
        };
        let weak_msg = canonical_revocation_message(
            &admin_pub,
            1_700_000_000,
            1_700_000_100,
            Some(&weak_rep),
            None,
        );
        let weak_notice = SyncRevocationNotice {
            drop_pub: admin_pub.clone(),
            issued_at: 1_700_000_000,
            effective_at: 1_700_000_100,
            reason: None,
            replacement: Some(weak_rep),
            signature: hex::encode(admin_sk.sign(weak_msg.as_bytes()).to_bytes()),
        };
        assert_invalid_payload_contains(
            verify_revocation_notice(&weak_notice, &admin_pub),
            "invalid replacement drop_pub",
            "verify_revocation_notice with weak drop_pub",
        );

        let weak_del_msg = humoco_market::format_lock_delegate_signature_message(
            &admin_pub,
            weak_pub,
            write_url,
            read_url,
            1_700_000_000,
        );
        let weak_del = SyncLockDelegationDto {
            admin_pub: admin_pub.clone(),
            lock_pub: weak_pub.clone(),
            write_url: write_url.to_string(),
            read_url: read_url.to_string(),
            issued_at: 1_700_000_000,
            sig: hex::encode(admin_sk.sign(weak_del_msg.as_bytes()).to_bytes()),
        };
        assert!(!verify_lock_delegation(weak_pub, &weak_del, Some(&admin_pub)));

        assert_local_validation_contains(
            push_lock_delegation(
                &client,
                write_url,
                read_url,
                &admin_sk,
                weak_pub,
                1_700_000_000,
            )
            .await,
            "Invalid Ed25519 lock_pub",
            "push_lock_delegation with weak lock_pub",
        );
    }
}

#[test]
fn test_validate_static_lock_rel_path_and_anchor_candidates() {
    assert_eq!(
        validate_static_lock_rel_path("/locks/2027-Q4/aa/present.lock").unwrap(),
        "2027-Q4/aa/present.lock"
    );
    assert_eq!(
        validate_static_lock_rel_path("2027-Q4/aa/present.lock").unwrap(),
        "2027-Q4/aa/present.lock"
    );
    for bad in [
        "",
        "locks/",
        "2027-Q4//present.lock",
        "2027-Q4/./present.lock",
        "2027-Q4/aa/present.bin",
        &format!("a/{}.lock", "b".repeat(260)),
    ] {
        assert_local_validation_contains(
            validate_static_lock_rel_path(bad),
            "Invalid static lock path",
            bad,
        );
    }

    let aid = "ab".repeat(32);
    assert_eq!(
        anchor_manifest_candidate_urls("https://sync.example.org/d/locks/", &aid).unwrap(),
        vec![
            format!("https://sync.example.org/d/anchors/{}.bin", aid),
            format!("https://sync.example.org/anchors/{}.bin", aid),
        ]
    );
    assert_eq!(
        anchor_manifest_candidate_urls("https://sync.example.org/", &aid).unwrap(),
        vec![format!("https://sync.example.org/anchors/{}.bin", aid)]
    );
    assert_local_validation_contains(
        anchor_manifest_candidate_urls("https://sync.example.org/d?q=1", &aid),
        "must not contain a query string",
        "non-sync.php read URL with query",
    );
}
