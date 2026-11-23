use super::http::{
    append_sync_query, apply_signed_headers, is_lower_hex_exact, map_unexpected_status,
    post_sync_json, rate_limit_error, read_error_snippet_bounded, read_response_bytes_bounded,
    read_response_json_bounded, sanitize_error_snippet, send_sync_request,
    validate_static_lock_rel_path, validated_sync_url,
};
use super::revocation::{has_outer_ws_or_control, parse_ed25519_pubkey_strict};
use super::slots::fetch_sync_info;
use super::ssrf::{validate_sync_read_url, validate_sync_url};
use super::*;
use ed25519_dalek::{Signature, VerifyingKey};
use human_money_core::humoco_proto::{
    causal::{LockEntry, LOCK_ENTRY_WIRE_LEN},
    crypto::{solve_ticket_pow, verify_ed25519_strict},
    decode_hex_fixed, AnchorManifest, CommitteeSignature, GenesisTicket, ImportAck, MigrationCert,
    MigrationProposal, QuorumCertificate, SnapshotExport,
};
use human_money_core::models::layer2_api::{SyncLockBatchRequest, SyncLockBatchResponse};
use serde::Deserialize;
use sha2::{Digest, Sha256};

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

pub(super) fn verify_ed25519_hex_strict(pubkey_hex: &str, sig_hex: &str, msg: &[u8]) -> bool {
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
    if delegation.issued_at == 0
        || has_outer_ws_or_control(&delegation.write_url)
        || has_outer_ws_or_control(&delegation.read_url)
        || has_outer_ws_or_control(&delegation.lock_pub)
        || has_outer_ws_or_control(&delegation.admin_pub)
        || has_outer_ws_or_control(&delegation.sig)
    {
        return false;
    }
    if validate_sync_url(&delegation.write_url).is_err()
        || validate_sync_read_url(&delegation.read_url).is_err()
        || parse_ed25519_pubkey_strict(&delegation.lock_pub).is_err()
    {
        return false;
    }
    let lock_lower = delegation.lock_pub.to_ascii_lowercase();
    let admin_lower = delegation.admin_pub.to_ascii_lowercase();
    if lock_lower != info_lock_pub.trim().to_ascii_lowercase() {
        return false;
    }
    if let Some(exp) = expected_admin_pub {
        if admin_lower != exp.trim().to_ascii_lowercase() {
            return false;
        }
    }
    let msg = humoco_market::format_lock_delegate_signature_message(
        &admin_lower,
        &lock_lower,
        &delegation.write_url,
        &delegation.read_url,
        delegation.issued_at,
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

fn evaluate_probe_delegation(
    lock_pub: &str,
    delegation: Option<&SyncLockDelegationDto>,
    expected_admin_pub: Option<&str>,
    norm_probe_write: &str,
    norm_probe_read: &str,
) -> (bool, String, Option<SyncLockDelegationDto>) {
    let Some(d) = delegation else {
        return (false, String::new(), None);
    };
    let d_write = d.write_url.trim();
    let d_read = d.read_url.trim();
    let valid = verify_lock_delegation(lock_pub, d, expected_admin_pub)
        && validate_sync_url(d_write).is_ok()
        && validate_sync_read_url(d_read).is_ok()
        && crate::services::common::normalize_sync_url(d_write) == norm_probe_write
        && crate::services::common::normalize_sync_url(d_read) == norm_probe_read;
    if valid {
        (
            true,
            d.admin_pub.trim().to_ascii_lowercase(),
            Some(d.clone()),
        )
    } else {
        (false, String::new(), None)
    }
}

async fn run_waf_lock_probe(client: &reqwest::Client, write_url: &str, lock_pub: &str) -> bool {
    let Ok(lock_url) = append_sync_query(write_url, "lock") else {
        return false;
    };
    let client_nonce = generate_lock_probe_nonce();
    let probe_body = serde_json::json!({
        "probe": true,
        "client_nonce": client_nonce,
    });
    let Ok(resp) = client.post(&lock_url).json(&probe_body).send().await else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(parsed) = read_response_json_bounded::<SyncLockProbeResponseDto>(
        resp,
        MAX_JSON_META_BODY_BYTES,
        "probing lock server",
        "Failed to parse lock probe response",
    )
    .await
    else {
        return false;
    };
    verify_lock_probe_response(lock_pub, &client_nonce, &parsed)
}

async fn run_selftest_probe(
    client: &reqwest::Client,
    write_url: &str,
    lock_pub: &str,
    cached_report: Option<&SyncLockSelftestReportDto>,
) -> bool {
    if let Ok(selftest_url) = append_sync_query(write_url, "lock_selftest") {
        if let Ok(resp) = client.get(&selftest_url).send().await {
            if resp.status().is_success() {
                if let Ok(report) = read_response_json_bounded::<SyncLockSelftestReportDto>(
                    resp,
                    MAX_JSON_META_BODY_BYTES,
                    "fetching lock selftest",
                    "Failed to parse lock selftest report",
                )
                .await
                {
                    if verify_lock_selftest(lock_pub, &report) {
                        return true;
                    }
                }
            }
        }
    }
    cached_report
        .map(|cached| verify_lock_selftest(lock_pub, cached))
        .unwrap_or(false)
}

async fn run_key_leak_probe(client: &reqwest::Client, read_url: &str) -> bool {
    let base_read = normalize_lock_read_base(read_url);
    let key_probe_url = format!("{}/.lock_key.php", base_read);
    let Ok(resp) = client.get(&key_probe_url).send().await else {
        return false;
    };
    let code = resp.status().as_u16();
    code == 403 || code == 404
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
        ticket_pow_bits: 0,
        inode_warning: false,
        used_inodes: 0,
        max_inodes: 0,
        probed_at_secs: now_secs,
        delegation: None,
        error: None,
    };

    if let Err(e) = validate_sync_url(&clean_write_url) {
        failed.error = Some(e.to_string());
        failed.latency_ms = start.elapsed().as_millis() as u64;
        return failed;
    }

    let info = match fetch_sync_info(client, &clean_write_url).await {
        Ok(i) => i,
        Err(e) => {
            failed.error = Some(e.to_string());
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
    if let Err(e) = validate_sync_read_url(&failed.read_url) {
        failed.error = Some(format!(
            "Invalid server read_url '{}': {}",
            failed.read_url, e
        ));
        failed.latency_ms = start.elapsed().as_millis() as u64;
        return failed;
    }

    if let Some(ref rev) = info.revocation {
        let rev_admin = expected_admin_pub
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                info.lock
                    .as_ref()
                    .and_then(|l| l.delegation.as_ref())
                    .map(|d| d.admin_pub.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| failed.drop_pub.clone());
        if !rev_admin.is_empty()
            && verify_revocation_notice(rev, &rev_admin).is_ok()
            && rev.effective_at <= now_secs
        {
            failed.admin_pub = rev_admin;
            failed.passed = false;
            failed.error = Some("Server is revoked".to_string());
            failed.latency_ms = start.elapsed().as_millis() as u64;
            return failed;
        }
    }

    let Some(lock_info) = info.lock else {
        failed.error = Some("Lock channel is not enabled on this server".to_string());
        failed.latency_ms = start.elapsed().as_millis() as u64;
        return failed;
    };

    let lock_pub = lock_info.lock_pub.trim().to_ascii_lowercase();
    failed.lock_pub = lock_pub.clone();
    failed.max_retention_quarter = lock_info.max_retention_quarter;
    failed.pow_base_bits = lock_info
        .pow_base_bits
        .min(human_money_core::humoco_proto::crypto::MAX_LOCK_POW_BITS);
    failed.allow_pow_tickets = lock_info.allow_pow_tickets;
    failed.ticket_pow_bits = lock_info
        .ticket_pow_bits
        .min(human_money_core::humoco_proto::crypto::MAX_LOCK_POW_BITS);
    failed.inode_warning = lock_info.inode_warning;
    failed.used_inodes = lock_info.used_inodes;
    failed.max_inodes = lock_info.max_inodes;

    let norm_probe_write = crate::services::common::normalize_sync_url(&clean_write_url);
    let norm_probe_read = crate::services::common::normalize_sync_url(&failed.read_url);
    let (delegation_valid, admin_pub, delegation) = evaluate_probe_delegation(
        &lock_pub,
        lock_info.delegation.as_ref(),
        expected_admin_pub,
        &norm_probe_write,
        &norm_probe_read,
    );
    failed.delegation_valid = delegation_valid;
    if delegation_valid {
        failed.admin_pub = admin_pub;
        failed.delegation = delegation;
    }

    failed.waf_probe_passed = run_waf_lock_probe(client, &clean_write_url, &lock_pub).await;
    failed.selftest_passed = run_selftest_probe(
        client,
        &clean_write_url,
        &lock_pub,
        lock_info.selftest.as_ref(),
    )
    .await;

    if failed.delegation_valid {
        failed.key_leak_safe = run_key_leak_probe(client, &failed.read_url).await;
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

struct CancelFlagDropGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for CancelFlagDropGuard {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub(super) fn next_pow_start_nonce() -> u64 {
    let mut start_bytes = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut start_bytes);
    // Keep start_nonce + max_total well below i64::MAX so PHP json_decode parses pow_nonce as int.
    u64::from_le_bytes(start_bytes) & POW_START_NONCE_MASK
}

pub async fn request_genesis_ticket(
    client: &reqwest::Client,
    write_url: &str,
    expected_lock_pub: [u8; 32],
    anchor_id: &[u8; 32],
    retain_until_quarter: u16,
    auth: LockTicketAuth,
) -> Result<GenesisTicket, SyncClientError> {
    let url = append_sync_query(write_url, "lock_ticket")?;
    let now_secs = current_unix_timestamp();

    let (pow_ts, pow_nonce) = match &auth {
        LockTicketAuth::Pow { pow_bits } => {
            if *pow_bits > human_money_core::humoco_proto::crypto::MAX_LOCK_POW_BITS {
                return Err(SyncClientError::LocalValidation(format!(
                    "Unsupported GenesisTicket PoW bits: {}",
                    pow_bits
                )));
            }
            let bits = *pow_bits;
            let aid = *anchor_id;
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let _cancel_guard = CancelFlagDropGuard(cancel.clone());
            let start_nonce = next_pow_start_nonce();
            let max_total = if bits <= 18 {
                2_000_000u64
            } else {
                8_000_000u64
            };
            let found = tokio::task::spawn_blocking(move || {
                let mut offset = 0u64;
                let chunk = 50_000u64;
                while offset < max_total {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        return None;
                    }
                    let step = (max_total - offset).min(chunk);
                    if let Some(nonce) = solve_ticket_pow(
                        &aid,
                        retain_until_quarter,
                        now_secs,
                        bits,
                        start_nonce.wrapping_add(offset),
                        step,
                    ) {
                        return Some(nonce);
                    }
                    offset += step;
                }
                None
            })
            .await
            .map_err(|e| SyncClientError::LocalValidation(format!("PoW solver join error: {}", e)))?
            .ok_or_else(|| {
                SyncClientError::LocalValidation(format!(
                    "Failed to solve GenesisTicket PoW ({} bits)",
                    bits
                ))
            })?;
            (Some(now_secs), Some(found))
        }
        LockTicketAuth::Direct(_) | LockTicketAuth::Stealth { .. } => (None, None),
    };

    let req_body = serde_json::json!({
        "anchor_id": hex::encode(anchor_id),
        "retain_until_quarter": retain_until_quarter,
        "pow_timestamp": pow_ts,
        "pow_nonce": pow_nonce,
    });
    let raw_bytes = serde_json::to_vec(&req_body).map_err(|e| {
        SyncClientError::LocalValidation(format!("Failed to encode lock_ticket request: {}", e))
    })?;

    let mut req = client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json");

    match &auth {
        LockTicketAuth::Direct(sk) => {
            let nonce = generate_sync_nonce();
            let signed = humoco_market::sign_member_request_direct(
                sk,
                "POST",
                "lock_ticket",
                None,
                now_secs,
                &nonce,
                &raw_bytes,
            )
            .map_err(|e| {
                SyncClientError::LocalValidation(format!(
                    "Failed direct lock_ticket signing: {}",
                    e
                ))
            })?;
            req = apply_signed_headers(req, &signed, false);
        }
        LockTicketAuth::Stealth {
            wallet_ed_sk,
            drop_pub_hex,
        } => {
            let nonce = generate_sync_nonce();
            let signed = humoco_market::sign_member_request_stealth(
                wallet_ed_sk,
                drop_pub_hex,
                "POST",
                "lock_ticket",
                None,
                now_secs,
                &nonce,
                &raw_bytes,
            )
            .map_err(|e| {
                SyncClientError::LocalValidation(format!(
                    "Failed stealth lock_ticket signing: {}",
                    e
                ))
            })?;
            req = apply_signed_headers(req, &signed, false);
        }
        LockTicketAuth::Pow { .. } => {}
    }

    let resp = send_sync_request(
        req.body(raw_bytes),
        &format!("requesting lock_ticket from {}", write_url),
    )
    .await?;

    let parsed: GenesisTicketResponse = read_response_json_bounded(
        resp,
        MAX_JSON_META_BODY_BYTES,
        "parsing GenesisTicket",
        "Failed to parse GenesisTicket JSON",
    )
    .await?;
    if !parsed.ticket.verify()
        || parsed.ticket.lock_pub != expected_lock_pub
        || parsed.ticket.anchor_id != *anchor_id
        || parsed.ticket.retain_until_quarter != retain_until_quarter
    {
        return Err(SyncClientError::InvalidPayload(
            "Returned GenesisTicket failed verification or lock_pub/anchor binding".to_string(),
        ));
    }
    Ok(parsed.ticket)
}

pub async fn post_lock_batch(
    client: &reqwest::Client,
    write_url: &str,
    request: &SyncLockBatchRequest,
) -> Result<SyncLockBatchOutcome, SyncClientError> {
    let url = append_sync_query(write_url, "lock")?;

    let resp = client.post(&url).json(request).send().await.map_err(|e| {
        SyncClientError::Transport(format!(
            "Network error posting lock batch to {}: {}",
            write_url, e
        ))
    })?;

    let status_code = resp.status().as_u16();
    if status_code == 429 {
        return Err(rate_limit_error(
            &format!("posting lock batch to {}", write_url),
            resp.headers(),
        ));
    }

    if status_code == 410 {
        let bytes = read_response_bytes_bounded(
            resp,
            MAX_LOCK_BATCH_BODY_BYTES,
            &format!("reading lock batch response from {}", write_url),
        )
        .await?;
        let notice = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|v| v.get("revocation").cloned())
            .and_then(|rv| serde_json::from_value::<SyncRevocationNotice>(rv).ok());
        return Ok(SyncLockBatchOutcome::Revoked {
            status_code,
            notice: notice.map(Box::new),
        });
    }

    if status_code == 200 || status_code == 201 || status_code == 409 {
        let bytes = read_response_bytes_bounded(
            resp,
            MAX_LOCK_BATCH_BODY_BYTES,
            &format!("reading lock batch response from {}", write_url),
        )
        .await?;
        let val: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
            SyncClientError::InvalidPayload(format!(
                "Invalid JSON in lock batch response (HTTP {}): {}",
                status_code, e
            ))
        })?;
        if val.get("receipts").is_some() {
            let body: SyncLockBatchResponse = serde_json::from_value(val).map_err(|e| {
                SyncClientError::InvalidPayload(format!(
                    "Invalid SyncLockBatchResponse JSON: {}",
                    e
                ))
            })?;
            return Ok(SyncLockBatchOutcome::Response {
                status_code,
                body: Box::new(body),
            });
        }
        let err_msg = val
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| sanitize_error_snippet(&bytes));
        if status_code == 409 && err_msg == "epoch_frozen" {
            let latest_manifest = val
                .get("latest_manifest")
                .cloned()
                .and_then(|m| serde_json::from_value::<AnchorManifest>(m).ok());
            return Ok(SyncLockBatchOutcome::EpochFrozen {
                status_code,
                error: err_msg,
                latest_manifest: latest_manifest.map(Box::new),
            });
        }
        return Ok(SyncLockBatchOutcome::Rejected {
            status_code,
            error: err_msg,
        });
    }

    let detail = read_error_snippet_bounded(resp, MAX_ERROR_BODY_BYTES).await;
    Err(SyncClientError::HttpStatus {
        status: status_code,
        retry_after_secs: None,
        context: format!("on lock server {}", write_url),
        snippet: detail,
    })
}

pub async fn get_lock_freeze_status(
    client: &reqwest::Client,
    write_url: &str,
    anchor_id_hex: &str,
) -> Result<Option<LockFreezeStatusDto>, SyncClientError> {
    let trimmed_anchor = anchor_id_hex.trim();
    if trimmed_anchor != anchor_id_hex
        || trimmed_anchor
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(SyncClientError::LocalValidation(
            "Invalid anchor_id hex".to_string(),
        ));
    }
    let aid = decode_hex_fixed::<32>(trimmed_anchor)
        .map_err(|e| SyncClientError::LocalValidation(format!("Invalid anchor_id hex: {}", e)))?;
    let url = append_sync_query(
        write_url,
        &format!("lock_freeze&anchor_id={}", hex::encode(aid)),
    )?;
    let resp = client.get(&url).send().await.map_err(|e| {
        SyncClientError::Transport(format!(
            "Network error querying ?lock_freeze on {}: {}",
            write_url, e
        ))
    })?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    let resp =
        map_unexpected_status(resp, &format!("querying ?lock_freeze on {}", write_url)).await?;
    let parsed: LockFreezeStatusDto = read_response_json_bounded(
        resp,
        MAX_FREEZE_JSON_BODY_BYTES,
        "querying ?lock_freeze",
        "Invalid ?lock_freeze status JSON",
    )
    .await?;
    Ok(Some(parsed))
}

pub async fn post_lock_freeze_action(
    client: &reqwest::Client,
    write_url: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, SyncClientError> {
    let resp = post_sync_json(
        client,
        write_url,
        "lock_freeze",
        body,
        &format!("posting ?lock_freeze to {}", write_url),
    )
    .await?;
    read_response_json_bounded(
        resp,
        MAX_FREEZE_JSON_BODY_BYTES,
        "posting ?lock_freeze",
        "Invalid POST ?lock_freeze response JSON",
    )
    .await
}

#[derive(Deserialize)]
struct LockImportResponseDto {
    import_ack: ImportAck,
}

pub async fn post_lock_import(
    client: &reqwest::Client,
    write_url: &str,
    anchor_id: &[u8; 32],
    active_manifest: &AnchorManifest,
    proposal: &MigrationProposal,
    snapshots: &[SnapshotExport],
    quorum_certificates: &[QuorumCertificate],
) -> Result<ImportAck, SyncClientError> {
    let payload = serde_json::json!({
        "anchor_id": hex::encode(anchor_id),
        "active_manifest": active_manifest,
        "proposal": proposal,
        "snapshots": snapshots,
        "quorum_certificates": quorum_certificates,
    });
    let resp = post_sync_json(
        client,
        write_url,
        "lock_import",
        &payload,
        &format!("posting ?lock_import to {}", write_url),
    )
    .await?;
    let parsed: LockImportResponseDto = read_response_json_bounded(
        resp,
        MAX_JSON_META_BODY_BYTES,
        "posting ?lock_import",
        "Invalid ?lock_import response JSON",
    )
    .await?;
    if parsed.import_ack.anchor_id != *anchor_id || !parsed.import_ack.verify() {
        return Err(SyncClientError::InvalidPayload(
            "Returned ImportAck failed signature or anchor_id verification".to_string(),
        ));
    }
    Ok(parsed.import_ack)
}

#[derive(Deserialize)]
struct LockCommitSignResponseDto {
    committee_signature: CommitteeSignature,
}

pub async fn post_lock_commit_sign(
    client: &reqwest::Client,
    write_url: &str,
    anchor_id: &[u8; 32],
    proposal: &MigrationProposal,
    imported_state_hash: &[u8; 32],
    import_acks: &[ImportAck],
) -> Result<CommitteeSignature, SyncClientError> {
    let payload = serde_json::json!({
        "action": "sign_commit",
        "anchor_id": hex::encode(anchor_id),
        "proposal": proposal,
        "imported_state_hash": hex::encode(imported_state_hash),
        "import_acks": import_acks,
    });
    let resp = post_sync_json(
        client,
        write_url,
        "lock_commit",
        &payload,
        &format!("posting ?lock_commit sign_commit to {}", write_url),
    )
    .await?;
    let parsed: LockCommitSignResponseDto = read_response_json_bounded(
        resp,
        MAX_JSON_META_BODY_BYTES,
        "posting ?lock_commit",
        "Invalid ?lock_commit response JSON",
    )
    .await?;
    let prop_hash = proposal.core_digest().map_err(|e| {
        SyncClientError::LocalValidation(format!("Invalid migration proposal: {}", e))
    })?;
    let digest =
        MigrationCert::commit_digest(anchor_id, &prop_hash, proposal.to_rev, imported_state_hash);
    if !verify_ed25519_strict(
        &parsed.committee_signature.lock_pub,
        &digest,
        &parsed.committee_signature.sig,
    ) {
        return Err(SyncClientError::InvalidPayload(
            "Returned CommitteeSignature failed verification".to_string(),
        ));
    }
    Ok(parsed.committee_signature)
}

pub async fn post_lock_commit_finalize(
    client: &reqwest::Client,
    write_url: &str,
    anchor_id: &[u8; 32],
    new_manifest: &AnchorManifest,
) -> Result<(), SyncClientError> {
    let payload = serde_json::json!({
        "action": "finalize",
        "anchor_id": hex::encode(anchor_id),
        "new_manifest": new_manifest,
    });
    post_sync_json(
        client,
        write_url,
        "lock_commit",
        &payload,
        &format!("finalizing ?lock_commit on {}", write_url),
    )
    .await?;
    Ok(())
}

pub fn anchor_manifest_candidate_urls(
    base_url: &str,
    anchor_hex: &str,
) -> Result<Vec<String>, SyncClientError> {
    if !is_lower_hex_exact(anchor_hex, 64) {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid anchor_hex '{}': expected 64 lowercase hex chars",
            anchor_hex
        )));
    }
    let trimmed = validated_sync_url(base_url)?.trim_end_matches('/');
    let parsed = reqwest::Url::parse(trimmed).map_err(|e| {
        SyncClientError::LocalValidation(format!("Invalid Sync URL '{}': {}", base_url, e))
    })?;
    let Some(host) = parsed.host_str() else {
        return Err(SyncClientError::LocalValidation(format!(
            "Sync URL missing host: '{}'",
            base_url
        )));
    };
    let port_part = parsed.port().map(|p| format!(":{}", p)).unwrap_or_default();
    let origin = format!("{}://{}{}", parsed.scheme(), host, port_part);
    let path_trimmed = parsed.path().trim_end_matches('/');
    let mut urls = Vec::new();
    if path_trimmed.ends_with("/sync.php") || path_trimmed.ends_with("/sync") {
        let parent = path_trimmed
            .rsplit_once('/')
            .map(|(prefix, _)| prefix)
            .unwrap_or("");
        urls.push(format!("{}{}/anchors/{}.bin", origin, parent, anchor_hex));
    } else {
        if parsed.query().is_some() {
            return Err(SyncClientError::LocalValidation(format!(
                "Sync read URL must not contain a query string: '{}'",
                base_url
            )));
        }
        let read_path = path_trimmed
            .strip_suffix("/locks")
            .unwrap_or(path_trimmed)
            .trim_end_matches('/');
        urls.push(format!(
            "{}{}/anchors/{}.bin",
            origin, read_path, anchor_hex
        ));
        urls.push(format!("{}/anchors/{}.bin", origin, anchor_hex));
    }
    urls.dedup();
    Ok(urls)
}

pub async fn fetch_anchor_manifest_bin(
    client: &reqwest::Client,
    base_url: &str,
    anchor_id: &[u8; 32],
) -> Result<Option<AnchorManifest>, SyncClientError> {
    let anchor_hex = hex::encode(anchor_id);
    let urls = anchor_manifest_candidate_urls(base_url, &anchor_hex)?;
    for url in urls {
        let Ok(resp) = client.get(&url).send().await else {
            continue;
        };
        let code = resp.status().as_u16();
        if code == 404 {
            continue;
        }
        if code == 429 {
            return Err(rate_limit_error("fetching anchor manifest", resp.headers()));
        }
        if !resp.status().is_success() {
            continue;
        }
        let Ok(bytes) = read_response_bytes_bounded(
            resp,
            MAX_LOCK_BATCH_BODY_BYTES,
            "fetching anchor manifest",
        )
        .await
        else {
            continue;
        };
        let Ok(manifest) = serde_json::from_slice::<AnchorManifest>(&bytes) else {
            continue;
        };
        if manifest.verify_anchor_id(anchor_id).is_ok() {
            return Ok(Some(manifest));
        }
    }
    Ok(None)
}

pub async fn fetch_static_lock_entry(
    client: &reqwest::Client,
    read_url: &str,
    rel_path: &str,
) -> Result<Option<LockEntry>, SyncClientError> {
    validate_sync_read_url(read_url)?;
    let base = normalize_lock_read_base(read_url);
    let clean_rel = validate_static_lock_rel_path(rel_path)?;
    let url = format!("{}/locks/{}", base, clean_rel);

    let resp = client.get(&url).send().await.map_err(|e| {
        SyncClientError::Transport(format!(
            "Network error fetching static lock '{}': {}",
            url, e
        ))
    })?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    let resp = map_unexpected_status(resp, &format!("fetching static lock '{}'", url)).await?;

    let bytes =
        read_response_bytes_bounded(resp, LOCK_ENTRY_WIRE_LEN, "reading static lock bytes").await?;
    if bytes.len() != LOCK_ENTRY_WIRE_LEN {
        return Err(SyncClientError::InvalidPayload(format!(
            "Invalid static lock entry size {} bytes (expected {} bytes)",
            bytes.len(),
            LOCK_ENTRY_WIRE_LEN
        )));
    }
    let entry = LockEntry::from_wire_bytes(&bytes).map_err(|e| {
        SyncClientError::InvalidPayload(format!("Corrupt static lock entry bytes: {}", e))
    })?;
    Ok(Some(entry))
}
