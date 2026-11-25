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
    fn test_canonical_drop_pub_hex_rejects_weak_and_malformed_keys() {
        let valid_pub = test_ed25519_pub_hex(0x11);
        assert_eq!(
            canonical_drop_pub_hex(&format!("  {}  ", valid_pub.to_ascii_uppercase())).unwrap(),
            valid_pub
        );

        let err_zero = canonical_drop_pub_hex(&"00".repeat(32))
            .expect_err("all-zero Ed25519 key must be rejected");
        assert!(
            err_zero.message.contains("weak small-order point"),
            "unexpected error: {}",
            err_zero.message
        );

        let mut order8 = [0u8; 32];
        order8[0] = 1;
        let err_order8 = canonical_drop_pub_hex(&hex::encode(order8))
            .expect_err("order-8 Ed25519 point must be rejected");
        assert!(
            err_order8.message.contains("weak small-order point"),
            "unexpected error: {}",
            err_order8.message
        );

        let err_non_hex = canonical_drop_pub_hex("not-hex")
            .expect_err("non-hex drop_pub must be rejected");
        assert!(
            err_non_hex.message.contains("64-character lowercase hex"),
            "unexpected error: {}",
            err_non_hex.message
        );

        let err_short = canonical_drop_pub_hex(&"aa".repeat(31))
            .expect_err("62-hex drop_pub must be rejected");
        assert!(
            err_short.message.contains("64-character lowercase hex"),
            "unexpected error: {}",
            err_short.message
        );
    }

    #[test]
    fn test_validate_upload_sync_descriptor_strict_gates() {
        let valid_pub = test_ed25519_pub_hex(0x11);
        let valid_pub2 = test_ed25519_pub_hex(0x22);
        let valid_slot = format!("{}_work", valid_pub2);

        let valid_upload = FrontendSyncDescriptor {
            write_url: "https://upload.example.org/sync.php".to_string(),
            read_url: "https://upload.example.org/d".to_string(),
            drop_pub: valid_pub,
            slot_id: Some(valid_slot),
            replacement: Some(FrontendSyncReplacementTarget {
                write_url: "https://next.example.org/sync.php".to_string(),
                read_url: "https://next.example.org/d".to_string(),
                drop_pub: valid_pub2.clone(),
            }),
            ..Default::default()
        };
        validate_upload_sync_descriptor(&valid_upload).expect("full upload descriptor must pass");

        for (label, partial) in [
            (
                "missing write_url",
                FrontendSyncDescriptor {
                    write_url: String::new(),
                    ..valid_upload.clone()
                },
            ),
            (
                "missing read_url",
                FrontendSyncDescriptor {
                    read_url: String::new(),
                    ..valid_upload.clone()
                },
            ),
            (
                "missing drop_pub",
                FrontendSyncDescriptor {
                    drop_pub: String::new(),
                    ..valid_upload.clone()
                },
            ),
        ] {
            let err = validate_upload_sync_descriptor(&partial)
                .expect_err(&format!("upload descriptor {} must fail", label));
            assert!(
                err.message
                    .contains("must specify writeUrl, readUrl, and dropPub"),
                "unexpected error for {}: {}",
                label,
                err.message
            );
        }

        let bad_slot_upload = validate_upload_sync_descriptor(&FrontendSyncDescriptor {
            slot_id: Some("bad slot".to_string()),
            ..valid_upload.clone()
        })
        .expect_err("upload descriptor with invalid slot_id must fail");
        assert!(
            bad_slot_upload.message.contains("Invalid slot_id"),
            "unexpected error: {}",
            bad_slot_upload.message
        );

        let bad_pub_upload = validate_upload_sync_descriptor(&FrontendSyncDescriptor {
            drop_pub: "not-hex".to_string(),
            ..valid_upload.clone()
        })
        .expect_err("upload descriptor with invalid drop_pub must fail");
        assert!(
            bad_pub_upload.message.contains("Invalid drop_pub"),
            "unexpected error: {}",
            bad_pub_upload.message
        );

        let bad_rep_upload = validate_upload_sync_descriptor(&FrontendSyncDescriptor {
            replacement: Some(FrontendSyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: String::new(),
                drop_pub: valid_pub2,
            }),
            ..valid_upload
        })
        .expect_err("upload descriptor with incomplete replacement must fail");
        assert!(
            bad_rep_upload
                .message
                .contains("Replacement sync target must specify"),
            "unexpected error: {}",
            bad_rep_upload.message
        );
    }

    #[test]
    fn test_validate_contact_sync_descriptors_strict_gates() {
        let valid_pub = test_ed25519_pub_hex(0x11);
        let valid_pub2 = test_ed25519_pub_hex(0x22);

        let qr_only = FrontendSyncDescriptor {
            write_url: "https://qr.example.org/sync.php".to_string(),
            ..Default::default()
        };
        validate_contact_sync_descriptors(&[qr_only]).expect("unprobed QR import must pass");

        let read_only_contact = FrontendSyncDescriptor {
            read_url: "https://qr.example.org/d".to_string(),
            ..Default::default()
        };
        validate_contact_sync_descriptors(&[read_only_contact])
            .expect("read_url-only contact descriptor must pass");

        let blank_slot_contact = FrontendSyncDescriptor {
            write_url: "https://qr.example.org/sync.php".to_string(),
            read_url: "https://qr.example.org/d".to_string(),
            drop_pub: valid_pub.clone(),
            slot_id: Some("   ".to_string()),
            ..Default::default()
        };
        validate_contact_sync_descriptors(&[blank_slot_contact])
            .expect("contact descriptor with blank slot_id must pass");

        let ssrf_contact = FrontendSyncDescriptor {
            write_url: "https://10.0.0.1/sync.php".to_string(),
            read_url: "https://qr.example.org/d".to_string(),
            drop_pub: valid_pub.clone(),
            ..Default::default()
        };
        let err_ssrf_contact = validate_contact_sync_descriptors(&[ssrf_contact])
            .expect_err("contact descriptor with SSRF write_url must fail");
        assert!(
            err_ssrf_contact
                .message
                .contains("blocked internal/private address"),
            "unexpected error for SSRF write_url: {}",
            err_ssrf_contact.message
        );

        let incomplete_rep = FrontendSyncDescriptor {
            write_url: "https://sync.example.org/sync.php".to_string(),
            read_url: "https://sync.example.org/d".to_string(),
            drop_pub: valid_pub.clone(),
            replacement: Some(FrontendSyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: String::new(),
                drop_pub: valid_pub2,
            }),
            ..Default::default()
        };
        let err_incomplete = validate_contact_sync_descriptors(&[incomplete_rep])
            .expect_err("incomplete replacement must fail");
        assert!(
            err_incomplete
                .message
                .contains("Replacement sync target must specify writeUrl, readUrl, and dropPub"),
            "unexpected error: {}",
            err_incomplete.message
        );

        let bad_rep_key = FrontendSyncDescriptor {
            write_url: "https://sync.example.org/sync.php".to_string(),
            read_url: "https://sync.example.org/d".to_string(),
            drop_pub: valid_pub,
            replacement: Some(FrontendSyncReplacementTarget {
                write_url: "https://new.example.org/sync.php".to_string(),
                read_url: "https://new.example.org/d".to_string(),
                drop_pub: "zz".repeat(32),
            }),
            ..Default::default()
        };
        let err_rep_key = validate_contact_sync_descriptors(&[bad_rep_key])
            .expect_err("invalid replacement dropPub must fail");
        assert!(
            err_rep_key.message.contains("Invalid replacement dropPub"),
            "unexpected error: {}",
            err_rep_key.message
        );
    }

    #[test]
    fn test_normalize_overlap_compatibility_and_endpoint_key() {
        let valid_pub = test_ed25519_pub_hex(0x11);
        let other_pub = test_ed25519_pub_hex(0x22);

        assert_eq!(
            normalize_sync_url("  HTTPS://Sync.Example.ORG/PathCase///  "),
            "https://sync.example.org/PathCase"
        );
        assert_eq!(normalize_sync_url("  not-a-url/// "), "not-a-url");

        assert!(is_endpoint_revoked("revoked"));
        assert!(is_endpoint_revoked("  REVOKED  "));
        assert!(!is_endpoint_revoked("active"));
        assert!(!is_endpoint_revoked(""));

        assert!(endpoints_overlap(
            "https://A.example/d/",
            "",
            "HTTPS://a.example/d",
            ""
        ));
        assert!(endpoints_overlap(
            "",
            "https://a.example/sync.php/",
            "https://a.example/sync.php",
            ""
        ));
        assert!(!endpoints_overlap("", "", "", ""));

        let ep_pinned = FrontendSyncDescriptor {
            write_url: "https://Sync.Example.ORG/sync.php/".to_string(),
            read_url: "https://Sync.Example.ORG/d/".to_string(),
            drop_pub: valid_pub.clone(),
            ..Default::default()
        };
        let ep_unpinned = FrontendSyncDescriptor {
            write_url: "https://sync.example.org/sync.php".to_string(),
            ..Default::default()
        };
        let ep_match = FrontendSyncDescriptor {
            read_url: "https://sync.example.org/d".to_string(),
            drop_pub: valid_pub.to_ascii_uppercase(),
            ..Default::default()
        };
        let ep_conflict = FrontendSyncDescriptor {
            write_url: "https://sync.example.org/sync.php".to_string(),
            drop_pub: other_pub,
            ..Default::default()
        };
        let ep_disjoint = FrontendSyncDescriptor {
            write_url: "https://other.example.org/sync.php".to_string(),
            drop_pub: valid_pub,
            ..Default::default()
        };

        assert!(descriptors_overlap(&ep_pinned, &ep_unpinned));
        assert!(descriptors_compatible_for_policy(&ep_pinned, &ep_unpinned));
        assert!(descriptors_compatible_for_policy(&ep_pinned, &ep_match));
        assert!(!descriptors_compatible_for_policy(&ep_pinned, &ep_conflict));
        assert!(!descriptors_compatible_for_policy(&ep_pinned, &ep_disjoint));

        assert!(drop_pub_matches(&ep_pinned.drop_pub, &ep_match.drop_pub));
        assert!(!drop_pub_matches("", &ep_pinned.drop_pub));
        assert!(!drop_pub_matches("", ""));
        assert!(!drop_pub_matches("  ", " "));

        assert_eq!(
            sync_endpoint_key(&ep_pinned),
            "https://sync.example.org/sync.php"
        );
        assert_eq!(sync_endpoint_key(&ep_match), "https://sync.example.org/d");
    }

    #[test]
    fn test_sanitize_untrusted_sync_descriptors_filters_dedups_and_caps() {
        let valid_pub = test_ed25519_pub_hex(0x11);
        let valid_pub2 = test_ed25519_pub_hex(0x22);
        let slot1 = format!("{}_work", valid_pub2);

        let raw_drops = vec![
            humoco_market::DropDescriptor {
                write_url: "https://169.254.169.254/sync.php".to_string(),
                read_url: "https://169.254.169.254/d".to_string(),
                slot_id: None,
                drop_pub: valid_pub.clone(),
            },
            humoco_market::DropDescriptor {
                write_url: "https://query.example.org/sync.php".to_string(),
                read_url: "https://query.example.org/d?leak=1".to_string(),
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
                write_url: "https://badslot.example.org/sync.php".to_string(),
                read_url: "https://badslot.example.org/d".to_string(),
                slot_id: Some("bad slot".to_string()),
                drop_pub: valid_pub.clone(),
            },
            humoco_market::DropDescriptor {
                write_url: "  https://good.example.org/sync.php/ ".to_string(),
                read_url: " https://good.example.org/d/ ".to_string(),
                slot_id: Some(format!(" {} ", slot1)),
                drop_pub: format!(" {} ", valid_pub.to_ascii_uppercase()),
            },
            // Duplicate after host/trailing-slash normalization with same slot_id -> dropped
            humoco_market::DropDescriptor {
                write_url: "HTTPS://Good.Example.ORG/sync.php".to_string(),
                read_url: "https://good.example.org/d".to_string(),
                slot_id: Some(slot1.clone()),
                drop_pub: valid_pub.clone(),
            },
            // Same write_url with None (blank) slot_id -> distinct dedup key -> kept
            humoco_market::DropDescriptor {
                write_url: "https://good.example.org/sync.php".to_string(),
                read_url: "https://good.example.org/d".to_string(),
                slot_id: Some("   ".to_string()),
                drop_pub: valid_pub.clone(),
            },
            // Extra valid item exceeding max_len = 2 -> omitted by cap
            humoco_market::DropDescriptor {
                write_url: "https://third.example.org/sync.php".to_string(),
                read_url: "https://third.example.org/d".to_string(),
                slot_id: None,
                drop_pub: valid_pub2,
            },
        ];

        let sanitized = sanitize_untrusted_sync_descriptors(&raw_drops, 2);
        assert_eq!(sanitized.len(), 2);
        assert_eq!(sanitized[0].write_url, "https://good.example.org/sync.php/");
        assert_eq!(sanitized[0].read_url, "https://good.example.org/d/");
        assert_eq!(sanitized[0].slot_id.as_deref(), Some(slot1.as_str()));
        assert_eq!(sanitized[0].drop_pub, valid_pub);
        assert_eq!(sanitized[0].status, "active");

        assert_eq!(sanitized[1].write_url, "https://good.example.org/sync.php");
        assert_eq!(sanitized[1].slot_id, None);
    }

    #[test]
    fn test_sanitize_untrusted_frontend_sync_descriptors_revoked_dedup_and_replacement() {
        let valid_pub = test_ed25519_pub_hex(0x11);
        let valid_pub2 = test_ed25519_pub_hex(0x22);
        let slot1 = format!("{}_home", valid_pub2);

        let raw_fe = vec![
            FrontendSyncDescriptor {
                write_url: "https://revoked.example.org/sync.php".to_string(),
                read_url: "https://revoked.example.org/d".to_string(),
                drop_pub: valid_pub.clone(),
                status: " REVOKED ".to_string(),
                ..Default::default()
            },
            FrontendSyncDescriptor {
                write_url: "https://weak.example.org/sync.php".to_string(),
                read_url: "https://weak.example.org/d".to_string(),
                drop_pub: "00".repeat(32),
                status: "active".to_string(),
                ..Default::default()
            },
            FrontendSyncDescriptor {
                write_url: "https://169.254.169.254/sync.php".to_string(),
                read_url: "https://ssrf.example.org/d".to_string(),
                drop_pub: valid_pub.clone(),
                status: "active".to_string(),
                ..Default::default()
            },
            FrontendSyncDescriptor {
                write_url: "https://query-read.example.org/sync.php".to_string(),
                read_url: "https://query-read.example.org/d?leak=1".to_string(),
                drop_pub: valid_pub.clone(),
                status: "active".to_string(),
                ..Default::default()
            },
            FrontendSyncDescriptor {
                write_url: " https://ok1.example.org/sync.php ".to_string(),
                read_url: " https://ok1.example.org/d ".to_string(),
                drop_pub: format!(" {} ", valid_pub.to_ascii_uppercase()),
                slot_id: Some(format!(" {} ", slot1)),
                status: "active".to_string(),
                replacement: Some(FrontendSyncReplacementTarget {
                    write_url: " https://rep.example.org/sync.php ".to_string(),
                    read_url: " https://rep.example.org/d ".to_string(),
                    drop_pub: format!(" {} ", valid_pub2.to_ascii_uppercase()),
                }),
                ..Default::default()
            },
            // Duplicate of ok1 after URL normalization -> dropped
            FrontendSyncDescriptor {
                write_url: "HTTPS://OK1.example.org/sync.php/".to_string(),
                read_url: "https://ok1.example.org/d".to_string(),
                drop_pub: valid_pub.clone(),
                slot_id: Some(slot1.clone()),
                status: "active".to_string(),
                ..Default::default()
            },
            // Valid outer descriptor whose invalid replacement must be stripped to None
            FrontendSyncDescriptor {
                write_url: "https://ok2.example.org/sync.php".to_string(),
                read_url: "https://ok2.example.org/d".to_string(),
                drop_pub: valid_pub.clone(),
                status: "active".to_string(),
                replacement: Some(FrontendSyncReplacementTarget {
                    write_url: "https://badrep.example.org/sync.php".to_string(),
                    read_url: "https://badrep.example.org/d?q=1".to_string(),
                    drop_pub: valid_pub2.clone(),
                }),
                ..Default::default()
            },
            // Exceeds max_len = 2 -> omitted
            FrontendSyncDescriptor {
                write_url: "https://ok3.example.org/sync.php".to_string(),
                read_url: "https://ok3.example.org/d".to_string(),
                drop_pub: valid_pub2.clone(),
                status: "active".to_string(),
                ..Default::default()
            },
        ];

        let sanitized = sanitize_untrusted_frontend_sync_descriptors(&raw_fe, 2);
        assert_eq!(sanitized.len(), 2);

        assert_eq!(sanitized[0].write_url, "https://ok1.example.org/sync.php");
        assert_eq!(sanitized[0].read_url, "https://ok1.example.org/d");
        assert_eq!(sanitized[0].drop_pub, valid_pub);
        assert_eq!(sanitized[0].slot_id.as_deref(), Some(slot1.as_str()));
        let rep = sanitized[0]
            .replacement
            .as_ref()
            .expect("valid replacement must be preserved and normalized");
        assert_eq!(rep.write_url, "https://rep.example.org/sync.php");
        assert_eq!(rep.read_url, "https://rep.example.org/d");
        assert_eq!(rep.drop_pub, valid_pub2);

        assert_eq!(sanitized[1].write_url, "https://ok2.example.org/sync.php");
        assert!(
            sanitized[1].replacement.is_none(),
            "invalid replacement must be stripped while retaining outer descriptor"
        );
    }
}
