use super::http::{
    append_sync_path, append_sync_query, apply_admin_headers, map_unexpected_status,
    read_response_json_bounded, send_sync_request,
};
use super::ssrf::{validate_sync_read_url, validate_sync_url};
use super::*;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use human_money_core::humoco_proto::decode_hex_fixed;

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

pub(super) fn has_outer_ws_or_control(s: &str) -> bool {
    s.trim() != s || s.chars().any(|c| c.is_control())
}

pub(super) fn parse_ed25519_pubkey_strict(hex_str: &str) -> Result<VerifyingKey, SyncClientError> {
    if has_outer_ws_or_control(hex_str) {
        return Err(SyncClientError::LocalValidation(
            "Public key hex contains outer whitespace or control characters".to_string(),
        ));
    }
    let pk_bytes = decode_hex_fixed::<32>(hex_str).map_err(|_| {
        SyncClientError::LocalValidation("Invalid Ed25519 public key hex".to_string())
    })?;
    let vk = VerifyingKey::from_bytes(&pk_bytes)
        .map_err(|_| SyncClientError::LocalValidation("Invalid Ed25519 public key".to_string()))?;
    if vk.is_weak() {
        return Err(SyncClientError::LocalValidation(
            "Ed25519 public key is a weak small-order point".to_string(),
        ));
    }
    Ok(vk)
}

pub fn sign_revocation_notice(
    admin_sk: &SigningKey,
    issued_at: u64,
    effective_at: u64,
    reason: Option<String>,
    replacement: Option<SyncReplacementTarget>,
) -> Result<SyncRevocationNotice, SyncClientError> {
    if issued_at == 0 || effective_at == 0 {
        return Err(SyncClientError::LocalValidation(
            "Revocation notice requires non-zero issued_at and effective_at".to_string(),
        ));
    }
    if let Some(ref r_txt) = reason {
        if r_txt.len() > 200 || r_txt.chars().any(|c| c.is_control()) {
            return Err(SyncClientError::LocalValidation(
                "Invalid revocation reason (max 200 chars, single-line)".to_string(),
            ));
        }
    }
    if let Some(ref rep) = replacement {
        validate_sync_url(&rep.write_url)?;
        validate_sync_read_url(&rep.read_url)?;
        parse_ed25519_pubkey_strict(rep.drop_pub.trim())?;
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
) -> Result<(), SyncClientError> {
    if has_outer_ws_or_control(&notice.drop_pub) || has_outer_ws_or_control(&notice.signature) {
        return Err(SyncClientError::InvalidPayload(
            "Revocation notice contains invalid whitespace or control chars".to_string(),
        ));
    }
    let expected = expected_drop_pub.trim().to_ascii_lowercase();
    let vk = parse_ed25519_pubkey_strict(&expected).map_err(|_| {
        SyncClientError::LocalValidation("Missing or invalid expected drop_pub".to_string())
    })?;
    if notice.drop_pub.to_ascii_lowercase() != expected {
        return Err(SyncClientError::InvalidPayload(
            "Revocation notice drop_pub mismatch".to_string(),
        ));
    }
    if notice.issued_at == 0 || notice.effective_at == 0 {
        return Err(SyncClientError::InvalidPayload(
            "Revocation notice missing timestamps".to_string(),
        ));
    }
    if let Some(ref r_txt) = notice.reason {
        if r_txt.len() > 200 || r_txt.chars().any(|c| c.is_control()) {
            return Err(SyncClientError::InvalidPayload(
                "Revocation notice contains invalid reason".to_string(),
            ));
        }
    }
    if let Some(ref rep) = notice.replacement {
        if has_outer_ws_or_control(&rep.write_url)
            || has_outer_ws_or_control(&rep.read_url)
            || has_outer_ws_or_control(&rep.drop_pub)
        {
            return Err(SyncClientError::InvalidPayload(
                "Revocation notice replacement contains invalid whitespace or control chars"
                    .to_string(),
            ));
        }
        validate_sync_url(&rep.write_url)
            .map_err(|e| SyncClientError::InvalidPayload(e.to_string()))?;
        validate_sync_read_url(&rep.read_url)
            .map_err(|e| SyncClientError::InvalidPayload(e.to_string()))?;
        parse_ed25519_pubkey_strict(&rep.drop_pub).map_err(|_| {
            SyncClientError::InvalidPayload(
                "Revocation notice contains invalid replacement drop_pub".to_string(),
            )
        })?;
    }
    let sig_bytes = decode_hex_fixed::<64>(&notice.signature).map_err(|_| {
        SyncClientError::InvalidPayload("Invalid revocation notice signature hex".to_string())
    })?;
    let sig = Signature::from_bytes(&sig_bytes);

    let canonical = canonical_revocation_message(
        &expected,
        notice.issued_at,
        notice.effective_at,
        notice.replacement.as_ref(),
        notice.reason.as_deref(),
    );
    vk.verify_strict(canonical.as_bytes(), &sig).map_err(|_| {
        SyncClientError::InvalidPayload("Invalid revocation notice Ed25519 signature".to_string())
    })
}

pub async fn fetch_revocation_notice(
    client: &reqwest::Client,
    read_url: &str,
    expected_drop_pub: &str,
) -> Result<Option<SyncRevocationNotice>, SyncClientError> {
    let url = append_sync_path(read_url, "revocation.json")?;
    let resp = client.get(&url).send().await.map_err(|e| {
        SyncClientError::Transport(format!(
            "Network error fetching revocation.json from {}: {}",
            read_url, e
        ))
    })?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    let resp = map_unexpected_status(resp, "fetching revocation.json").await?;
    let notice: SyncRevocationNotice = read_response_json_bounded(
        resp,
        MAX_JSON_META_BODY_BYTES,
        "fetching revocation.json",
        "Failed to parse revocation.json",
    )
    .await?;
    verify_revocation_notice(&notice, expected_drop_pub)?;
    Ok(Some(notice))
}

pub async fn push_revocation_notice(
    client: &reqwest::Client,
    write_url: &str,
    admin_sk: &SigningKey,
    notice: Option<&SyncRevocationNotice>,
    issued_at: u64,
) -> Result<(), SyncClientError> {
    let url = append_sync_query(write_url, "revoke")?;
    let body_json = match notice {
        Some(n) => serde_json::to_string(n).map_err(|e| {
            SyncClientError::LocalValidation(format!(
                "Failed to serialize revocation notice: {}",
                e
            ))
        })?,
        None => serde_json::to_string(&serde_json::json!({
            "issued_at": issued_at,
            "effective_at": null
        }))
        .map_err(|e| {
            SyncClientError::LocalValidation(format!(
                "Failed to serialize revocation clear payload: {}",
                e
            ))
        })?,
    };

    let req = apply_admin_headers(
        client.put(&url),
        admin_sk,
        "PUT_REVOKE",
        "",
        body_json.as_bytes(),
    )
    .header("Content-Type", "application/json")
    .body(body_json);

    send_sync_request(req, "pushing revocation").await?;
    Ok(())
}

pub async fn push_lock_delegation(
    client: &reqwest::Client,
    write_url: &str,
    read_url: &str,
    admin_sk: &SigningKey,
    lock_pub_hex: &str,
    issued_at: u64,
) -> Result<SyncLockDelegationDto, SyncClientError> {
    let url = append_sync_query(write_url, "lock_delegate")?;
    let clean_write = write_url.trim().to_string();
    let clean_read = read_url.trim().to_string();
    if validate_sync_url(&clean_write).is_err() || validate_sync_read_url(&clean_read).is_err() {
        return Err(SyncClientError::LocalValidation(
            "Invalid write_url or read_url for lock delegation".to_string(),
        ));
    }
    let lock_pub = lock_pub_hex.trim().to_ascii_lowercase();
    parse_ed25519_pubkey_strict(&lock_pub).map_err(|_| {
        SyncClientError::LocalValidation("Invalid Ed25519 lock_pub for lock delegation".to_string())
    })?;
    let admin_pub = hex::encode(admin_sk.verifying_key().as_bytes());
    let msg = humoco_market::format_lock_delegate_signature_message(
        &admin_pub,
        &lock_pub,
        &clean_write,
        &clean_read,
        issued_at,
    );
    let sig = hex::encode(admin_sk.sign(msg.as_bytes()).to_bytes());
    let payload = SyncLockDelegationDto {
        admin_pub,
        lock_pub,
        write_url: clean_write,
        read_url: clean_read,
        issued_at,
        sig,
    };
    let body_json = serde_json::to_string(&payload).map_err(|e| {
        SyncClientError::LocalValidation(format!(
            "Failed to serialize lock delegation payload: {}",
            e
        ))
    })?;

    let req = apply_admin_headers(
        client.put(&url),
        admin_sk,
        "PUT_LOCK_DELEGATE",
        "",
        body_json.as_bytes(),
    )
    .header("Content-Type", "application/json")
    .body(body_json);

    let resp = send_sync_request(req, "pushing lock delegation").await?;
    read_response_json_bounded(
        resp,
        MAX_JSON_META_BODY_BYTES,
        "pushing lock delegation",
        "Failed to parse lock delegation response JSON",
    )
    .await
}
