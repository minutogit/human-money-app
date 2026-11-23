use super::http::{
    append_sync_path, append_sync_query, apply_admin_headers, apply_signed_headers,
    handle_put_slot_response, is_lower_hex_exact, is_valid_sync_channel, map_unexpected_status,
    normalize_etag_token, parse_header, rate_limit_error, read_response_bytes_bounded,
    read_response_json_bounded, send_delete_request, send_sync_request, validated_sync_url,
};
use super::ssrf::validate_sync_url;
use super::*;
use ed25519_dalek::SigningKey;
use humoco_market::SignedDropHeaders;

pub async fn fetch_sync_info(
    client: &reqwest::Client,
    write_url: &str,
) -> Result<SyncServerInfo, SyncClientError> {
    let url = append_sync_query(write_url, "info")?;
    let resp = send_sync_request(
        client.get(&url),
        &format!("fetching sync info from {}", write_url),
    )
    .await?;

    let mut info: SyncServerInfo = read_response_json_bounded(
        resp,
        MAX_JSON_META_BODY_BYTES,
        "fetching sync info",
        "Failed to parse sync info JSON",
    )
    .await?;

    info.register_url = info
        .register_url
        .take()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty() && validate_sync_url(u).is_ok());

    Ok(info)
}

pub async fn head_slot_capability(
    client: &reqwest::Client,
    write_url: &str,
    pubkey_hex64: &str,
    prefix: Option<&str>,
    channel: &str,
) -> Result<SyncCapabilityResult, SyncClientError> {
    let clean_url = validated_sync_url(write_url)?;
    if !is_valid_sync_channel(channel) {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid sync channel '{}'",
            channel
        )));
    }
    let clean_prefix = humoco_market::validate_sync_prefix(prefix)
        .map_err(|e| SyncClientError::LocalValidation(format!("Invalid slot prefix: {}", e)))?;

    let mut req = client
        .head(clean_url)
        .header("X-Drop-Pubkey", pubkey_hex64)
        .header("X-Channel", channel);

    if let Some(p) = clean_prefix.as_deref() {
        req = req.header("X-Drop-Prefix", p);
    }

    let resp = req.send().await.map_err(|e| {
        SyncClientError::Transport(format!(
            "Network error checking capability on {}: {}",
            write_url, e
        ))
    })?;

    let status = resp.status().as_u16();
    if status == 429 {
        return Err(rate_limit_error("by sync server", resp.headers()));
    }
    if status != 200 {
        return Err(SyncClientError::HttpStatus {
            status,
            retry_after_secs: None,
            context: "on slot capability check".to_string(),
            snippet: String::new(),
        });
    }

    let headers = resp.headers();
    let authorized = headers
        .get("x-drop-authorized")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    Ok(SyncCapabilityResult {
        authorized,
        max_bytes: parse_header(headers, "x-channel-max-bytes"),
        ttl_days: parse_header(headers, "x-channel-ttl-days"),
        slot_etag: extract_etag(headers),
        media_max_count: parse_header(headers, "x-media-max-count"),
        media_max_user_bytes: parse_header(headers, "x-media-max-bytes"),
        media_used_count: parse_header(headers, "x-media-count"),
        media_used_bytes: parse_header(headers, "x-media-bytes"),
    })
}

pub async fn put_slot(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
    channel: &str,
    blob_bytes: &[u8],
    if_match: Option<&str>,
) -> Result<PutSlotOutcome, SyncClientError> {
    let clean_url = validated_sync_url(write_url)?;
    if !is_valid_sync_channel(channel) {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid sync channel '{}'",
            channel
        )));
    }
    let mut req = apply_signed_headers(client.put(clean_url), signed_headers, true)
        .header("X-Channel", channel)
        .header("Content-Type", "application/octet-stream");

    if let Some(im) = if_match {
        if im == "none" {
            req = req.header("If-Match", "none");
        } else {
            let clean = normalize_etag_token(im).ok_or_else(|| {
                SyncClientError::LocalValidation("Invalid If-Match ETag".to_string())
            })?;
            req = req.header("If-Match", format!("\"{}\"", clean));
        }
    }

    let resp =
        req.body(blob_bytes.to_vec()).send().await.map_err(|e| {
            SyncClientError::Transport(format!("Network error sending slot: {}", e))
        })?;

    handle_put_slot_response(resp, "slot", "on slot PUT").await
}

pub async fn delete_slot(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
    channel: &str,
) -> Result<(), SyncClientError> {
    let clean_url = validated_sync_url(write_url)?;
    if !is_valid_sync_channel(channel) {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid sync channel '{}'",
            channel
        )));
    }
    let req = apply_signed_headers(client.delete(clean_url), signed_headers, true)
        .header("X-Channel", channel);
    send_delete_request(req, "deleting slot").await
}

pub async fn delete_account(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
) -> Result<(), SyncClientError> {
    let url = append_sync_query(write_url, "account")?;
    let req = apply_signed_headers(client.delete(&url), signed_headers, false);
    send_delete_request(req, "deleting account").await
}

pub async fn put_media_blob(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
    blob_id: &str,
    blob_bytes: &[u8],
) -> Result<PutSlotOutcome, SyncClientError> {
    let clean_url = validated_sync_url(write_url)?;
    if !is_lower_hex_exact(blob_id, 64) {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid blob_id '{}': expected 64 lowercase hex chars",
            blob_id
        )));
    }
    let resp = apply_signed_headers(client.put(clean_url), signed_headers, false)
        .header("X-Channel", "media")
        .header("X-Media-Id", blob_id)
        .header("Content-Type", "application/octet-stream")
        .body(blob_bytes.to_vec())
        .send()
        .await
        .map_err(|e| {
            SyncClientError::Transport(format!("Network error sending media blob: {}", e))
        })?;

    handle_put_slot_response(resp, "media", "on media PUT").await
}

pub async fn delete_media_blob(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
    blob_id: Option<&str>,
) -> Result<(), SyncClientError> {
    let clean_url = validated_sync_url(write_url)?;
    if let Some(id) = blob_id {
        if !is_lower_hex_exact(id, 64) {
            return Err(SyncClientError::LocalValidation(format!(
                "Invalid blob_id '{}': expected 64 lowercase hex chars",
                id
            )));
        }
    }
    let mut req = apply_signed_headers(client.delete(clean_url), signed_headers, false)
        .header("X-Channel", "media");
    if let Some(id) = blob_id {
        req = req.header("X-Media-Id", id);
    }
    send_delete_request(req, "deleting media blob").await
}

pub async fn push_members(
    client: &reqwest::Client,
    write_url: &str,
    admin_sk: &SigningKey,
    channel: &str,
    pubkeys: &[String],
) -> Result<(), SyncClientError> {
    if !is_valid_sync_channel(channel) {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid sync channel '{}'",
            channel
        )));
    }
    let url = append_sync_query(write_url, &format!("members&channel={}", channel))?;
    let body_json =
        serde_json::to_string(&serde_json::json!({ "pubkeys": pubkeys })).map_err(|e| {
            SyncClientError::LocalValidation(format!("Failed to serialize members payload: {}", e))
        })?;

    let req = apply_admin_headers(
        client.put(&url),
        admin_sk,
        "PUT_MEMBERS",
        channel,
        body_json.as_bytes(),
    )
    .header("Content-Type", "application/json")
    .body(body_json);

    send_sync_request(req, "pushing members").await?;
    Ok(())
}

pub async fn fetch_admin_status(
    client: &reqwest::Client,
    write_url: &str,
    admin_sk: &SigningKey,
) -> Result<SyncServerAdminStatus, SyncClientError> {
    let url = append_sync_query(write_url, "status")?;
    let req = apply_admin_headers(client.get(&url), admin_sk, "STATUS", "", b"");
    let resp = send_sync_request(req, "fetching admin status").await?;
    read_response_json_bounded(
        resp,
        MAX_JSON_META_BODY_BYTES,
        "fetching admin status",
        "Failed to parse admin status JSON",
    )
    .await
}

pub async fn put_admin_config(
    client: &reqwest::Client,
    write_url: &str,
    admin_sk: &SigningKey,
    max_total_storage_mb: usize,
    max_members: usize,
    max_user_storage_mb: usize,
) -> Result<SyncServerAdminStatus, SyncClientError> {
    let url = append_sync_query(write_url, "admin_config")?;
    let body_json = serde_json::to_string(&serde_json::json!({
        "max_total_storage_mb": max_total_storage_mb,
        "max_members": max_members,
        "max_user_storage_mb": max_user_storage_mb,
    }))
    .map_err(|e| {
        SyncClientError::LocalValidation(format!("Failed to serialize admin_config payload: {}", e))
    })?;

    let req = apply_admin_headers(
        client.put(&url),
        admin_sk,
        "PUT_ADMIN_CONFIG",
        "",
        body_json.as_bytes(),
    )
    .header("Content-Type", "application/json")
    .body(body_json);

    let resp = send_sync_request(req, "pushing admin_config").await?;
    read_response_json_bounded(
        resp,
        MAX_JSON_META_BODY_BYTES,
        "pushing admin_config",
        "Failed to parse admin_config response JSON",
    )
    .await
}

pub async fn list_index(
    client: &reqwest::Client,
    read_url: &str,
    cached_etag: Option<&str>,
    cached_last_modified: Option<&str>,
) -> Result<IndexFetchResult, SyncClientError> {
    let url = append_sync_path(read_url, "index.json")?;
    let mut req = client.get(&url);
    if let Some(etag) = cached_etag.and_then(normalize_etag_token) {
        req = req.header("If-None-Match", format!("\"{}\"", etag));
    }
    if let Some(lm) = cached_last_modified
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.chars().any(|c| c.is_control()))
    {
        req = req.header("If-Modified-Since", lm);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| SyncClientError::Transport(format!("Network error fetching index: {}", e)))?;

    match resp.status().as_u16() {
        304 => return Ok(IndexFetchResult::NotModified),
        404 => {
            return Ok(IndexFetchResult::Modified {
                entries: Vec::new(),
                etag: None,
                last_modified: None,
            })
        }
        _ => {}
    }

    let resp = map_unexpected_status(resp, "fetching index").await?;
    let new_etag = extract_etag(resp.headers());
    let new_last_modified = resp
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let entries: Vec<SyncIndexEntry> = read_response_json_bounded(
        resp,
        MAX_INDEX_JSON_BODY_BYTES,
        "fetching index",
        "Failed to parse index.json",
    )
    .await?;

    Ok(IndexFetchResult::Modified {
        entries,
        etag: new_etag,
        last_modified: new_last_modified,
    })
}

pub async fn get_slot_bounded(
    client: &reqwest::Client,
    read_url: &str,
    id: &str,
    prefix: Option<&str>,
    channel: &str,
    cached_etag: Option<&str>,
    max_bytes: usize,
) -> Result<SlotFetchResult, SyncClientError> {
    if !is_lower_hex_exact(id, 64) {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid slot id '{}': expected 64 lowercase hex chars",
            id
        )));
    }
    let clean_prefix = humoco_market::validate_sync_prefix(prefix)
        .map_err(|e| SyncClientError::LocalValidation(format!("Invalid slot prefix: {}", e)))?;
    if !is_valid_sync_channel(channel) {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid sync channel '{}'",
            channel
        )));
    }
    let slot_key = match clean_prefix.as_deref() {
        Some(p) => format!("{}_{}", id, p),
        None => id.to_string(),
    };
    let url = append_sync_path(read_url, &format!("{}.{}.bin", slot_key, channel))?;

    let mut req = client.get(&url);
    if let Some(etag) = cached_etag.and_then(normalize_etag_token) {
        req = req.header("If-None-Match", format!("\"{}\"", etag));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| SyncClientError::Transport(format!("Network error fetching slot: {}", e)))?;

    match resp.status().as_u16() {
        304 => return Ok(SlotFetchResult::NotModified),
        404 => return Ok(SlotFetchResult::NotFound),
        _ => {}
    }

    let resp = map_unexpected_status(resp, &format!("fetching slot {}", id)).await?;
    let etag = extract_etag(resp.headers());
    let bytes = read_response_bytes_bounded(resp, max_bytes, "reading slot bytes").await?;

    Ok(SlotFetchResult::Modified { bytes, etag })
}

pub async fn get_slot(
    client: &reqwest::Client,
    read_url: &str,
    id: &str,
    prefix: Option<&str>,
    channel: &str,
    cached_etag: Option<&str>,
) -> Result<SlotFetchResult, SyncClientError> {
    get_slot_bounded(
        client,
        read_url,
        id,
        prefix,
        channel,
        cached_etag,
        MAX_SLOT_BLOB_BODY_BYTES,
    )
    .await
}

pub async fn get_media_blob(
    client: &reqwest::Client,
    read_url: &str,
    blob_id: &str,
) -> Result<Option<Vec<u8>>, SyncClientError> {
    if !is_lower_hex_exact(blob_id, 64) {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid blob_id '{}': expected 64 lowercase hex chars",
            blob_id
        )));
    }
    match get_slot_bounded(
        client,
        read_url,
        blob_id,
        None,
        "media",
        None,
        MAX_MEDIA_BLOB_BODY_BYTES,
    )
    .await?
    {
        SlotFetchResult::Modified { bytes, .. } => Ok(Some(bytes)),
        SlotFetchResult::NotFound | SlotFetchResult::NotModified => Ok(None),
    }
}

pub async fn fetch_text_bounded(
    client: &reqwest::Client,
    url: &str,
    max_bytes: usize,
    context: &str,
) -> Result<String, SyncClientError> {
    let clean_url = validated_sync_url(url)?;
    let resp = client
        .get(clean_url)
        .send()
        .await
        .map_err(|e| SyncClientError::Transport(format!("Network error {}: {}", context, e)))?;
    let resp = map_unexpected_status(resp, context).await?;
    let bytes = read_response_bytes_bounded(resp, max_bytes, context).await?;
    String::from_utf8(bytes)
        .map_err(|e| SyncClientError::InvalidPayload(format!("Invalid UTF-8 {}: {}", context, e)))
}
