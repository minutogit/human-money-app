use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use human_money_core::humoco_proto::{
    causal::{LockEntry, LOCK_ENTRY_WIRE_LEN},
    crypto::solve_ticket_pow,
    decode_hex_fixed,
    manifest::GenesisTicket,
};
use human_money_core::models::layer2_api::{SyncLockBatchRequest, SyncLockBatchResponse};
use humoco_market::SignedDropHeaders;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotFetchResult {
    NotModified,
    NotFound,
    Modified {
        bytes: Vec<u8>,
        etag: Option<String>,
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
pub struct SyncLockDelegationDto {
    pub admin_pub: String,
    pub lock_pub: String,
    pub issued_at: u64,
    pub sig: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SyncLockSelftestChecksDto {
    pub ext_sodium: String,
    pub sha3_256: String,
    pub atomic_link_eexist: String,
    pub fsync: String,
    pub ignore_user_abort: String,
    pub storage_writable: String,
    pub inode_budget: String,
    pub fastcgi_finish_request: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SyncLockSelftestReportDto {
    pub runtime: String,
    pub checked_at: u64,
    pub lock_pub: String,
    pub php_version: Option<String>,
    pub checks: SyncLockSelftestChecksDto,
    pub free_inodes: u64,
    pub all_passed: bool,
    pub sig: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SyncLockInfoDto {
    pub lock_pub: String,
    #[serde(default)]
    pub delegation: Option<SyncLockDelegationDto>,
    pub max_retention_quarter: u16,
    #[serde(default)]
    pub max_inodes: u64,
    #[serde(default)]
    pub used_inodes: u64,
    #[serde(default)]
    pub inode_warning: bool,
    pub pow_base_bits: u32,
    #[serde(default)]
    pub allow_pow_tickets: bool,
    #[serde(default)]
    pub selftest: Option<SyncLockSelftestReportDto>,
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
    #[serde(default)]
    pub lock: Option<SyncLockInfoDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrontendLockServerProbeResult {
    pub write_url: String,
    pub read_url: String,
    pub drop_pub: String,
    pub admin_pub: String,
    pub lock_pub: String,
    pub delegation_valid: bool,
    pub waf_probe_passed: bool,
    pub selftest_passed: bool,
    pub key_leak_safe: bool,
    pub passed: bool,
    pub latency_ms: u64,
    pub max_retention_quarter: u16,
    pub pow_base_bits: u32,
    pub allow_pow_tickets: bool,
    pub inode_warning: bool,
    pub used_inodes: u64,
    pub max_inodes: u64,
    pub probed_at_secs: u64,
    pub error: Option<String>,
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
    Saved { etag: Option<String> },
    PreconditionFailed { current_etag: Option<String> },
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
        if r_txt.len() > 200 || r_txt.contains('\n') || r_txt.contains('\r') || r_txt.contains('\0')
        {
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
        if r_txt.len() > 200 || r_txt.contains('\n') || r_txt.contains('\r') || r_txt.contains('\0')
        {
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
    let vk =
        VerifyingKey::from_bytes(&pk_bytes).map_err(|_| "Invalid Ed25519 drop_pub".to_string())?;
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
    let resp = client.get(&url).send().await.map_err(|e| {
        format!(
            "Network error fetching revocation.json from {}: {}",
            read_url, e
        )
    })?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(format!(
            "Server returned HTTP {} fetching revocation.json",
            status
        ));
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

fn loopback_sync_allowed() -> bool {
    cfg!(test) || std::env::var("HUMOCO_ALLOW_LOOPBACK_SYNC").ok().as_deref() == Some("1")
}

fn is_blocked_ip(ip: std::net::IpAddr, allow_loopback: bool) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            if v4.is_loopback() {
                !allow_loopback
            } else {
                v4.is_private() || v4.is_link_local() || v4.is_unspecified() || v4.is_multicast()
            }
        }
        std::net::IpAddr::V6(v6) => {
            if v6.is_loopback() {
                !allow_loopback
            } else {
                let seg0 = v6.segments()[0];
                v6.is_unspecified()
                    || v6.is_multicast()
                    || (seg0 & 0xfe00) == 0xfc00
                    || (seg0 & 0xffc0) == 0xfe80
            }
        }
    }
}

pub fn validate_sync_url(url: &str) -> Result<(), String> {
    validate_sync_url_strict(url, loopback_sync_allowed())
}

pub fn validate_sync_url_strict(url: &str, allow_loopback: bool) -> Result<(), String> {
    let trimmed = url.trim();
    let parsed =
        reqwest::Url::parse(trimmed).map_err(|e| format!("Invalid Sync URL '{}': {}", url, e))?;
    if !parsed.username().is_empty() || parsed.password().is_some() || parsed.fragment().is_some() {
        return Err(format!(
            "Sync URL must not contain credentials or fragment: '{}'",
            url
        ));
    }
    let Some(host) = parsed.host_str() else {
        return Err(format!("Sync URL missing host: '{}'", url));
    };
    let host_lower = host
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    let is_loopback_name = host_lower == "localhost"
        || host_lower.ends_with(".localhost")
        || host_lower == "127.0.0.1"
        || host_lower == "::1";

    if let Ok(ip) = host_lower.parse::<std::net::IpAddr>() {
        if is_blocked_ip(ip, allow_loopback) {
            return Err(format!(
                "Sync URL targets blocked internal/private address '{}'",
                url
            ));
        }
    } else if host_lower.ends_with(".local") || host_lower.ends_with(".internal") {
        return Err(format!("Sync URL targets internal hostname '{}'", url));
    } else if is_loopback_name && !allow_loopback {
        return Err(format!(
            "Sync URL loopback is forbidden in production: '{}'",
            url
        ));
    }

    match parsed.scheme() {
        "https" => Ok(()),
        "http" if allow_loopback && is_loopback_name => Ok(()),
        _ => Err(format!(
            "Sync URL must use HTTPS (or allowed dev loopback, got '{}')",
            url
        )),
    }
}

pub fn create_sync_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub fn create_sync_probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(6))
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

    let mut info: SyncServerInfo = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse sync info JSON: {}", e))?;

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
        let authorized = resp
            .headers()
            .get("x-drop-authorized")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let max_bytes = resp
            .headers()
            .get("x-channel-max-bytes")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<usize>().ok());
        let ttl_days = resp
            .headers()
            .get("x-channel-ttl-days")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u32>().ok());
        let slot_etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty());
        let media_max_count = resp
            .headers()
            .get("x-media-max-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<usize>().ok());
        let media_max_user_bytes = resp
            .headers()
            .get("x-media-max-bytes")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<usize>().ok());
        let media_used_count = resp
            .headers()
            .get("x-media-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<usize>().ok());
        let media_used_bytes = resp
            .headers()
            .get("x-media-bytes")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<usize>().ok());

        Ok(SyncCapabilityResult {
            authorized,
            max_bytes,
            ttl_days,
            slot_etag,
            media_max_count,
            media_max_user_bytes,
            media_used_count,
            media_used_bytes,
        })
    } else if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!(
            "Rate limited by sync server (HTTP 429). Retry-After: {}s",
            secs
        ))
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
        Err(format!(
            "Too many requests: sync server rate limit exceeded (HTTP 429). Retry-After: {}s",
            secs
        ))
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
    if status.as_u16() == 200 || status.as_u16() == 204 || status.as_u16() == 404 {
        Ok(())
    } else if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!(
            "Rate limited deleting slot (HTTP 429). Retry-After: {}s",
            secs
        ))
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} on slot DELETE: {}",
            status,
            text.trim()
        ))
    }
}

pub async fn delete_account(
    client: &reqwest::Client,
    write_url: &str,
    signed_headers: &SignedDropHeaders,
) -> Result<(), String> {
    validate_sync_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}account", write_url, sep);

    let resp = client
        .delete(&url)
        .header("X-Drop-Pubkey", &signed_headers.pubkey_hex)
        .header("X-Drop-Timestamp", signed_headers.timestamp.to_string())
        .header("X-Drop-Nonce", &signed_headers.nonce_hex)
        .header("X-Drop-Signature", &signed_headers.signature_hex)
        .send()
        .await
        .map_err(|e| format!("Network error deleting account: {}", e))?;

    let status = resp.status();
    if status.as_u16() == 200 || status.as_u16() == 204 || status.as_u16() == 404 {
        Ok(())
    } else if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!(
            "Rate limited deleting account (HTTP 429). Retry-After: {}s",
            secs
        ))
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} on account DELETE: {}",
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
        Err(format!(
            "Too many requests: sync server rate limit exceeded (HTTP 429). Retry-After: {}s",
            secs
        ))
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
    if status.as_u16() == 200 || status.as_u16() == 204 || status.as_u16() == 404 {
        Ok(())
    } else if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        Err(format!(
            "Rate limited deleting media blob (HTTP 429). Retry-After: {}s",
            secs
        ))
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
        Err(format!(
            "Rate limited pushing members (HTTP 429). Retry-After: {}s",
            secs
        ))
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
    let signed = humoco_market::sign_admin_request(admin_sk, "STATUS", "", timestamp, &nonce, b"");

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
        return Err(format!(
            "Rate limited fetching admin status (HTTP 429). Retry-After: {}s",
            secs
        ));
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
        return Err(format!(
            "Rate limited fetching index (HTTP 429). Retry-After: {}s",
            secs
        ));
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
    cached_etag: Option<&str>,
) -> Result<SlotFetchResult, String> {
    validate_sync_url(read_url)?;
    let base = read_url.trim().trim_end_matches('/');
    let slot_key = match prefix.filter(|s| !s.is_empty()) {
        Some(p) => format!("{}_{}", id, p),
        None => id.to_string(),
    };
    let url = format!("{}/{}.{}.bin", base, slot_key, channel);

    let mut req = client.get(&url);
    if let Some(etag) = cached_etag
        .map(|s| s.trim().trim_matches('"'))
        .filter(|s| !s.is_empty())
    {
        req = req.header("If-None-Match", format!("\"{}\"", etag));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error fetching slot: {}", e))?;

    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(SlotFetchResult::NotModified);
    }

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(SlotFetchResult::NotFound);
    }

    if resp.status().as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        return Err(format!(
            "Rate limited fetching slot {} (HTTP 429). Retry-After: {}s",
            id, secs
        ));
    }

    if !resp.status().is_success() {
        return Err(format!(
            "Server returned HTTP {} fetching slot {}",
            resp.status(),
            id
        ));
    }

    let etag = resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty());

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read slot bytes: {}", e))?;

    Ok(SlotFetchResult::Modified {
        bytes: bytes.to_vec(),
        etag,
    })
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
    match get_slot(client, read_url, blob_id, None, "media", None).await? {
        SlotFetchResult::Modified { bytes, .. } => Ok(Some(bytes)),
        SlotFetchResult::NotFound | SlotFetchResult::NotModified => Ok(None),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SyncLockProbeResponseDto {
    pub probe_ok: bool,
    pub lock_pub: String,
    pub client_nonce: String,
    pub server_time: u64,
    pub sig: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GenesisTicketResponse {
    pub ticket: GenesisTicket,
}

#[derive(Debug, Clone)]
pub enum SyncLockBatchOutcome {
    Response {
        status_code: u16,
        body: Box<SyncLockBatchResponse>,
    },
    Rejected {
        status_code: u16,
        error: String,
    },
}

pub fn normalize_lock_read_base(read_url: &str) -> String {
    let trimmed = read_url.trim().trim_end_matches('/');
    trimmed
        .strip_suffix("/locks")
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .to_string()
}

pub fn generate_lock_probe_nonce() -> String {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    hex::encode(bytes)
}

fn verify_ed25519_hex_strict(pubkey_hex: &str, sig_hex: &str, msg: &[u8]) -> bool {
    let Ok(pk_bytes) = decode_hex_fixed::<32>(pubkey_hex.trim()) else {
        return false;
    };
    let Ok(sig_bytes) = decode_hex_fixed::<64>(sig_hex.trim()) else {
        return false;
    };
    let Ok(vk) = VerifyingKey::from_bytes(&pk_bytes) else {
        return false;
    };
    let sig = Signature::from_bytes(&sig_bytes);
    vk.verify_strict(msg, &sig).is_ok()
}

pub fn verify_lock_delegation(
    info_lock_pub: &str,
    delegation: &SyncLockDelegationDto,
    expected_admin_pub: Option<&str>,
) -> bool {
    if delegation.issued_at == 0 {
        return false;
    }
    let lock_lower = delegation.lock_pub.trim().to_ascii_lowercase();
    let admin_lower = delegation.admin_pub.trim().to_ascii_lowercase();
    if lock_lower != info_lock_pub.trim().to_ascii_lowercase() {
        return false;
    }
    if let Some(exp) = expected_admin_pub {
        if admin_lower != exp.trim().to_ascii_lowercase() {
            return false;
        }
    }
    let msg = format!(
        "HUMOCO-LOCK-DELEGATE-V1\n{}\n{}\n{}",
        admin_lower, lock_lower, delegation.issued_at
    );
    verify_ed25519_hex_strict(&admin_lower, &delegation.sig, msg.as_bytes())
}

pub fn verify_lock_selftest(expected_lock_pub: &str, report: &SyncLockSelftestReportDto) -> bool {
    if !report.all_passed {
        return false;
    }
    let runtime = report.runtime.trim();
    if runtime != "rust" && runtime != "php" {
        return false;
    }
    let lock_lower = report.lock_pub.trim().to_ascii_lowercase();
    if lock_lower != expected_lock_pub.trim().to_ascii_lowercase() {
        return false;
    }
    let c = &report.checks;
    let canonical_checks_line = format!(
        "{},{},{},{},{},{},{},{},{}",
        c.ext_sodium,
        c.sha3_256,
        c.atomic_link_eexist,
        c.fsync,
        c.ignore_user_abort,
        c.storage_writable,
        c.inode_budget,
        c.fastcgi_finish_request,
        report.free_inodes
    );
    let checks_digest_hex = hex::encode(Sha256::digest(canonical_checks_line.as_bytes()));
    let msg = format!(
        "HUMOCO-LOCK-SELFTEST-V1\n{}\n{}\n{}\n{}\n{}",
        runtime, report.checked_at, lock_lower, 1, checks_digest_hex
    );
    verify_ed25519_hex_strict(&lock_lower, &report.sig, msg.as_bytes())
}

pub fn verify_lock_probe_response(
    expected_lock_pub: &str,
    expected_nonce: &str,
    resp: &SyncLockProbeResponseDto,
) -> bool {
    if !resp.probe_ok {
        return false;
    }
    let lock_lower = resp.lock_pub.trim().to_ascii_lowercase();
    let nonce_lower = resp.client_nonce.trim().to_ascii_lowercase();
    if lock_lower != expected_lock_pub.trim().to_ascii_lowercase()
        || nonce_lower != expected_nonce.trim().to_ascii_lowercase()
    {
        return false;
    }
    let msg = format!(
        "HUMOCO-LOCK-PROBE-V1\n{}\n{}\n{}",
        lock_lower, nonce_lower, resp.server_time
    );
    verify_ed25519_hex_strict(&lock_lower, &resp.sig, msg.as_bytes())
}

pub async fn probe_lock_server(
    client: &reqwest::Client,
    write_url: &str,
    expected_admin_pub: Option<&str>,
) -> FrontendLockServerProbeResult {
    let start = std::time::Instant::now();
    let clean_write_url = write_url.trim().to_string();
    let now_secs = current_unix_timestamp();

    let mut failed = FrontendLockServerProbeResult {
        write_url: clean_write_url.clone(),
        read_url: String::new(),
        drop_pub: String::new(),
        admin_pub: String::new(),
        lock_pub: String::new(),
        delegation_valid: false,
        waf_probe_passed: false,
        selftest_passed: false,
        key_leak_safe: false,
        passed: false,
        latency_ms: 0,
        max_retention_quarter: 0,
        pow_base_bits: 0,
        allow_pow_tickets: false,
        inode_warning: false,
        used_inodes: 0,
        max_inodes: 0,
        probed_at_secs: now_secs,
        error: None,
    };

    if let Err(e) = validate_sync_url(&clean_write_url) {
        failed.error = Some(e);
        failed.latency_ms = start.elapsed().as_millis() as u64;
        return failed;
    }

    let info = match fetch_sync_info(client, &clean_write_url).await {
        Ok(i) => i,
        Err(e) => {
            failed.error = Some(e);
            failed.latency_ms = start.elapsed().as_millis() as u64;
            return failed;
        }
    };

    failed.drop_pub = info.drop_pub.trim().to_ascii_lowercase();
    failed.read_url = info.read_url.trim().to_string();

    if failed.read_url.is_empty() {
        failed.error = Some("Server ?info did not return a read_url".to_string());
        failed.latency_ms = start.elapsed().as_millis() as u64;
        return failed;
    }
    if let Err(e) = validate_sync_url(&failed.read_url) {
        failed.error = Some(format!(
            "Invalid server read_url '{}': {}",
            failed.read_url, e
        ));
        failed.latency_ms = start.elapsed().as_millis() as u64;
        return failed;
    }

    let Some(lock_info) = info.lock else {
        failed.error = Some("Lock channel is not enabled on this server".to_string());
        failed.latency_ms = start.elapsed().as_millis() as u64;
        return failed;
    };

    let lock_pub = lock_info.lock_pub.trim().to_ascii_lowercase();
    failed.lock_pub = lock_pub.clone();
    failed.max_retention_quarter = lock_info.max_retention_quarter;
    failed.pow_base_bits = lock_info.pow_base_bits;
    failed.allow_pow_tickets = lock_info.allow_pow_tickets;
    failed.inode_warning = lock_info.inode_warning;
    failed.used_inodes = lock_info.used_inodes;
    failed.max_inodes = lock_info.max_inodes;

    failed.delegation_valid = lock_info
        .delegation
        .as_ref()
        .map(|d| {
            let valid = verify_lock_delegation(&lock_pub, d, expected_admin_pub);
            if valid {
                failed.admin_pub = d.admin_pub.trim().to_ascii_lowercase();
            }
            valid
        })
        .unwrap_or(false);

    let sep = if clean_write_url.contains('?') {
        "&"
    } else {
        "?"
    };
    let lock_url = format!("{}{}lock", clean_write_url, sep);
    let client_nonce = generate_lock_probe_nonce();
    let probe_body = serde_json::json!({
        "probe": true,
        "client_nonce": client_nonce,
    });
    if let Ok(resp) = client.post(&lock_url).json(&probe_body).send().await {
        if resp.status().is_success() {
            if let Ok(parsed) = resp.json::<SyncLockProbeResponseDto>().await {
                failed.waf_probe_passed =
                    verify_lock_probe_response(&lock_pub, &client_nonce, &parsed);
            }
        }
    }

    let selftest_url = format!("{}{}lock_selftest", clean_write_url, sep);
    if let Ok(resp) = client.get(&selftest_url).send().await {
        if resp.status().is_success() {
            if let Ok(report) = resp.json::<SyncLockSelftestReportDto>().await {
                failed.selftest_passed = verify_lock_selftest(&lock_pub, &report);
            }
        }
    }
    if !failed.selftest_passed {
        if let Some(ref cached_report) = lock_info.selftest {
            failed.selftest_passed = verify_lock_selftest(&lock_pub, cached_report);
        }
    }

    let base_read = normalize_lock_read_base(&failed.read_url);
    let key_probe_url = format!("{}/.lock_key.php", base_read);
    if let Ok(resp) = client.get(&key_probe_url).send().await {
        let code = resp.status().as_u16();
        failed.key_leak_safe = code == 403 || code == 404;
    }

    failed.passed = failed.delegation_valid
        && failed.waf_probe_passed
        && failed.selftest_passed
        && failed.key_leak_safe;
    if !failed.passed {
        failed.error = Some(format!(
            "Lock server probe failed (delegation={}, waf_probe={}, selftest={}, key_leak_safe={})",
            failed.delegation_valid,
            failed.waf_probe_passed,
            failed.selftest_passed,
            failed.key_leak_safe
        ));
    }
    failed.latency_ms = start.elapsed().as_millis() as u64;
    failed
}

pub async fn request_genesis_ticket(
    client: &reqwest::Client,
    write_url: &str,
    anchor_id: &[u8; 32],
    retain_until_quarter: u16,
    pow_bits: Option<u32>,
    caller_signing_key: Option<&SigningKey>,
) -> Result<GenesisTicket, String> {
    validate_sync_url(write_url)?;
    let now_secs = current_unix_timestamp();

    let (pow_ts, pow_nonce) = if caller_signing_key.is_none() || pow_bits.is_some() {
        let bits = pow_bits.unwrap_or(16);
        let found = solve_ticket_pow(
            anchor_id,
            retain_until_quarter,
            now_secs,
            bits,
            0,
            2_000_000,
        )
        .ok_or_else(|| format!("Failed to solve GenesisTicket PoW ({} bits)", bits))?;
        (Some(now_secs), Some(found))
    } else {
        (None, None)
    };

    let req_body = serde_json::json!({
        "anchor_id": hex::encode(anchor_id),
        "retain_until_quarter": retain_until_quarter,
        "pow_timestamp": pow_ts,
        "pow_nonce": pow_nonce,
    });
    let raw_bytes = serde_json::to_vec(&req_body)
        .map_err(|e| format!("Failed to encode lock_ticket request: {}", e))?;

    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}lock_ticket", write_url.trim(), sep);
    let mut req = client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json");

    if let Some(sk) = caller_signing_key {
        let pub_hex = hex::encode(sk.verifying_key().as_bytes());
        let nonce = generate_sync_nonce();
        let body_hash = hex::encode(Sha256::digest(&raw_bytes));
        let msg = format!(
            "HUMOCO-DROP-REQ-V1\nPOST\nlock_ticket\n\n{}\n{}\n{}",
            now_secs, nonce, body_hash
        );
        let sig_hex = hex::encode(sk.sign(msg.as_bytes()).to_bytes());
        req = req
            .header("X-Drop-Pubkey", pub_hex)
            .header("X-Drop-Timestamp", now_secs.to_string())
            .header("X-Drop-Nonce", nonce)
            .header("X-Drop-Signature", sig_hex);
    }

    let resp = req.body(raw_bytes).send().await.map_err(|e| {
        format!(
            "Network error requesting lock_ticket from {}: {}",
            write_url, e
        )
    })?;

    let status = resp.status();
    if status.as_u16() == 429 {
        let secs = parse_retry_after(resp.headers());
        return Err(format!(
            "Rate limited requesting lock_ticket (HTTP 429). Retry-After: {}s",
            secs
        ));
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Server returned HTTP {} on ?lock_ticket: {}",
            status,
            text.trim()
        ));
    }

    let parsed: GenesisTicketResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GenesisTicket JSON: {}", e))?;
    if !parsed.ticket.verify()
        || parsed.ticket.anchor_id != *anchor_id
        || parsed.ticket.retain_until_quarter != retain_until_quarter
    {
        return Err("Returned GenesisTicket failed verification or anchor binding".to_string());
    }
    Ok(parsed.ticket)
}

pub async fn post_lock_batch(
    client: &reqwest::Client,
    write_url: &str,
    request: &SyncLockBatchRequest,
) -> Result<SyncLockBatchOutcome, String> {
    validate_sync_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}lock", write_url.trim(), sep);

    let resp = client
        .post(&url)
        .json(request)
        .send()
        .await
        .map_err(|e| format!("Network error posting lock batch to {}: {}", write_url, e))?;

    let status_code = resp.status().as_u16();
    if status_code == 429 {
        let secs = parse_retry_after(resp.headers());
        return Err(format!(
            "Rate limited posting lock batch to {} (HTTP 429). Retry-After: {}s",
            write_url, secs
        ));
    }

    let text = resp.text().await.map_err(|e| {
        format!(
            "Failed reading lock batch response from {}: {}",
            write_url, e
        )
    })?;

    if status_code == 200 || status_code == 201 || status_code == 409 {
        let val: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
            format!(
                "Invalid JSON in lock batch response (HTTP {}): {}",
                status_code, e
            )
        })?;
        if val.get("receipts").is_some() {
            let body: SyncLockBatchResponse = serde_json::from_value(val)
                .map_err(|e| format!("Invalid SyncLockBatchResponse JSON: {}", e))?;
            return Ok(SyncLockBatchOutcome::Response {
                status_code,
                body: Box::new(body),
            });
        }
        let err_msg = val
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or(text.trim())
            .to_string();
        return Ok(SyncLockBatchOutcome::Rejected {
            status_code,
            error: err_msg,
        });
    }

    Err(format!(
        "Lock server {} returned HTTP {}: {}",
        write_url,
        status_code,
        text.trim()
    ))
}

pub async fn fetch_static_lock_entry(
    client: &reqwest::Client,
    read_url: &str,
    rel_path: &str,
) -> Result<Option<LockEntry>, String> {
    validate_sync_url(read_url)?;
    let base = normalize_lock_read_base(read_url);
    let clean_rel = rel_path.trim().trim_start_matches('/');
    let clean_rel = clean_rel.strip_prefix("locks/").unwrap_or(clean_rel);
    if clean_rel.is_empty() || clean_rel.contains("..") {
        return Err(format!("Invalid static lock path '{}'", rel_path));
    }
    let url = format!("{}/locks/{}", base, clean_rel);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching static lock '{}': {}", url, e))?;

    let code = resp.status().as_u16();
    if code == 404 {
        return Ok(None);
    }
    if code == 429 {
        let secs = parse_retry_after(resp.headers());
        return Err(format!(
            "Rate limited fetching static lock (HTTP 429). Retry-After: {}s",
            secs
        ));
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Server returned HTTP {} fetching static lock '{}'",
            code, url
        ));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed reading static lock bytes: {}", e))?;
    if bytes.len() != LOCK_ENTRY_WIRE_LEN {
        return Err(format!(
            "Invalid static lock entry size {} bytes (expected {} bytes)",
            bytes.len(),
            LOCK_ENTRY_WIRE_LEN
        ));
    }
    let entry = LockEntry::from_wire_bytes(&bytes)
        .map_err(|e| format!("Corrupt static lock entry bytes: {}", e))?;
    Ok(Some(entry))
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
        headers.insert(
            reqwest::header::RETRY_AFTER,
            "invalid_seconds".parse().unwrap(),
        );
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
        let err_short = get_media_blob(&client, "https://example.com/sync", "abc")
            .await
            .unwrap_err();
        assert!(err_short.contains("Invalid blob_id"));

        // Invalid characters (uppercase or non-hex)
        let err_chars = get_media_blob(&client, "https://example.com/sync", &"G".repeat(64))
            .await
            .unwrap_err();
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
        assert!(
            sign_revocation_notice(&sk, 100, 200, Some("bad\nline".to_string()), None).is_err()
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

                let resp: Vec<u8> = if path.ends_with("missing.card.bin") {
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
        let first = get_slot(&client, &read_url, "alice", None, "card", None)
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
        let second = get_slot(&client, &read_url, "alice", None, "card", Some("slot-v1"))
            .await
            .unwrap();
        assert_eq!(second, SlotFetchResult::NotModified);
        assert_eq!(seen_if_none_match.load(Ordering::SeqCst), 1);

        // 3. Missing slot -> NotFound
        let third = get_slot(&client, &read_url, "missing", None, "card", None)
            .await
            .unwrap();
        assert_eq!(third, SlotFetchResult::NotFound);

        server.await.unwrap();
    }

    #[test]
    fn test_ssrf_and_production_https_validation() {
        assert!(validate_sync_url("https://sync.example.com/api.php").is_ok());
        assert!(validate_sync_url("https://10.0.0.1/api.php").is_err());
        assert!(validate_sync_url("https://192.168.1.50/api.php").is_err());
        assert!(validate_sync_url("https://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_sync_url("https://printer.local/sync").is_err());
        assert!(validate_sync_url("https://user:pass@sync.example.com/api.php").is_err());

        // Strict production check (allow_loopback = false):
        assert!(validate_sync_url_strict("http://sync.example.com/api.php", false).is_err());
        assert!(validate_sync_url_strict("http://127.0.0.1:8080/api.php", false).is_err());
        assert!(validate_sync_url_strict("https://sync.example.com/api.php", false).is_ok());

        // Dev loopback allowed:
        assert!(validate_sync_url_strict("http://127.0.0.1:8080/api.php", true).is_ok());
        assert_eq!(
            normalize_lock_read_base("https://sync.example.com/data/locks/"),
            "https://sync.example.com/data"
        );
    }

    #[test]
    fn test_verify_lock_delegation_selftest_and_probe() {
        let admin_sk = SigningKey::from_bytes(&[11u8; 32]);
        let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
        let lock_sk = SigningKey::from_bytes(&[22u8; 32]);
        let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());

        let del_msg = format!(
            "HUMOCO-LOCK-DELEGATE-V1\n{}\n{}\n{}",
            admin_hex, lock_hex, 1700000000u64
        );
        let del = SyncLockDelegationDto {
            admin_pub: admin_hex.clone(),
            lock_pub: lock_hex.clone(),
            issued_at: 1700000000,
            sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
        };
        assert!(verify_lock_delegation(&lock_hex, &del, Some(&admin_hex)));
        assert!(!verify_lock_delegation(
            &lock_hex,
            &del,
            Some(&hex::encode([99u8; 32]))
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
}
