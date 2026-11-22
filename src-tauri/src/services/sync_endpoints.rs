use crate::models::{FrontendError, FrontendSyncDescriptor, FrontendSyncReplacementTarget};
use crate::sync_client;

/// Canonical URL normalization for sync endpoints and probe cache lookups.
/// Lowercases scheme and host (via `url::Url`) while preserving path case and stripping trailing slashes.
pub(crate) fn normalize_sync_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Ok(parsed) = url::Url::parse(trimmed) {
        parsed.as_str().trim_end_matches('/').to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

/// Validates and normalizes an Ed25519 drop public key to lowercase 64-char hex,
/// rejecting malformed, off-curve, and weak small-order points.
pub(crate) fn canonical_drop_pub_hex(raw: &str) -> Result<String, FrontendError> {
    let clean = raw.trim().to_ascii_lowercase();
    humoco_market::parse_ed25519_pubkey_hex64(&clean)
        .map(|_| clean)
        .map_err(|e| FrontendError::from(e.to_string()))
}

/// Returns `true` when an endpoint status indicates permanent/active revocation.
pub(crate) fn is_endpoint_revoked(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("revoked")
}

/// Returns `true` when two endpoints share a normalized non-empty `read_url` or `write_url`.
pub(crate) fn endpoints_overlap(a_read: &str, a_write: &str, b_read: &str, b_write: &str) -> bool {
    let ar = normalize_sync_url(a_read);
    let aw = normalize_sync_url(a_write);
    let br = normalize_sync_url(b_read);
    let bw = normalize_sync_url(b_write);
    (!ar.is_empty() && (ar == br || ar == bw)) || (!aw.is_empty() && (aw == br || aw == bw))
}

/// Returns `true` when two sync descriptors share a normalized `read_url` or `write_url`.
pub(crate) fn descriptors_overlap(a: &FrontendSyncDescriptor, b: &FrontendSyncDescriptor) -> bool {
    endpoints_overlap(&a.read_url, &a.write_url, &b.read_url, &b.write_url)
}

/// Returns `true` when both `drop_pub` strings are non-empty and match case-insensitively after trimming.
pub(crate) fn drop_pub_matches(a: &str, b: &str) -> bool {
    let a_t = a.trim();
    let b_t = b.trim();
    !a_t.is_empty() && a_t.eq_ignore_ascii_case(b_t)
}

/// Checks whether two overlapping descriptors are compatible under key pinning:
/// either key is unpinned/empty, or both keys match.
pub(crate) fn descriptors_compatible_for_policy(
    sel: &FrontendSyncDescriptor,
    ep: &FrontendSyncDescriptor,
) -> bool {
    if !descriptors_overlap(sel, ep) {
        return false;
    }
    let s_dp = sel.drop_pub.trim();
    let e_dp = ep.drop_pub.trim();
    s_dp.is_empty() || e_dp.is_empty() || s_dp.eq_ignore_ascii_case(e_dp)
}

/// Canonical deduplication key for a sync descriptor (`write_url` preferred, fallback `read_url`).
pub fn sync_endpoint_key(d: &FrontendSyncDescriptor) -> String {
    let w = normalize_sync_url(&d.write_url);
    if !w.is_empty() {
        w
    } else {
        normalize_sync_url(&d.read_url)
    }
}

/// Strict validation for a revocation replacement target (`write_url`, `read_url`, `drop_pub` all required and valid).
pub(crate) fn validate_replacement_target(
    rep: &FrontendSyncReplacementTarget,
) -> Result<(), FrontendError> {
    let write = rep.write_url.trim();
    let read = rep.read_url.trim();
    if write.is_empty() || read.is_empty() || rep.drop_pub.trim().is_empty() {
        return Err(FrontendError::from(
            "Replacement sync target must specify writeUrl, readUrl, and dropPub".to_string(),
        ));
    }
    sync_client::validate_sync_url(write).map_err(FrontendError::from)?;
    sync_client::validate_sync_read_url(read).map_err(FrontendError::from)?;
    canonical_drop_pub_hex(&rep.drop_pub).map_err(|e| {
        FrontendError::from(format!(
            "Invalid replacement dropPub '{}': {}",
            rep.drop_pub, e.message
        ))
    })?;
    Ok(())
}

/// Strict validation for a fully configured upload/selected sync descriptor (`write_url`, `read_url`, `drop_pub` required).
pub(crate) fn validate_upload_sync_descriptor(
    d: &FrontendSyncDescriptor,
) -> Result<(), FrontendError> {
    let write = d.write_url.trim();
    let read = d.read_url.trim();
    if write.is_empty() || read.is_empty() || d.drop_pub.trim().is_empty() {
        return Err(FrontendError::from(
            "Sync descriptor must specify writeUrl, readUrl, and dropPub".to_string(),
        ));
    }
    sync_client::validate_sync_url(write).map_err(FrontendError::from)?;
    sync_client::validate_sync_read_url(read).map_err(FrontendError::from)?;
    canonical_drop_pub_hex(&d.drop_pub).map_err(|e| {
        FrontendError::from(format!("Invalid drop_pub '{}': {}", d.drop_pub, e.message))
    })?;
    if let Some(ref sid) = d.slot_id {
        humoco_market::parse_slot_id(sid.trim())
            .map_err(|e| FrontendError::from(format!("Invalid slot_id '{}': {}", sid, e)))?;
    }
    if let Some(ref rep) = d.replacement {
        validate_replacement_target(rep)?;
    }
    Ok(())
}

/// Strict validation for a contact sync descriptor.
///
/// Rejects malformed URLs, weak/non-64-hex Ed25519 `drop_pub`, invalid `slot_id`,
/// or incomplete/malformed `replacement`, while preserving unprobed `write_url`-only
/// QR imports (`read_url`, `drop_pub`, `slot_id` empty and `replacement` `None`).
pub(crate) fn validate_contact_sync_descriptor(
    d: &FrontendSyncDescriptor,
) -> Result<(), FrontendError> {
    let write = d.write_url.trim();
    let read = d.read_url.trim();
    let drop_pub = d.drop_pub.trim();
    if write.is_empty() && read.is_empty() {
        return Err(FrontendError::from(
            "Sync endpoint must specify writeUrl or readUrl".to_string(),
        ));
    }
    if !write.is_empty() {
        sync_client::validate_sync_url(write).map_err(FrontendError::from)?;
    }
    if !read.is_empty() {
        sync_client::validate_sync_read_url(read).map_err(FrontendError::from)?;
    }
    if !drop_pub.is_empty() {
        canonical_drop_pub_hex(drop_pub).map_err(|e| {
            FrontendError::from(format!(
                "Invalid sync dropPub '{}': {}",
                d.drop_pub, e.message
            ))
        })?;
    }
    if let Some(ref sid) = d.slot_id {
        if !sid.trim().is_empty() {
            humoco_market::parse_slot_id(sid.trim()).map_err(|e| {
                FrontendError::from(format!("Invalid sync slotId '{}': {}", sid, e))
            })?;
        }
    }
    if let Some(ref rep) = d.replacement {
        validate_replacement_target(rep)?;
    }
    Ok(())
}

/// Strict validation over a contact's sync descriptor slice.
pub(crate) fn validate_contact_sync_descriptors(
    syncs: &[FrontendSyncDescriptor],
) -> Result<(), FrontendError> {
    for d in syncs {
        validate_contact_sync_descriptor(d)?;
    }
    Ok(())
}

/// Sanitizes untrusted network `DropDescriptor` slices (e.g. `SdCard.payload.drops`, PoST `reply_drops`):
/// normalizes keys/URLs, validates SSRF & Ed25519 pinning, filters malformed entries, and deduplicates
/// up to `max_len` without aborting on a single bad peer entry.
pub(crate) fn sanitize_untrusted_sync_descriptors(
    raw: &[humoco_market::DropDescriptor],
    max_len: usize,
) -> Vec<FrontendSyncDescriptor> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for d in raw {
        if out.len() >= max_len {
            break;
        }
        let write_url = d.write_url.trim().to_string();
        let read_url = d.read_url.trim().to_string();
        if sync_client::validate_sync_url(&write_url).is_err()
            || sync_client::validate_sync_read_url(&read_url).is_err()
        {
            continue;
        }
        let Ok(drop_pub) = canonical_drop_pub_hex(&d.drop_pub) else {
            continue;
        };
        let slot_id = match d
            .slot_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(sid) => {
                if humoco_market::parse_slot_id(sid).is_err() {
                    continue;
                }
                Some(sid.to_string())
            }
            None => None,
        };
        let dedup_key = format!(
            "{}\x00{}",
            normalize_sync_url(&write_url),
            slot_id.as_deref().unwrap_or("")
        );
        if !seen.insert(dedup_key) {
            continue;
        }
        out.push(FrontendSyncDescriptor {
            write_url,
            read_url,
            slot_id,
            drop_pub,
            status: "active".to_string(),
            ..Default::default()
        });
    }
    out
}

/// Sanitizes untrusted `FrontendSyncDescriptor` slices (e.g. IPC `recipient_syncs`, card/reply DTOs):
/// excludes revoked or malformed endpoints, normalizes `drop_pub`/`slot_id`, and drops invalid `replacement` targets.
pub(crate) fn sanitize_untrusted_frontend_sync_descriptors(
    raw: &[FrontendSyncDescriptor],
    max_len: usize,
) -> Vec<FrontendSyncDescriptor> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for d in raw {
        if out.len() >= max_len {
            break;
        }
        if is_endpoint_revoked(&d.status) {
            continue;
        }
        let write_url = d.write_url.trim().to_string();
        let read_url = d.read_url.trim().to_string();
        if sync_client::validate_sync_url(&write_url).is_err()
            || sync_client::validate_sync_read_url(&read_url).is_err()
        {
            continue;
        }
        let Ok(drop_pub) = canonical_drop_pub_hex(&d.drop_pub) else {
            continue;
        };
        let slot_id = match d
            .slot_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(sid) => {
                if humoco_market::parse_slot_id(sid).is_err() {
                    continue;
                }
                Some(sid.to_string())
            }
            None => None,
        };
        let dedup_key = format!(
            "{}\x00{}",
            normalize_sync_url(&write_url),
            slot_id.as_deref().unwrap_or("")
        );
        if !seen.insert(dedup_key) {
            continue;
        }
        let mut sanitized = d.clone();
        sanitized.write_url = write_url;
        sanitized.read_url = read_url;
        sanitized.slot_id = slot_id;
        sanitized.drop_pub = drop_pub;
        sanitized.replacement = sanitized
            .replacement
            .as_ref()
            .filter(|rep| validate_replacement_target(rep).is_ok())
            .map(|rep| FrontendSyncReplacementTarget {
                write_url: rep.write_url.trim().to_string(),
                read_url: rep.read_url.trim().to_string(),
                drop_pub: canonical_drop_pub_hex(&rep.drop_pub).unwrap_or_default(),
            });
        out.push(sanitized);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_ed25519_pub_hex;

    #[test]
    fn s11_x3_strict_and_sanitize_modes_handle_weak_keys_ssrf_and_revocations() {
        let valid_pub = test_ed25519_pub_hex(0x11);
        let valid_pub2 = test_ed25519_pub_hex(0x22);

        // 1. Weak small-order Ed25519 key (0x00*32 and 0x01 || 0x00*31) rejected
        assert!(canonical_drop_pub_hex(&"00".repeat(32)).is_err());
        let mut order8 = [0u8; 32];
        order8[0] = 1;
        assert!(canonical_drop_pub_hex(&hex::encode(order8)).is_err());

        // Uppercase hex normalized to lowercase
        assert_eq!(
            canonical_drop_pub_hex(&format!("  {}  ", valid_pub.to_ascii_uppercase())).unwrap(),
            valid_pub
        );

        // 2. Strict mode preserves unprobed write_url-only QR import
        let qr_only = FrontendSyncDescriptor {
            write_url: "https://qr.example.org/sync.php".to_string(),
            ..Default::default()
        };
        assert!(validate_contact_sync_descriptors(&[qr_only]).is_ok());
        assert!(validate_upload_sync_descriptor(&FrontendSyncDescriptor {
            write_url: "https://qr.example.org/sync.php".to_string(),
            ..Default::default()
        })
        .is_err());

        // Incomplete replacement rejected in strict mode
        let incomplete_rep = FrontendSyncDescriptor {
            write_url: "https://sync.example.org/sync.php".to_string(),
            read_url: "https://sync.example.org/d".to_string(),
            drop_pub: valid_pub.clone(),
            replacement: Some(FrontendSyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: String::new(),
                drop_pub: valid_pub2.clone(),
            }),
            ..Default::default()
        };
        assert!(validate_contact_sync_descriptors(&[incomplete_rep]).is_err());

        // 3. Sanitize mode filters invalid/SSRF/weak/revoked entries while keeping valid ones
        let raw_drops = vec![
            humoco_market::DropDescriptor {
                write_url: "https://169.254.169.254/sync.php".to_string(),
                read_url: "https://169.254.169.254/d".to_string(),
                slot_id: None,
                drop_pub: valid_pub.clone(),
            },
            humoco_market::DropDescriptor {
                write_url: "https://weak.example.org/sync.php".to_string(),
                read_url: "https://weak.example.org/d".to_string(),
                slot_id: None,
                drop_pub: "00".repeat(32),
            },
            humoco_market::DropDescriptor {
                write_url: "https://good.example.org/sync.php".to_string(),
                read_url: "https://good.example.org/d".to_string(),
                slot_id: Some(format!("{}_work", valid_pub2)),
                drop_pub: format!(" {} ", valid_pub.to_ascii_uppercase()),
            },
        ];
        let sanitized = sanitize_untrusted_sync_descriptors(&raw_drops, 3);
        assert_eq!(sanitized.len(), 1);
        assert_eq!(sanitized[0].write_url, "https://good.example.org/sync.php");
        assert_eq!(sanitized[0].drop_pub, valid_pub);

        let raw_fe = vec![
            FrontendSyncDescriptor {
                write_url: "https://revoked.example.org/sync.php".to_string(),
                read_url: "https://revoked.example.org/d".to_string(),
                drop_pub: valid_pub.clone(),
                status: "REVOKED".to_string(),
                ..Default::default()
            },
            FrontendSyncDescriptor {
                write_url: "https://ok.example.org/sync.php".to_string(),
                read_url: "https://ok.example.org/d".to_string(),
                drop_pub: valid_pub.clone(),
                status: "active".to_string(),
                ..Default::default()
            },
        ];
        let sanitized_fe = sanitize_untrusted_frontend_sync_descriptors(&raw_fe, 3);
        assert_eq!(sanitized_fe.len(), 1);
        assert_eq!(sanitized_fe[0].write_url, "https://ok.example.org/sync.php");
    }
}
