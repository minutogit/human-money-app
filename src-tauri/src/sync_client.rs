use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use humoco_market::SignedDropHeaders;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SyncIndexEntry {
    pub id: String, // 64-hex pubkey
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub channel: String,
    pub mtime: u64,
    pub size: usize,
    #[serde(default)]
    pub etag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexFetchResult {
    NotModified,
    Modified {
        entries: Vec<SyncIndexEntry>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SyncReplacementTarget {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SyncRevocationNotice {
    pub drop_pub: String,
    pub issued_at: u64,
    pub effective_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<SyncReplacementTarget>,
    pub signature: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SyncServerInfo {
    pub drop_pub: String,
    pub read_url: String,
    #[serde(default)]
    pub register_url: Option<String>,
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub revocation: Option<SyncRevocationNotice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncCapabilityResult {
    pub authorized: bool,
    pub max_bytes: Option<usize>,
    pub ttl_days: Option<u32>,
    pub slot_etag: Option<String>,
    pub media_max_count: Option<usize>,
    pub media_max_user_bytes: Option<usize>,
    pub media_used_count: Option<usize>,
    pub media_used_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutSlotOutcome {
    Saved {
        etag: Option<String>,
    },
    PreconditionFailed {
        current_etag: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SyncServerAdminStatus {
    pub used_bytes: u64,
    pub max_bytes: u64,
    pub soft_limit_bytes: u64,
    pub soft_warning: bool,
    pub slot_count: usize,
    #[serde(default)]
    pub channel_counts: std::collections::BTreeMap<String, usize>,
    pub admin_pubkey: String,
}

pub fn canonical_revocation_message(
    drop_pub: &str,
    issued_at: u64,
    effective_at: u64,
    replacement: Option<&SyncReplacementTarget>,
    reason: Option<&str>,
) -> String {
    let (rep_w, rep_r, rep_p) = match replacement {
        Some(r) => (
            r.write_url.as_str(),
            r.read_url.as_str(),
            r.drop_pub.to_lowercase(),
        ),
        None => ("", "", String::new()),
    };
    format!(
        "HUMOCO-DROP-REVOKE-V1\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        drop_pub.to_lowercase(),
        issued_at,
        effective_at,
        rep_w,
        rep_r,
        rep_p,
        reason.unwrap_or("")
    )
}

pub fn sign_revocation_notice(
    admin_sk: &SigningKey,
    issued_at: u64,
    effective_at: u64,
    reason: Option<String>,
    replacement: Option<SyncReplacementTarget>,
) -> Result<SyncRevocationNotice, String> {
    if let Some(ref r_txt) = reason {
        if r_txt.len() > 200 || r_txt.contains('\n') || r_txt.contains('\r') || r_txt.contains('\0') {
            return Err("Invalid revocation reason (max 200 chars, single-line)".to_string());
        }
    }
    if let Some(ref rep) = replacement {
        validate_sync_url(&rep.write_url)?;
        validate_sync_url(&rep.read_url)?;
        if rep.drop_pub.len() != 64 || !rep.drop_pub.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid replacement drop_pub".to_string());
        }
    }
    let drop_pub = hex::encode(admin_sk.verifying_key().as_bytes());
    let clean_reason = reason
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let clean_rep = replacement.map(|r| SyncReplacementTarget {
        write_url: r.write_url.trim().to_string(),
        read_url: r.read_url.trim().to_string(),
        drop_pub: r.drop_pub.trim().to_lowercase(),
    });
    let canonical = canonical_revocation_message(
        &drop_pub,
        issued_at,
        effective_at,
        clean_rep.as_ref(),
        clean_reason.as_deref(),
    );
    let sig = admin_sk.sign(canonical.as_bytes());
    Ok(SyncRevocationNotice {
        drop_pub,
        issued_at,
        effective_at,
        reason: clean_reason,
        replacement: clean_rep,
        signature: hex::encode(sig.to_bytes()),
    })
}

pub fn verify_revocation_notice(
    notice: &SyncRevocationNotice,
    expected_drop_pub: &str,
) -> Result<(), String> {
    let expected = expected_drop_pub.trim().to_lowercase();
    if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Missing or invalid expected drop_pub".to_string());
    }
    if notice.drop_pub.trim().to_lowercase() != expected {
        return Err("Revocation notice drop_pub mismatch".to_string());
    }
    if notice.issued_at == 0 || notice.effective_at == 0 {
        return Err("Revocation notice missing timestamps".to_string());
    }
    if let Some(ref r_txt) = notice.reason {
        if r_txt.len() > 200 || r_txt.contains('\n') || r_txt.contains('\r') || r_txt.contains('\0') {
            return Err("Revocation notice contains invalid reason".to_string());
        }
    }
    if let Some(ref rep) = notice.replacement {
        validate_sync_url(&rep.write_url)?;
        validate_sync_url(&rep.read_url)?;
        if rep.drop_pub.len() != 64 || !rep.drop_pub.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Revocation notice contains invalid replacement drop_pub".to_string());
        }
    }
    let pk_bytes: [u8; 32] = hex::decode(&expected)
        .map_err(|_| "Invalid drop_pub hex".to_string())?
        .try_into()
        .map_err(|_| "Invalid drop_pub length".to_string())?;
    let vk = VerifyingKey::from_bytes(&pk_bytes)
        .map_err(|_| "Invalid Ed25519 drop_pub".to_string())?;
    let sig_bytes: [u8; 64] = hex::decode(notice.signature.trim())
        .map_err(|_| "Invalid signature hex".to_string())?
        .try_into()
        .map_err(|_| "Invalid signature length".to_string())?;
    let sig = Signature::from_bytes(&sig_bytes);

    let canonical = canonical_revocation_message(
        &expected,
        notice.issued_at,
        notice.effective_at,
        notice.replacement.as_ref(),
        notice.reason.as_deref(),
    );
    vk.verify(canonical.as_bytes(), &sig)
        .map_err(|_| "Invalid revocation notice Ed25519 signature".to_string())
}

pub async fn fetch_revocation_notice(
    client: &reqwest::Client,
    read_url: &str,
    expected_drop_pub: &str,
) -> Result<Option<SyncRevocationNotice>, String> {
    validate_sync_url(read_url)?;
    let base = read_url.trim().trim_end_matches('/');
    let url = format!("{}/revocation.json", base);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching revocation.json from {}: {}", read_url, e))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(format!("Server returned HTTP {} fetching revocation.json", status));
    }
    let notice: SyncRevocationNotice = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse revocation.json: {}", e))?;
    verify_revocation_notice(&notice, expected_drop_pub)?;
    Ok(Some(notice))
}

pub async fn push_revocation_notice(
    client: &reqwest::Client,
    write_url: &str,
    admin_sk: &SigningKey,
    notice: Option<&SyncRevocationNotice>,
    issued_at: u64,
) -> Result<(), String> {
    validate_sync_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}revoke", write_url, sep);

    let body_json = match notice {
        Some(n) => serde_json::to_string(n)
            .map_err(|e| format!("Failed to serialize revocation notice: {}", e))?,
        None => serde_json::to_string(&serde_json::json!({
            "issued_at": issued_at,
            "effective_at": null
        }))
        .map_err(|e| format!("Failed to serialize revocation clear payload: {}", e))?,
    };

    let timestamp = current_unix_timestamp();
    let nonce = generate_sync_nonce();
    let signed = humoco_market::sign_admin_request(
        admin_sk,
        "PUT_REVOKE",
        "",
        timestamp,
        &nonce,
        body_json.as_bytes(),
    );

    let resp = client
        .put(&url)
        .header("X-Drop-Pubkey", &signed.pubkey_hex)
        .header("X-Drop-Timestamp", signed.timestamp.to_string())
        .header("X-Drop-Nonce", &signed.nonce_hex)
        .header("X-Drop-Signature", &signed.signature_hex)
        .header("Content-Type", "application/json")
        .body(body_json)
        .send()
        .await
        .map_err(|e| format!("Network error pushing revocation: {}", e))?;

    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} pushing revocation: {}",
            status,
            text.trim()
        ))
    }
}

pub fn validate_sync_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.starts_with("https://")
        || trimmed.starts_with("http://127.0.0.1")
        || trimmed.starts_with("http://localhost")
        || trimmed.starts_with("http://[::1]")
    {
        return Ok(());
    }
    Err(format!(
        "Sync URL must use HTTPS (or local dev loopback, got '{}')",
        url
    ))
}

pub fn create_sync_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub fn create_sync_probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

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

pub fn extract_etag(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
}

pub fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> u64 {
    if let Some(val) = headers.get(reqwest::header::RETRY_AFTER) {
        if let Ok(s) = val.to_str() {
            if let Ok(secs) = s.trim().parse::<u64>() {
                return secs;
            }
        }
    }
    60
}

pub fn extract_retry_after_from_error_str(err: &str) -> Option<u64> {
    if let Some(pos) = err.find("Retry-After:") {
        let after = &err[pos + "Retry-After:".len()..];
        let trimmed = after.trim_start();
        let num_str: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(secs) = num_str.parse::<u64>() {
            return Some(secs);
        }
    }
    if err.contains("429") {
        return Some(60);
    }
    None
}

pub async fn fetch_sync_info(
    client: &reqwest::Client,
    write_url: &str,
) -> Result<SyncServerInfo, String> {
    validate_sync_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}info", write_url, sep);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching sync info from {}: {}", write_url, e))?;

    let status = resp.status();
    if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        return Err(format!(
            "Rate limited fetching sync info from {} (HTTP 429). Retry-After: {}s",
            write_url, secs
        ));
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Server returned HTTP {} on ?info: {}",
            status,
            text.trim()
        ));
    }

    let info: SyncServerInfo = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse sync info JSON: {}", e))?;

    Ok(info)
}

pub async fn head_slot_capability(
    client: &reqwest::Client,
    write_url: &str,
    pubkey_hex64: &str,
    prefix: Option<&str>,
    channel: &str,
) -> Result<SyncCapabilityResult, String> {
    validate_sync_url(write_url)?;

    let mut req = client
        .head(write_url)
        .header("X-Drop-Pubkey", pubkey_hex64)
        .header("X-Channel", channel);

    if let Some(p) = prefix.filter(|s| !s.is_empty()) {
        req = req.header("X-Drop-Prefix", p);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error checking capability on {}: {}", write_url, e))?;

    let status = resp.status();

    if status.as_u16() == 200 {
        let max_bytes = resp
            .headers()
            .get("x-drop-max-bytes")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());
        let ttl_days = resp
            .headers()
            .get("x-drop-ttl-days")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok());
        let slot_etag = resp
            .headers()
            .get("x-drop-slot-etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty());
        let media_max_count = resp
            .headers()
            .get("x-drop-media-max-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());
        let media_max_user_bytes = resp
            .headers()
            .get("x-drop-media-max-user-bytes")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());
        let media_used_count = resp
            .headers()
            .get("x-drop-media-used-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());
        let media_used_bytes = resp
            .headers()
            .get("x-drop-media-used-bytes")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());

        Ok(SyncCapabilityResult {
            authorized: true,
            max_bytes,
            ttl_days,
            slot_etag,
            media_max_count,
            media_max_user_bytes,
            media_used_count,
            media_used_bytes,
        })
    } else if status.as_u16() == 403 {
        Ok(SyncCapabilityResult {
            authorized: false,
            max_bytes: None,
            ttl_days: None,
            slot_etag: None,
            media_max_count: None,
            media_max_user_bytes: None,
            media_used_count: None,
            media_used_bytes: None,
        })
    } else if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!("Rate limited by sync server (HTTP 429). Retry-After: {}s", secs))
    } else {
        Err(format!(
            "Server returned HTTP {} on slot capability check",
            status
        ))
    }
}

pub async fn put_slot(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
    channel: &str,
    blob_bytes: &[u8],
    if_match: Option<&str>,
) -> Result<PutSlotOutcome, String> {
    validate_sync_url(write_url)?;

    let mut req = client
        .put(write_url)
        .header("X-Drop-Pubkey", &signed_headers.pubkey_hex)
        .header("X-Channel", channel)
        .header("X-Drop-Timestamp", signed_headers.timestamp.to_string())
        .header("X-Drop-Nonce", &signed_headers.nonce_hex)
        .header("X-Drop-Signature", &signed_headers.signature_hex)
        .header("Content-Type", "application/octet-stream");

    if let Some(ref p) = signed_headers.prefix {
        if !p.is_empty() {
            req = req.header("X-Drop-Prefix", p);
        }
    }

    if let Some(im) = if_match {
        if im == "none" {
            req = req.header("If-Match", "none");
        } else {
            req = req.header("If-Match", format!("\"{}\"", im.trim_matches('"')));
        }
    }

    let resp = req
        .body(blob_bytes.to_vec())
        .send()
        .await
        .map_err(|e| format!("Network error sending slot: {}", e))?;

    let status = resp.status();
    let status_code = status.as_u16();

    if status_code == 200 || status_code == 201 || status_code == 204 {
        let etag = extract_etag(resp.headers());
        Ok(PutSlotOutcome::Saved { etag })
    } else if status_code == 412 {
        let current_etag = extract_etag(resp.headers());
        Ok(PutSlotOutcome::PreconditionFailed { current_etag })
    } else if status_code == 413 {
        Err("Payload too large: sync server rejected slot size (HTTP 413)".to_string())
    } else if status_code == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!("Too many requests: sync server rate limit exceeded (HTTP 429). Retry-After: {}s", secs))
    } else if status_code == 507 {
        Err("Insufficient storage: sync server quota exceeded (HTTP 507)".to_string())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} on slot PUT: {}",
            status,
            text.trim()
        ))
    }
}

pub async fn delete_slot(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
    channel: &str,
) -> Result<(), String> {
    validate_sync_url(write_url)?;

    let mut req = client
        .delete(write_url)
        .header("X-Drop-Pubkey", &signed_headers.pubkey_hex)
        .header("X-Channel", channel)
        .header("X-Drop-Timestamp", signed_headers.timestamp.to_string())
        .header("X-Drop-Nonce", &signed_headers.nonce_hex)
        .header("X-Drop-Signature", &signed_headers.signature_hex);

    if let Some(ref p) = signed_headers.prefix {
        if !p.is_empty() {
            req = req.header("X-Drop-Prefix", p);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error deleting slot: {}", e))?;

    let status = resp.status();
    if status.as_u16() == 200 || status.as_u16() == 204 {
        Ok(())
    } else if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!("Rate limited deleting slot (HTTP 429). Retry-After: {}s", secs))
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} on slot DELETE: {}",
            status,
            text.trim()
        ))
    }
}

pub async fn put_media_blob(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
    blob_id: &str,
    blob_bytes: &[u8],
) -> Result<PutSlotOutcome, String> {
    validate_sync_url(write_url)?;

    let resp = client
        .put(write_url)
        .header("X-Drop-Pubkey", &signed_headers.pubkey_hex)
        .header("X-Channel", "media")
        .header("X-Media-Id", blob_id)
        .header("X-Drop-Timestamp", signed_headers.timestamp.to_string())
        .header("X-Drop-Nonce", &signed_headers.nonce_hex)
        .header("X-Drop-Signature", &signed_headers.signature_hex)
        .header("Content-Type", "application/octet-stream")
        .body(blob_bytes.to_vec())
        .send()
        .await
        .map_err(|e| format!("Network error sending media blob: {}", e))?;

    let status = resp.status();
    let status_code = status.as_u16();

    if status_code == 200 || status_code == 201 || status_code == 204 {
        let etag = extract_etag(resp.headers());
        Ok(PutSlotOutcome::Saved { etag })
    } else if status_code == 412 {
        let current_etag = extract_etag(resp.headers());
        Ok(PutSlotOutcome::PreconditionFailed { current_etag })
    } else if status_code == 413 {
        Err("Payload too large: sync server rejected media size (HTTP 413)".to_string())
    } else if status_code == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!("Too many requests: sync server rate limit exceeded (HTTP 429). Retry-After: {}s", secs))
    } else if status_code == 507 {
        Err("Insufficient storage: sync server quota exceeded (HTTP 507)".to_string())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} on media PUT: {}",
            status,
            text.trim()
        ))
    }
}

pub async fn delete_media_blob(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
    blob_id: Option<&str>,
) -> Result<(), String> {
    validate_sync_url(write_url)?;

    let mut req = client
        .delete(write_url)
        .header("X-Drop-Pubkey", &signed_headers.pubkey_hex)
        .header("X-Channel", "media")
        .header("X-Drop-Timestamp", signed_headers.timestamp.to_string())
        .header("X-Drop-Nonce", &signed_headers.nonce_hex)
        .header("X-Drop-Signature", &signed_headers.signature_hex);

    if let Some(id) = blob_id {
        req = req.header("X-Media-Id", id);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error deleting media blob: {}", e))?;

    let status = resp.status();
    if status.as_u16() == 200 || status.as_u16() == 204 {
        Ok(())
    } else if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!("Rate limited deleting media blob (HTTP 429). Retry-After: {}s", secs))
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} on media DELETE: {}",
            status,
            text.trim()
        ))
    }
}

pub async fn push_members(
    client: &reqwest::Client,
    write_url: &str,
    admin_sk: &SigningKey,
    channel: &str,
    pubkeys: &[String],
) -> Result<(), String> {
    validate_sync_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}members&channel={}", write_url, sep, channel);

    let body_json = serde_json::to_string(&serde_json::json!({ "pubkeys": pubkeys }))
        .map_err(|e| format!("Failed to serialize members payload: {}", e))?;

    let timestamp = current_unix_timestamp();
    let nonce = generate_sync_nonce();
    let signed = humoco_market::sign_admin_request(
        admin_sk,
        "PUT_MEMBERS",
        channel,
        timestamp,
        &nonce,
        body_json.as_bytes(),
    );

    let resp = client
        .put(&url)
        .header("X-Drop-Pubkey", &signed.pubkey_hex)
        .header("X-Drop-Timestamp", signed.timestamp.to_string())
        .header("X-Drop-Nonce", &signed.nonce_hex)
        .header("X-Drop-Signature", &signed.signature_hex)
        .header("Content-Type", "application/json")
        .body(body_json)
        .send()
        .await
        .map_err(|e| format!("Network error pushing members: {}", e))?;

    let status = resp.status();
    if status.as_u16() == 200 || status.as_u16() == 204 {
        Ok(())
    } else if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!("Rate limited pushing members (HTTP 429). Retry-After: {}s", secs))
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} pushing members: {}",
            status,
            text.trim()
        ))
    }
}



pub async fn fetch_admin_status(
    client: &reqwest::Client,
    write_url: &str,
    admin_sk: &SigningKey,
) -> Result<SyncServerAdminStatus, String> {
    validate_sync_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}status", write_url, sep);

    let timestamp = current_unix_timestamp();
    let nonce = generate_sync_nonce();
    let signed = humoco_market::sign_admin_request(
        admin_sk,
        "STATUS",
        "",
        timestamp,
        &nonce,
        b"",
    );

    let resp = client
        .get(&url)
        .header("X-Drop-Pubkey", &signed.pubkey_hex)
        .header("X-Drop-Timestamp", signed.timestamp.to_string())
        .header("X-Drop-Nonce", &signed.nonce_hex)
        .header("X-Drop-Signature", &signed.signature_hex)
        .send()
        .await
        .map_err(|e| format!("Network error fetching admin status: {}", e))?;

    let status = resp.status();
    if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        return Err(format!("Rate limited fetching admin status (HTTP 429). Retry-After: {}s", secs));
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Server returned HTTP {} on ?status: {}",
            status,
            text.trim()
        ));
    }

    let status_data: SyncServerAdminStatus = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse admin status JSON: {}", e))?;

    Ok(status_data)
}

pub async fn list_index(
    client: &reqwest::Client,
    read_url: &str,
    cached_etag: Option<&str>,
    cached_last_modified: Option<&str>,
) -> Result<IndexFetchResult, String> {
    validate_sync_url(read_url)?;
    let base = read_url.trim().trim_end_matches('/');
    let url = format!("{}/index.json", base);

    let mut req = client.get(&url);
    if let Some(etag) = cached_etag {
        req = req.header("If-None-Match", etag);
    }
    if let Some(lm) = cached_last_modified {
        req = req.header("If-Modified-Since", lm);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error fetching index: {}", e))?;

    let status = resp.status();
    if status.as_u16() == 304 {
        return Ok(IndexFetchResult::NotModified);
    }
    if status.as_u16() == 404 {
        return Ok(IndexFetchResult::Modified {
            entries: Vec::new(),
            etag: None,
            last_modified: None,
        });
    }
    if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        return Err(format!("Rate limited fetching index (HTTP 429). Retry-After: {}s", secs));
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Server returned HTTP {} fetching index: {}",
            status,
            text.trim()
        ));
    }

    let new_etag = extract_etag(resp.headers());
    let new_last_modified = resp
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let entries: Vec<SyncIndexEntry> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse index.json: {}", e))?;

    Ok(IndexFetchResult::Modified {
        entries,
        etag: new_etag,
        last_modified: new_last_modified,
    })
}

pub async fn get_slot(
    client: &reqwest::Client,
    read_url: &str,
    id: &str,
    prefix: Option<&str>,
    channel: &str,
) -> Result<Option<Vec<u8>>, String> {
    validate_sync_url(read_url)?;
    let base = read_url.trim().trim_end_matches('/');
    let slot_key = match prefix.filter(|s| !s.is_empty()) {
        Some(p) => format!("{}_{}", id, p),
        None => id.to_string(),
    };
    let url = format!("{}/{}.{}.bin", base, slot_key, channel);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching slot: {}", e))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if resp.status().as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        return Err(format!("Rate limited fetching slot {} (HTTP 429). Retry-After: {}s", id, secs));
    }

    if !resp.status().is_success() {
        return Err(format!(
            "Server returned HTTP {} fetching slot {}",
            resp.status(),
            id
        ));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read slot bytes: {}", e))?;

    Ok(Some(bytes.to_vec()))
}

pub async fn get_media_blob(
    client: &reqwest::Client,
    read_url: &str,
    blob_id: &str,
) -> Result<Option<Vec<u8>>, String> {
    if blob_id.len() != 64 || !blob_id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        return Err(format!(
            "Invalid blob_id '{}': expected 64 lowercase hex chars",
            blob_id
        ));
    }
    get_slot(client, read_url, blob_id, None, "media").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_retry_after_header() {
        let mut headers = reqwest::header::HeaderMap::new();
        // Missing header -> 60s fallback
        assert_eq!(parse_retry_after(&headers), 60);

        // Valid integer seconds
        headers.insert(reqwest::header::RETRY_AFTER, "45".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), 45);

        // Invalid string -> 60s fallback
        headers.insert(reqwest::header::RETRY_AFTER, "invalid_seconds".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), 60);
    }

    #[test]
    fn test_extract_retry_after_from_error_str() {
        let err1 = "Rate limited by sync server (HTTP 429). Retry-After: 45s";
        assert_eq!(extract_retry_after_from_error_str(err1), Some(45));

        let err2 = "Server returned HTTP 429: Too Many Requests";
        assert_eq!(extract_retry_after_from_error_str(err2), Some(60));

        let err3 = "Network timeout: connection refused";
        assert_eq!(extract_retry_after_from_error_str(err3), None);
    }

    #[tokio::test]
    async fn test_get_media_blob_id_validation() {
        let client = create_sync_http_client();
        // Invalid length
        let err_short = get_media_blob(&client, "https://example.com/sync", "abc").await.unwrap_err();
        assert!(err_short.contains("Invalid blob_id"));

        // Invalid characters (uppercase or non-hex)
        let err_chars = get_media_blob(&client, "https://example.com/sync", &"G".repeat(64)).await.unwrap_err();
        assert!(err_chars.contains("Invalid blob_id"));
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
        let rep = SyncReplacementTarget {
            write_url: "https://new.example.org/sync.php".to_string(),
            read_url: "https://new.example.org/d-1234567890abcdef".to_string(),
            drop_pub: hex::encode([7u8; 32]),
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

        // Wrong expected drop_pub
        let wrong_pub = hex::encode([99u8; 32]);
        assert!(verify_revocation_notice(&notice, &wrong_pub).is_err());

        // Tampered replacement URL
        let mut tampered = notice.clone();
        tampered.replacement.as_mut().unwrap().write_url =
            "https://evil.example.org/sync.php".to_string();
        assert!(verify_revocation_notice(&tampered, &drop_pub).is_err());

        // Insecure http:// replacement URL rejected
        let bad_rep = SyncReplacementTarget {
            write_url: "http://insecure.example.org/sync.php".to_string(),
            read_url: "https://new.example.org/d-1234".to_string(),
            drop_pub: hex::encode([7u8; 32]),
        };
        assert!(sign_revocation_notice(&sk, 100, 200, None, Some(bad_rep)).is_err());

        // Newline injection in reason rejected
        assert!(sign_revocation_notice(&sk, 100, 200, Some("bad\nline".to_string()), None).is_err());
    }
}
