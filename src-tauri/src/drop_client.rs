use ed25519_dalek::SigningKey;
use humoco_market::{DropRotationCertificate, SignedDropHeaders};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DropIndexEntry {
    pub id: String, // 64-hex pubkey
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
        entries: Vec<DropIndexEntry>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DropServerInfo {
    pub drop_pub: String,
    pub read_url: String,
    #[serde(default)]
    pub register_url: Option<String>,
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub rotation: Option<DropRotationCertificate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropCapabilityResult {
    pub authorized: bool,
    pub max_bytes: Option<usize>,
    pub ttl_days: Option<u32>,
    pub slot_etag: Option<String>,
    pub rotation: Option<DropRotationCertificate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutSlotOutcome {
    Saved {
        etag: Option<String>,
        rotation: Option<DropRotationCertificate>,
    },
    PreconditionFailed {
        current_etag: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DropServerAdminStatus {
    pub used_bytes: u64,
    pub max_bytes: u64,
    pub soft_limit_bytes: u64,
    pub soft_warning: bool,
    pub slot_count: usize,
    #[serde(default)]
    pub channel_counts: std::collections::BTreeMap<String, usize>,
    pub admin_pubkey: String,
    #[serde(default)]
    pub rotation: Option<DropRotationCertificate>,
}

pub fn validate_drop_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.starts_with("https://")
        || trimmed.starts_with("http://127.0.0.1")
        || trimmed.starts_with("http://localhost")
        || trimmed.starts_with("http://[::1]")
    {
        return Ok(());
    }
    Err(format!(
        "Drop URL must use HTTPS (or local dev loopback, got '{}')",
        url
    ))
}

pub fn create_drop_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub fn create_drop_probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub fn generate_drop_nonce() -> String {
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

pub fn extract_rotation_header(
    headers: &reqwest::header::HeaderMap,
) -> Option<DropRotationCertificate> {
    let val = headers.get("x-humoco-drop-rotation")?.to_str().ok()?;
    DropRotationCertificate::from_header_value(val).ok()
}

pub fn extract_etag(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
}

pub fn apply_verified_rotation(
    desc: &mut humoco_market::DropDescriptor,
    cert: &DropRotationCertificate,
    now_secs: u64,
) -> bool {
    if humoco_market::verify_drop_rotation_certificate(cert, &desc.drop_pub, now_secs).is_ok() {
        desc.drop_pub = cert.new_pubkey.clone();
        true
    } else {
        false
    }
}

pub async fn fetch_drop_info(
    client: &reqwest::Client,
    write_url: &str,
) -> Result<DropServerInfo, String> {
    validate_drop_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}info", write_url, sep);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching drop info from {}: {}", write_url, e))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Server returned HTTP {} on ?info: {}",
            status,
            text.trim()
        ));
    }

    let rotation = extract_rotation_header(resp.headers());
    let mut info: DropServerInfo = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse drop info JSON: {}", e))?;

    if info.rotation.is_none() {
        info.rotation = rotation;
    }

    Ok(info)
}

pub async fn head_slot_capability(
    client: &reqwest::Client,
    write_url: &str,
    pubkey_hex64: &str,
    channel: &str,
) -> Result<DropCapabilityResult, String> {
    validate_drop_url(write_url)?;

    let resp = client
        .head(write_url)
        .header("X-Drop-Pubkey", pubkey_hex64)
        .header("X-Channel", channel)
        .send()
        .await
        .map_err(|e| format!("Network error checking capability on {}: {}", write_url, e))?;

    let status = resp.status();
    let rotation = extract_rotation_header(resp.headers());

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

        Ok(DropCapabilityResult {
            authorized: true,
            max_bytes,
            ttl_days,
            slot_etag,
            rotation,
        })
    } else if status.as_u16() == 403 {
        Ok(DropCapabilityResult {
            authorized: false,
            max_bytes: None,
            ttl_days: None,
            slot_etag: None,
            rotation,
        })
    } else if status.as_u16() == 429 {
        Err("Rate limited by drop server (HTTP 429)".to_string())
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
    validate_drop_url(write_url)?;

    let mut req = client
        .put(write_url)
        .header("X-Drop-Pubkey", &signed_headers.pubkey_hex)
        .header("X-Channel", channel)
        .header("X-Drop-Timestamp", signed_headers.timestamp.to_string())
        .header("X-Drop-Nonce", &signed_headers.nonce_hex)
        .header("X-Drop-Signature", &signed_headers.signature_hex)
        .header("Content-Type", "application/octet-stream");

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
        let rotation = extract_rotation_header(resp.headers());
        Ok(PutSlotOutcome::Saved { etag, rotation })
    } else if status_code == 412 {
        let current_etag = extract_etag(resp.headers());
        Ok(PutSlotOutcome::PreconditionFailed { current_etag })
    } else if status_code == 413 {
        Err("Payload too large: drop server rejected slot size (HTTP 413)".to_string())
    } else if status_code == 429 {
        Err("Too many requests: drop server rate limit exceeded (HTTP 429)".to_string())
    } else if status_code == 507 {
        Err("Insufficient storage: drop server quota exceeded (HTTP 507)".to_string())
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
    validate_drop_url(write_url)?;

    let resp = client
        .delete(write_url)
        .header("X-Drop-Pubkey", &signed_headers.pubkey_hex)
        .header("X-Channel", channel)
        .header("X-Drop-Timestamp", signed_headers.timestamp.to_string())
        .header("X-Drop-Nonce", &signed_headers.nonce_hex)
        .header("X-Drop-Signature", &signed_headers.signature_hex)
        .send()
        .await
        .map_err(|e| format!("Network error deleting slot: {}", e))?;

    let status = resp.status();
    if status.as_u16() == 200 || status.as_u16() == 204 {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} on slot DELETE: {}",
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
    validate_drop_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}members&channel={}", write_url, sep, channel);

    let body_json = serde_json::to_string(&serde_json::json!({ "pubkeys": pubkeys }))
        .map_err(|e| format!("Failed to serialize members payload: {}", e))?;

    let timestamp = current_unix_timestamp();
    let nonce = generate_drop_nonce();
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
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} pushing members: {}",
            status,
            text.trim()
        ))
    }
}

pub async fn rotate_admin_key(
    client: &reqwest::Client,
    write_url: &str,
    new_admin_sk: &SigningKey,
    cert: &DropRotationCertificate,
) -> Result<(), String> {
    validate_drop_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}rotate", write_url, sep);

    let body_json = serde_json::to_string(cert)
        .map_err(|e| format!("Failed to serialize rotation certificate: {}", e))?;

    let timestamp = current_unix_timestamp();
    let nonce = generate_drop_nonce();
    let signed = humoco_market::sign_admin_request(
        new_admin_sk,
        "ROTATE",
        "",
        timestamp,
        &nonce,
        body_json.as_bytes(),
    );

    let resp = client
        .post(&url)
        .header("X-Drop-Pubkey", &signed.pubkey_hex)
        .header("X-Drop-Timestamp", signed.timestamp.to_string())
        .header("X-Drop-Nonce", &signed.nonce_hex)
        .header("X-Drop-Signature", &signed.signature_hex)
        .header("Content-Type", "application/json")
        .body(body_json)
        .send()
        .await
        .map_err(|e| format!("Network error rotating admin key: {}", e))?;

    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} rotating admin key: {}",
            status,
            text.trim()
        ))
    }
}

pub async fn fetch_admin_status(
    client: &reqwest::Client,
    write_url: &str,
    admin_sk: &SigningKey,
) -> Result<DropServerAdminStatus, String> {
    validate_drop_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}status", write_url, sep);

    let timestamp = current_unix_timestamp();
    let nonce = generate_drop_nonce();
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
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Server returned HTTP {} on ?status: {}",
            status,
            text.trim()
        ));
    }

    let status_data: DropServerAdminStatus = resp
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
    validate_drop_url(read_url)?;
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

    let entries: Vec<DropIndexEntry> = resp
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
    channel: &str,
) -> Result<Option<Vec<u8>>, String> {
    validate_drop_url(read_url)?;
    let base = read_url.trim().trim_end_matches('/');
    let url = format!("{}/{}.{}.bin", base, id, channel);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching slot: {}", e))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
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
