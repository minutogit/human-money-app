use super::ssrf::{validate_sync_read_url, validate_sync_url};
use super::*;
use ed25519_dalek::SigningKey;
use humoco_market::SignedDropHeaders;
use serde::Serialize;

pub fn generate_sync_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    hex::encode(bytes)
}

pub fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn normalize_etag_token(raw: &str) -> Option<String> {
    let s = raw.trim();
    let s = s
        .strip_prefix("W/")
        .or_else(|| s.strip_prefix("w/"))
        .unwrap_or(s)
        .trim();
    let s = s.trim_matches('"').trim();
    if s.is_empty() || s.chars().any(|c| c.is_control() || c == '"') {
        return None;
    }
    Some(s.to_string())
}

pub fn extract_etag(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .and_then(normalize_etag_token)
}

pub fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> u64 {
    if let Some(val) = headers.get(reqwest::header::RETRY_AFTER) {
        if let Ok(s) = val.to_str() {
            if let Ok(secs) = s.trim().parse::<u64>() {
                return secs.clamp(MIN_RETRY_AFTER_SECS, MAX_RETRY_AFTER_SECS);
            }
        }
    }
    60
}

/// Validates `url` against SSRF rules and returns the trimmed slice.
pub(super) fn validated_sync_url(url: &str) -> Result<&str, SyncClientError> {
    validate_sync_url(url)?;
    Ok(url.trim())
}

/// Validates `base_url` against SSRF rules and appends `query` using `?` or `&`.
pub(super) fn append_sync_query(base_url: &str, query: &str) -> Result<String, SyncClientError> {
    let clean = validated_sync_url(base_url)?;
    let sep = if clean.contains('?') { "&" } else { "?" };
    Ok(format!("{}{}{}", clean, sep, query))
}

pub(super) fn is_lower_hex_exact(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub(super) fn is_valid_sync_channel(channel: &str) -> bool {
    !channel.is_empty()
        && channel.len() <= 32
        && channel
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

pub(super) fn validate_static_lock_rel_path(rel_path: &str) -> Result<String, SyncClientError> {
    let clean_rel = rel_path.trim().trim_start_matches('/');
    let clean_rel = clean_rel.strip_prefix("locks/").unwrap_or(clean_rel);
    if clean_rel.is_empty()
        || clean_rel.len() > 256
        || !clean_rel.ends_with(".lock")
        || clean_rel
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '?' | '#' | '\\' | '%'))
    {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid static lock path '{}'",
            rel_path
        )));
    }
    for seg in clean_rel.split('/') {
        if seg.is_empty()
            || seg == "."
            || seg == ".."
            || !seg
                .bytes()
                .all(|b| matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
        {
            return Err(SyncClientError::LocalValidation(format!(
                "Invalid static lock path '{}'",
                rel_path
            )));
        }
    }
    Ok(clean_rel.to_string())
}

/// Validates `base_url` as a read URL (no query string) and appends a relative path segment.
pub(super) fn append_sync_path(base_url: &str, path: &str) -> Result<String, SyncClientError> {
    validate_sync_read_url(base_url)?;
    let clean = base_url.trim().trim_end_matches('/');
    let rel = path.trim_start_matches('/');
    Ok(format!("{}/{}", clean, rel))
}

/// Attaches `X-Drop-*` authentication headers (and optionally `X-Drop-Prefix`) to `req`.
pub(super) fn apply_signed_headers(
    req: reqwest::RequestBuilder,
    headers: &SignedDropHeaders,
    include_prefix: bool,
) -> reqwest::RequestBuilder {
    let mut req = req
        .header("X-Drop-Pubkey", &headers.pubkey_hex)
        .header("X-Drop-Timestamp", headers.timestamp.to_string())
        .header("X-Drop-Nonce", &headers.nonce_hex)
        .header("X-Drop-Signature", &headers.signature_hex);
    if include_prefix {
        if let Some(ref prefix) = headers.prefix {
            if !prefix.is_empty() {
                req = req.header("X-Drop-Prefix", prefix);
            }
        }
    }
    req
}

/// Signs an admin operation with a fresh timestamp/nonce and attaches `X-Drop-*` headers.
pub(super) fn apply_admin_headers(
    req: reqwest::RequestBuilder,
    admin_sk: &SigningKey,
    action: &str,
    channel: &str,
    body: &[u8],
) -> reqwest::RequestBuilder {
    let timestamp = current_unix_timestamp();
    let nonce = generate_sync_nonce();
    let signed =
        humoco_market::sign_admin_request(admin_sk, action, channel, timestamp, &nonce, body);
    apply_signed_headers(req, &signed, false)
}

/// Parses a trimmed ASCII header value into `T`, returning `None` if missing or invalid.
pub(super) fn parse_header<T: std::str::FromStr>(
    headers: &reqwest::header::HeaderMap,
    name: &str,
) -> Option<T> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<T>().ok())
}

/// Reads an HTTP response body up to `max_bytes`, failing closed on oversized `Content-Length`
/// or streaming chunks exceeding the cap.
pub(super) async fn read_response_bytes_bounded(
    mut resp: reqwest::Response,
    max_bytes: usize,
    op: &str,
) -> Result<Vec<u8>, SyncClientError> {
    if let Some(len) = resp.content_length() {
        if len > max_bytes as u64 {
            return Err(SyncClientError::ResponseTooLarge {
                max_bytes,
                context: op.to_string(),
            });
        }
    }
    let initial_cap = resp
        .content_length()
        .map(|l| (l as usize).min(max_bytes).min(64 * 1024))
        .unwrap_or(0);
    let mut buf = Vec::with_capacity(initial_cap);
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| SyncClientError::Transport(format!("Failed reading response {}: {}", op, e)))?
    {
        if buf.len().saturating_add(chunk.len()) > max_bytes {
            return Err(SyncClientError::ResponseTooLarge {
                max_bytes,
                context: op.to_string(),
            });
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Streams a bounded HTTP response body up to `max_bytes` and parses it as JSON.
pub(super) async fn read_response_json_bounded<T: serde::de::DeserializeOwned>(
    resp: reqwest::Response,
    max_bytes: usize,
    op: &str,
    parse_ctx: &str,
) -> Result<T, SyncClientError> {
    let bytes = read_response_bytes_bounded(resp, max_bytes, op).await?;
    serde_json::from_slice(&bytes)
        .map_err(|e| SyncClientError::InvalidPayload(format!("{}: {}", parse_ctx, e)))
}

/// Sanitizes an untrusted error snippet by stripping control chars, collapsing whitespace, and
/// bounding length to `MAX_ERROR_DETAIL_CHARS`.
pub(super) fn sanitize_error_snippet(bytes: &[u8]) -> String {
    let raw = String::from_utf8_lossy(bytes);
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(MAX_ERROR_DETAIL_CHARS).collect()
}

/// Reads at most `max_bytes` from an error HTTP response and sanitizes the snippet for logs/UI.
pub(super) async fn read_error_snippet_bounded(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> String {
    let mut buf = Vec::new();
    while buf.len() < max_bytes {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                let remaining = max_bytes - buf.len();
                let take = chunk.len().min(remaining);
                buf.extend_from_slice(&chunk[..take]);
                if chunk.len() > remaining {
                    break;
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
    sanitize_error_snippet(&buf)
}

/// Constructs an HTTP 429 `SyncClientError::RateLimited` including the clamped `Retry-After` duration.
pub(super) fn rate_limit_error(op: &str, headers: &reqwest::header::HeaderMap) -> SyncClientError {
    let secs = parse_retry_after(headers);
    SyncClientError::RateLimited {
        status: 429,
        retry_after_secs: secs,
        context: op.to_string(),
        snippet: String::new(),
    }
}

/// Converts a non-success HTTP response into a typed `SyncClientError`.
pub(super) async fn error_from_response(resp: reqwest::Response, op: &str) -> SyncClientError {
    let status = resp.status().as_u16();
    if status == 429 {
        return rate_limit_error(op, resp.headers());
    }
    let detail = read_error_snippet_bounded(resp, MAX_ERROR_BODY_BYTES).await;
    SyncClientError::HttpStatus {
        status,
        retry_after_secs: None,
        context: op.to_string(),
        snippet: detail,
    }
}

/// Returns `Ok(resp)` for 2xx responses, or a typed `SyncClientError`.
pub(super) async fn map_unexpected_status(
    resp: reqwest::Response,
    op: &str,
) -> Result<reqwest::Response, SyncClientError> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    Err(error_from_response(resp, op).await)
}

/// Executes `req`, mapping transport failures and non-2xx responses with `op` context.
pub(super) async fn send_sync_request(
    req: reqwest::RequestBuilder,
    op: &str,
) -> Result<reqwest::Response, SyncClientError> {
    let resp = req
        .send()
        .await
        .map_err(|e| SyncClientError::Transport(format!("Network error {}: {}", op, e)))?;
    map_unexpected_status(resp, op).await
}

/// Executes a `DELETE` request treating HTTP 200, 204, and 404 (idempotent delete) as success.
pub(super) async fn send_delete_request(
    req: reqwest::RequestBuilder,
    op: &str,
) -> Result<(), SyncClientError> {
    let resp = req
        .send()
        .await
        .map_err(|e| SyncClientError::Transport(format!("Network error {}: {}", op, e)))?;
    match resp.status().as_u16() {
        200 | 204 | 404 => Ok(()),
        _ => Err(error_from_response(resp, op).await),
    }
}

/// Handles `PUT` responses for slot and media uploads, preserving 412, 413, 429, and 507 semantics.
pub(super) async fn handle_put_slot_response(
    resp: reqwest::Response,
    target: &str,
    op: &str,
) -> Result<PutSlotOutcome, SyncClientError> {
    match resp.status().as_u16() {
        200 | 201 | 204 => {
            let header_etag = extract_etag(resp.headers());
            let body_bytes = read_response_bytes_bounded(resp, MAX_JSON_META_BODY_BYTES, op)
                .await
                .unwrap_or_default();
            let parsed_json = serde_json::from_slice::<serde_json::Value>(&body_bytes).ok();
            let body_etag = parsed_json
                .as_ref()
                .and_then(|v| v.get("etag").and_then(|e| e.as_str()))
                .and_then(normalize_etag_token);
            let slot_ack = parsed_json
                .as_ref()
                .and_then(|v| v.get("slot_ack"))
                .and_then(|ack| serde_json::from_value::<SlotAckReceipt>(ack.clone()).ok());
            Ok(PutSlotOutcome::Saved {
                etag: header_etag.or(body_etag),
                slot_ack,
            })
        }
        412 => Ok(PutSlotOutcome::PreconditionFailed {
            current_etag: extract_etag(resp.headers()),
        }),
        413 => Err(SyncClientError::HttpStatus {
            status: 413,
            retry_after_secs: None,
            context: format!("Payload too large: sync server rejected {} size", target),
            snippet: String::new(),
        }),
        507 => Err(SyncClientError::HttpStatus {
            status: 507,
            retry_after_secs: None,
            context: "Insufficient storage: sync server quota exceeded".to_string(),
            snippet: String::new(),
        }),
        _ => Err(error_from_response(resp, op).await),
    }
}

/// Validates `write_url`, sends a JSON `POST` to `?query`, and checks for non-2xx status.
pub(super) async fn post_sync_json<T: Serialize + ?Sized>(
    client: &reqwest::Client,
    write_url: &str,
    query: &str,
    payload: &T,
    op: &str,
) -> Result<reqwest::Response, SyncClientError> {
    let url = append_sync_query(write_url, query)?;
    send_sync_request(client.post(&url).json(payload), op).await
}
