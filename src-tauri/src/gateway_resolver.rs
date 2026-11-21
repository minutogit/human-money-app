//! Sync Lock committee HTTP fan-out, connection pre-warming, and Sybil folding helpers.
//!
//! Core owns durable intent state and signature verification; this module handles bounded
//! transport fan-out and correlation folding only.

use std::collections::HashSet;
use std::time::Duration;

use human_money_core::humoco_proto::{
    causal::LockEntry,
    manifest::{AnchorManifest, ServerEntry},
    types::{LockPubKey, LockReceipt, LockReceiptKind},
    EquivocationProof,
};
use human_money_core::models::layer2_api::SyncLockBatchRequest;
use serde::Serialize;

use crate::sync_client::{self, SyncLockBatchOutcome};

/// Per-server fan-out status DTO returned to callers after outbound dispatch.
#[derive(Debug, Clone, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentDispatchObservation {
    pub local_instance_id: String,
    pub status: String,
    pub http_status: Option<u16>,
}

/// Result of dispatching a voucher's `SyncLockBatchRequest` payloads across a Sync Lock committee.
#[derive(Debug, Clone, Default)]
pub struct CommitteeDispatchResult {
    pub receipts: Vec<LockReceipt>,
    pub competing_entries: Vec<LockEntry>,
    pub http_statuses: Vec<(LockPubKey, Option<u16>)>,
    pub rejected_errors: Vec<(LockPubKey, u16, String)>,
    pub transport_errors: Vec<(LockPubKey, String)>,
    pub pending_retries: Vec<(ServerEntry, SyncLockBatchRequest)>,
    pub epoch_frozen_manifests: Vec<AnchorManifest>,
    pub had_epoch_frozen: bool,
    pub revoked_notices: Vec<(LockPubKey, sync_client::SyncRevocationNotice)>,
    pub equivocation_proofs: Vec<EquivocationProof>,
}

/// Candidate lock server metadata considered for Sybil folding (`assemble_anchor_template` & Bürgen-Check).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SybilServerCandidate {
    pub write_url: String,
    pub lock_pub: LockPubKey,
    pub admin_pub: Option<[u8; 32]>,
    pub asn: Option<u32>,
}

/// Extracts a normalized registrable domain key for Sybil folding (IP literals stay distinct by host,
/// loopback addresses retain their port in dev/test mode, DNS hosts collapse to their last 2 labels).
pub fn extract_registrable_domain(raw_url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(raw_url.trim()).ok()?;
    let host = parsed
        .host_str()?
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if ip.is_loopback() {
            return Some(format!(
                "{}:{}",
                host,
                parsed.port_or_known_default().unwrap_or(0)
            ));
        }
        return Some(host);
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Some(format!(
            "{}:{}",
            host,
            parsed.port_or_known_default().unwrap_or(0)
        ));
    }
    let parts: Vec<&str> = host.split('.').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 3 {
        let last_two = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
        const TWO_LEVEL_CCTLDS: &[&str] = &[
            "co.uk", "org.uk", "ac.uk", "com.au", "net.au", "co.jp", "ne.jp", "co.nz", "com.br",
            "co.za", "gv.at", "co.at", "or.at", "com.cn", "co.in", "com.mx",
        ];
        if TWO_LEVEL_CCTLDS.contains(&last_two.as_str()) {
            return Some(format!("{}.{}", parts[parts.len() - 3], last_two));
        }
    }
    if parts.len() >= 2 {
        Some(format!(
            "{}.{}",
            parts[parts.len() - 2],
            parts[parts.len() - 1]
        ))
    } else {
        Some(host)
    }
}

/// Folds committee server candidates sharing the same `lock_pub`, `admin_pub`, registrable domain,
/// or optional AS number (`asn`) so correlated servers count at most once against quorum safety.
pub fn sybil_fold_committee(candidates: &[SybilServerCandidate]) -> Vec<SybilServerCandidate> {
    let mut seen_lock_pubs: HashSet<LockPubKey> = HashSet::new();
    let mut seen_admin_pubs: HashSet<[u8; 32]> = HashSet::new();
    let mut seen_domains: HashSet<String> = HashSet::new();
    let mut seen_asns: HashSet<u32> = HashSet::new();
    let mut folded = Vec::new();

    for cand in candidates {
        if seen_lock_pubs.contains(&cand.lock_pub) {
            continue;
        }
        if let Some(admin) = cand.admin_pub {
            if seen_admin_pubs.contains(&admin) {
                continue;
            }
        }
        let Some(domain_key) = extract_registrable_domain(&cand.write_url) else {
            continue;
        };
        if seen_domains.contains(&domain_key) {
            continue;
        }
        if let Some(asn) = cand.asn {
            if seen_asns.contains(&asn) {
                continue;
            }
        }

        seen_lock_pubs.insert(cand.lock_pub);
        if let Some(admin) = cand.admin_pub {
            seen_admin_pubs.insert(admin);
        }
        seen_domains.insert(domain_key);
        if let Some(asn) = cand.asn {
            seen_asns.insert(asn);
        }
        folded.push(cand.clone());
    }

    folded
}

/// Warms DNS/TCP/TLS connections and checks `?info` readiness across Sync Lock URLs.
pub async fn prewarm_lock_urls(client: &reqwest::Client, write_urls: &[String]) {
    let unique: Vec<String> = write_urls
        .iter()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    if unique.is_empty() {
        return;
    }

    let mut join_set = tokio::task::JoinSet::new();
    for url in unique {
        let c = client.clone();
        join_set.spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                sync_client::fetch_sync_info(&c, &url),
            )
            .await;
        });
    }
    while join_set.join_next().await.is_some() {}
}

/// Fans out pre-built per-server `SyncLockBatchRequest` payloads in parallel across a committee
/// within a bounded per-server timeout.
///
/// Collects all verified HTTP responses within `per_server_timeout`, keeps any competing 605-byte
/// `LockEntry` returned with `ConflictSeen`, and lists timed-out / failed servers in `pending_retries`
/// so callers can fire background catch-up completion without stalling healthy checkouts.
pub async fn dispatch_lock_committee_batches(
    client: &reqwest::Client,
    requests: &[(ServerEntry, SyncLockBatchRequest)],
    per_server_timeout: Duration,
    probe_cache: &std::collections::BTreeMap<String, sync_client::FrontendLockServerProbeResult>,
) -> CommitteeDispatchResult {
    let mut join_set = tokio::task::JoinSet::new();
    for (server, req) in requests {
        let c = client.clone();
        let srv = server.clone();
        let (target_write_url, _) =
            crate::services::common::resolve_server_locator(&srv, probe_cache);
        let payload = req.clone();
        join_set.spawn(async move {
            let outcome = tokio::time::timeout(
                per_server_timeout,
                sync_client::post_lock_batch(&c, &target_write_url, &payload),
            )
            .await;
            (srv, payload, outcome)
        });
    }

    let mut results = Vec::with_capacity(requests.len());
    while let Some(res) = join_set.join_next().await {
        if let Ok(item) = res {
            results.push(item);
        }
    }
    let mut out = CommitteeDispatchResult::default();

    for (server, req, wrapped) in results {
        match wrapped {
            Ok(Ok(SyncLockBatchOutcome::Response { status_code, body })) => {
                out.http_statuses.push((server.lock_pub, Some(status_code)));
                let mut expected_entries: Vec<([u8; 32], [u8; 32], [u8; 32])> =
                    Vec::with_capacity(req.entries.len());
                let mut requested_slots: HashSet<[u8; 32]> =
                    HashSet::with_capacity(req.entries.len());
                for hex_str in &req.entries {
                    if let Ok(wire) = human_money_core::humoco_proto::decode_hex_fixed::<
                        { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                    >(hex_str)
                    {
                        if let Ok(entry) = LockEntry::from_wire_bytes(&wire) {
                            if let Ok(eh) = entry.entry_hash() {
                                requested_slots.insert(entry.slot);
                                expected_entries.push((entry.slot, entry.child_lock, eh));
                            }
                        }
                    }
                }

                let max_receipts = req.entries.len().saturating_mul(2).clamp(2, 64);
                let mut seen_server_receipts: HashSet<([u8; 32], [u8; 32], [u8; 32])> =
                    HashSet::new();
                let mut covered_requested_slots: HashSet<[u8; 32]> = HashSet::new();
                let mut valid_first_writes_for_equiv: Vec<LockReceipt> = Vec::new();
                for receipt in body.receipts.into_iter().take(max_receipts) {
                    // ConflictSeen is accepted EXCLUSIVELY via the verified `body.conflict` branch.
                    if !matches!(receipt.kind, LockReceiptKind::FirstWrite) {
                        continue;
                    }
                    if receipt.lock_pub != server.lock_pub
                        || receipt.bucket != req.bucket
                        || !requested_slots.contains(&receipt.slot)
                        || !receipt.verify_signature()
                    {
                        continue;
                    }
                    valid_first_writes_for_equiv.push(receipt.clone());
                    let matches_requested = expected_entries.iter().any(|&(s, c, eh)| {
                        s == receipt.slot && c == receipt.child_lock && eh == receipt.entry_hash
                    });
                    if receipt.recipient_nonce == req.recipient_nonce
                        && matches_requested
                        && seen_server_receipts.insert((
                            receipt.slot,
                            receipt.child_lock,
                            receipt.entry_hash,
                        ))
                    {
                        covered_requested_slots.insert(receipt.slot);
                        out.receipts.push(receipt);
                    }
                }
                if let Some(proof) = human_money_core::services::l2_gateway::detect_equivocation(
                    &valid_first_writes_for_equiv,
                ) {
                    out.equivocation_proofs.push(proof);
                }

                let mut had_verified_conflict = false;
                if let Some(conflict) = body.conflict {
                    let valid_receipt = conflict.receipt.verify_signature()
                        && conflict.receipt.lock_pub == server.lock_pub
                        && conflict.receipt.bucket == req.bucket
                        && conflict.receipt.slot == conflict.slot
                        && requested_slots.contains(&conflict.slot)
                        && matches!(conflict.receipt.kind, LockReceiptKind::ConflictSeen)
                        && conflict.receipt.recipient_nonce == req.recipient_nonce;
                    if valid_receipt {
                        if let Ok(entry) = human_money_core::humoco_proto::decode_hex_fixed::<
                            { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                        >(&conflict.competing_entry)
                        .map_err(|e| e.to_string())
                        .and_then(|bytes| {
                            LockEntry::from_wire_bytes(&bytes).map_err(|e| e.to_string())
                        }) {
                            let entry_matches_receipt = entry.slot == conflict.receipt.slot
                                && entry.child_lock == conflict.receipt.child_lock
                                && entry.entry_hash().ok() == Some(conflict.receipt.entry_hash);
                            if entry_matches_receipt {
                                out.receipts.push(conflict.receipt);
                                out.competing_entries.push(entry);
                                // No need to retry a slot already settled by verified ConflictSeen
                                had_verified_conflict = true;
                            }
                        }
                    }
                }
                let had_server_receipt = had_verified_conflict
                    || (!requested_slots.is_empty()
                        && covered_requested_slots.len() == requested_slots.len());
                if !had_server_receipt {
                    out.pending_retries.push((server, req));
                }
            }
            Ok(Ok(SyncLockBatchOutcome::EpochFrozen {
                status_code,
                error,
                latest_manifest,
            })) => {
                out.http_statuses.push((server.lock_pub, Some(status_code)));
                out.had_epoch_frozen = true;
                if let Some(m) = latest_manifest {
                    out.epoch_frozen_manifests.push(*m);
                }
                out.rejected_errors
                    .push((server.lock_pub, status_code, error));
            }
            Ok(Ok(SyncLockBatchOutcome::Revoked {
                status_code,
                notice,
            })) => {
                out.http_statuses.push((server.lock_pub, Some(status_code)));
                if let Some(n) = notice {
                    out.revoked_notices.push((server.lock_pub, *n));
                }
                out.rejected_errors
                    .push((server.lock_pub, status_code, "revoked".to_string()));
            }
            Ok(Ok(SyncLockBatchOutcome::Rejected { status_code, error })) => {
                out.http_statuses.push((server.lock_pub, Some(status_code)));
                out.rejected_errors
                    .push((server.lock_pub, status_code, error));
            }
            Ok(Err(err)) => {
                out.http_statuses.push((server.lock_pub, None));
                out.transport_errors.push((server.lock_pub, err));
                out.pending_retries.push((server, req));
            }
            Err(_) => {
                out.http_statuses.push((server.lock_pub, None));
                out.transport_errors
                    .push((server.lock_pub, "Lock batch request timed out".to_string()));
                out.pending_retries.push((server, req));
            }
        }
    }

    out
}

/// Converts committee dispatch status into frontend-facing `IntentDispatchObservation` records.
pub fn summarize_intent_dispatch(
    local_instance_id: &str,
    res: &CommitteeDispatchResult,
) -> IntentDispatchObservation {
    let any_ok = res
        .http_statuses
        .iter()
        .any(|(_, s)| matches!(s, Some(200 | 201)));
    let any_conflict = !res.competing_entries.is_empty()
        || res
            .receipts
            .iter()
            .any(|r| matches!(r.kind, LockReceiptKind::ConflictSeen));
    let first_http = res.http_statuses.iter().find_map(|(_, s)| *s);

    let status = if any_conflict {
        "conflicted".to_string()
    } else if any_ok && res.pending_retries.is_empty() && res.rejected_errors.is_empty() {
        "confirmed".to_string()
    } else {
        "pending".to_string()
    };

    IntentDispatchObservation {
        local_instance_id: local_instance_id.to_string(),
        status,
        http_status: first_http,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use human_money_core::humoco_proto::manifest::NominatedBy;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_sybil_fold_committee_dedupes_domains_admins_and_asns() {
        let a1 = SybilServerCandidate {
            write_url: "https://s1.evil-host.example/sync.php".into(),
            lock_pub: [1u8; 32],
            admin_pub: Some([10u8; 32]),
            asn: Some(64500),
        };
        // Different lock_pub, different admin_pub, same registrable domain -> folded!
        let a2 = SybilServerCandidate {
            write_url: "https://s2.evil-host.example/sync.php".into(),
            lock_pub: [2u8; 32],
            admin_pub: Some([11u8; 32]),
            asn: Some(64501),
        };
        // Different domain, same admin_pub -> folded!
        let a3 = SybilServerCandidate {
            write_url: "https://unrelated.org/sync.php".into(),
            lock_pub: [3u8; 32],
            admin_pub: Some([10u8; 32]),
            asn: Some(64502),
        };
        // Different domain, different admin_pub, same ASN -> folded!
        let a4 = SybilServerCandidate {
            write_url: "https://another.org/sync.php".into(),
            lock_pub: [4u8; 32],
            admin_pub: Some([12u8; 32]),
            asn: Some(64500),
        };
        // Truly independent operator -> kept!
        let b1 = SybilServerCandidate {
            write_url: "https://honest.coop/sync.php".into(),
            lock_pub: [5u8; 32],
            admin_pub: Some([13u8; 32]),
            asn: Some(64599),
        };

        let folded = sybil_fold_committee(&[a1, a2, a3, a4, b1]);
        assert_eq!(folded.len(), 2);
        assert_eq!(folded[0].lock_pub, [1u8; 32]);
        assert_eq!(folded[1].lock_pub, [5u8; 32]);
    }

    #[tokio::test]
    async fn test_dispatch_lock_committee_batches_handles_partial_failure() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        use ed25519_dalek::Signer;
        let sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let lock_pub = sk.verifying_key().to_bytes();

        let anchor_id = [1u8; 32];
        let bucket = [2u8; 32];
        let slot = [3u8; 32];
        let child_lock = [4u8; 32];
        let nonce = [6u8; 32];
        let req_entry = LockEntry {
            slot,
            parent_slot: [0u8; 32],
            output_branch: 0,
            depth: 0,
            child_lock,
            sender_pubkey: [11u8; 32],
            receiver_pubkey_hash: [12u8; 32],
            change_pubkey_hash: None,
            causal_sig: [13u8; 64],
            root_mig_sig: Some([14u8; 64]),
            encrypted_payload: [0u8;
                human_money_core::humoco_proto::causal::HOP_ENCRYPTED_PAYLOAD_LEN],
            pow_nonce: 0,
        };
        let req_entry_hex = req_entry.to_wire_hex().unwrap();
        let entry_hash = req_entry.entry_hash().unwrap();
        let mut receipt = LockReceipt {
            kind: LockReceiptKind::FirstWrite,
            lock_pub,
            bucket,
            slot,
            child_lock,
            entry_hash,
            retain_until_quarter: 8159,
            write_seq: 1,
            bucket_head: [9u8; 32],
            recipient_nonce: nonce,
            sig: [0u8; 64],
        };
        let digest = receipt.digest();
        receipt.sig = sk.sign(&digest).to_bytes();
        let resp_body = serde_json::json!({
            "status": "ok",
            "bucket": hex::encode(bucket),
            "retain_until_quarter": 8159,
            "receipts": [receipt],
            "conflict": null,
        })
        .to_string();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).await.unwrap();
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            stream.write_all(http.as_bytes()).await.unwrap();
        });

        let dead_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_port = dead_listener.local_addr().unwrap().port();
        drop(dead_listener);

        let srv1 = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", addr.port()),
            read_url: format!("http://127.0.0.1:{}/d", addr.port()),
            admin_pub: [8u8; 32],
            lock_pub,
            nominated_by: NominatedBy::Creator,
        };
        let srv2 = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", dead_port),
            read_url: format!("http://127.0.0.1:{}/d", dead_port),
            admin_pub: [9u8; 32],
            lock_pub: [10u8; 32],
            nominated_by: NominatedBy::Creator,
        };

        let req1 = SyncLockBatchRequest {
            anchor_id,
            bucket,
            recipient_nonce: nonce,
            entries: vec![req_entry_hex],
            manifest: None,
            genesis_ticket: None,
            known_head_receipt: None,
        };
        let req2 = req1.clone();

        let client = sync_client::create_sync_http_client();
        let empty_cache = std::collections::BTreeMap::new();
        let res = dispatch_lock_committee_batches(
            &client,
            &[(srv1, req1), (srv2, req2)],
            Duration::from_secs(2),
            &empty_cache,
        )
        .await;

        assert_eq!(res.receipts.len(), 1);
        assert_eq!(res.pending_retries.len(), 1);
        assert_eq!(res.transport_errors.len(), 1);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn k1_dispatch_rejects_forged_conflict_receipt_wrong_nonce_and_hash_mismatch() {
        use ed25519_dalek::Signer;
        use human_money_core::humoco_proto::causal::{LockEntry, HOP_ENCRYPTED_PAYLOAD_LEN};

        let sk = ed25519_dalek::SigningKey::from_bytes(&[17u8; 32]);
        let lock_pub = sk.verifying_key().to_bytes();
        let anchor_id = [1u8; 32];
        let bucket = [2u8; 32];
        let slot = [3u8; 32];
        let child_lock = [4u8; 32];
        let nonce = [6u8; 32];

        let fake_entry = LockEntry {
            slot,
            parent_slot: [0u8; 32],
            output_branch: 0,
            depth: 0,
            child_lock,
            sender_pubkey: [11u8; 32],
            receiver_pubkey_hash: [12u8; 32],
            change_pubkey_hash: None,
            causal_sig: [13u8; 64],
            root_mig_sig: Some([14u8; 64]),
            encrypted_payload: [0u8; HOP_ENCRYPTED_PAYLOAD_LEN],
            pow_nonce: 0,
        };
        let entry_hex = fake_entry.to_wire_hex().unwrap();
        let real_hash = fake_entry.entry_hash().unwrap();

        let make_conflict_receipt = |recipient_nonce: [u8; 32], entry_hash: [u8; 32]| {
            let mut r = LockReceipt {
                kind: LockReceiptKind::ConflictSeen,
                lock_pub,
                bucket,
                slot,
                child_lock,
                entry_hash,
                retain_until_quarter: 8159,
                write_seq: 1,
                bucket_head: [9u8; 32],
                recipient_nonce,
                sig: [0u8; 64],
            };
            r.sig = sk.sign(&r.digest()).to_bytes();
            r
        };

        // Case 1: valid signature + hash, WRONG recipient_nonce -> must ignore conflict & schedule retry
        // Case 2: valid signature + nonce, WRONG entry_hash -> must ignore conflict & schedule retry
        for bad_receipt in [
            make_conflict_receipt([0xFF; 32], real_hash),
            make_conflict_receipt(nonce, [0xEE; 32]),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let resp_body = serde_json::json!({
                "status": "conflict",
                "bucket": hex::encode(bucket),
                "retain_until_quarter": 8159,
                "receipts": [],
                "conflict": {
                    "entry_index": 0,
                    "slot": hex::encode(slot),
                    "receipt": bad_receipt,
                    "competing_entry": entry_hex,
                }
            })
            .to_string();

            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await.unwrap();
                let http = format!(
                    "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    resp_body.len(),
                    resp_body
                );
                stream.write_all(http.as_bytes()).await.unwrap();
            });

            let srv = ServerEntry {
                write_url: format!("http://127.0.0.1:{}/sync.php", addr.port()),
                read_url: format!("http://127.0.0.1:{}/d", addr.port()),
                admin_pub: [8u8; 32],
                lock_pub,
                nominated_by: NominatedBy::Creator,
            };
            let req = SyncLockBatchRequest {
                anchor_id,
                bucket,
                recipient_nonce: nonce,
                entries: vec![entry_hex.clone()],
                manifest: None,
                genesis_ticket: None,
                known_head_receipt: None,
            };

            let client = sync_client::create_sync_http_client();
            let empty_cache = std::collections::BTreeMap::new();
            let res = dispatch_lock_committee_batches(
                &client,
                &[(srv, req)],
                Duration::from_secs(2),
                &empty_cache,
            )
            .await;
            assert!(
                res.competing_entries.is_empty(),
                "Forged/mismatched ConflictSeen receipt must NOT yield competing_entry"
            );
            assert!(
                res.receipts.is_empty(),
                "Rejected ConflictSeen receipt must not enter receipts"
            );
            assert_eq!(
                res.pending_retries.len(),
                1,
                "Rejected ConflictSeen must NOT satisfy had_server_receipt"
            );
            server.await.unwrap();
        }
    }

    #[test]
    fn test_s04_f4_sybil_fold_skips_candidates_without_registrable_domain() {
        let c_empty = SybilServerCandidate {
            write_url: String::new(),
            lock_pub: [1u8; 32],
            admin_pub: Some([11u8; 32]),
            asn: None,
        };
        let c_invalid = SybilServerCandidate {
            write_url: "not-a-valid-url".to_string(),
            lock_pub: [2u8; 32],
            admin_pub: Some([12u8; 32]),
            asn: None,
        };
        let c_valid = SybilServerCandidate {
            write_url: "https://honest.example/sync.php".to_string(),
            lock_pub: [3u8; 32],
            admin_pub: Some([13u8; 32]),
            asn: None,
        };
        let folded = sybil_fold_committee(&[c_empty, c_invalid, c_valid]);
        assert_eq!(
            folded.len(),
            1,
            "S04-F4: candidates without a registrable domain must be excluded"
        );
        assert_eq!(folded[0].lock_pub, [3u8; 32]);
    }

    #[tokio::test]
    async fn test_s04_f1_dispatch_rejects_unmatched_first_write_and_conflict_in_receipts_array() {
        use ed25519_dalek::Signer;
        use human_money_core::humoco_proto::causal::{LockEntry, HOP_ENCRYPTED_PAYLOAD_LEN};

        let sk = ed25519_dalek::SigningKey::from_bytes(&[27u8; 32]);
        let lock_pub = sk.verifying_key().to_bytes();
        let anchor_id = [1u8; 32];
        let bucket = [2u8; 32];
        let slot = [3u8; 32];
        let child_lock = [4u8; 32];
        let nonce = [6u8; 32];

        let req_entry = LockEntry {
            slot,
            parent_slot: [0u8; 32],
            output_branch: 0,
            depth: 0,
            child_lock,
            sender_pubkey: [11u8; 32],
            receiver_pubkey_hash: [12u8; 32],
            change_pubkey_hash: None,
            causal_sig: [13u8; 64],
            root_mig_sig: Some([14u8; 64]),
            encrypted_payload: [0u8; HOP_ENCRYPTED_PAYLOAD_LEN],
            pow_nonce: 0,
        };
        let req_entry_hex = req_entry.to_wire_hex().unwrap();
        let req_entry_hash = req_entry.entry_hash().unwrap();

        let sign = |mut r: LockReceipt| {
            r.sig = sk.sign(&r.digest()).to_bytes();
            r
        };

        // Case 1: ConflictSeen smuggled inside body.receipts (with conflict: null)
        let smuggled_conflict = sign(LockReceipt {
            kind: LockReceiptKind::ConflictSeen,
            lock_pub,
            bucket,
            slot,
            child_lock: [0xEE; 32],
            entry_hash: [0xDD; 32],
            retain_until_quarter: 8159,
            write_seq: 1,
            bucket_head: [9u8; 32],
            recipient_nonce: nonce,
            sig: [0u8; 64],
        });
        // Case 2: FirstWrite with wrong bucket
        let wrong_bucket_fw = sign(LockReceipt {
            kind: LockReceiptKind::FirstWrite,
            lock_pub,
            bucket: [0xFF; 32],
            slot,
            child_lock,
            entry_hash: req_entry_hash,
            retain_until_quarter: 8159,
            write_seq: 1,
            bucket_head: [9u8; 32],
            recipient_nonce: nonce,
            sig: [0u8; 64],
        });
        // Case 3: FirstWrite with wrong recipient_nonce
        let wrong_nonce_fw = sign(LockReceipt {
            kind: LockReceiptKind::FirstWrite,
            lock_pub,
            bucket,
            slot,
            child_lock,
            entry_hash: req_entry_hash,
            retain_until_quarter: 8159,
            write_seq: 1,
            bucket_head: [9u8; 32],
            recipient_nonce: [0xAA; 32],
            sig: [0u8; 64],
        });
        // Case 4: FirstWrite with wrong slot / child_lock
        let wrong_entry_fw = sign(LockReceipt {
            kind: LockReceiptKind::FirstWrite,
            lock_pub,
            bucket,
            slot: [0xBB; 32],
            child_lock,
            entry_hash: req_entry_hash,
            retain_until_quarter: 8159,
            write_seq: 1,
            bucket_head: [9u8; 32],
            recipient_nonce: nonce,
            sig: [0u8; 64],
        });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let resp_body = serde_json::json!({
            "status": "ok",
            "bucket": hex::encode(bucket),
            "retain_until_quarter": 8159,
            "receipts": [smuggled_conflict, wrong_bucket_fw, wrong_nonce_fw, wrong_entry_fw],
            "conflict": null,
        })
        .to_string();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf).await.unwrap();
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            stream.write_all(http.as_bytes()).await.unwrap();
        });

        let srv = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", addr.port()),
            read_url: format!("http://127.0.0.1:{}/d", addr.port()),
            admin_pub: [8u8; 32],
            lock_pub,
            nominated_by: NominatedBy::Creator,
        };
        let req = SyncLockBatchRequest {
            anchor_id,
            bucket,
            recipient_nonce: nonce,
            entries: vec![req_entry_hex],
            manifest: None,
            genesis_ticket: None,
            known_head_receipt: None,
        };

        let client = sync_client::create_sync_http_client();
        let empty_cache = std::collections::BTreeMap::new();
        let res = dispatch_lock_committee_batches(
            &client,
            &[(srv, req)],
            Duration::from_secs(2),
            &empty_cache,
        )
        .await;
        assert!(
            res.receipts.is_empty(),
            "S04-F1: smuggled ConflictSeen & mismatched FirstWrite receipts must all be dropped, got {:?}",
            res.receipts
        );
        assert_eq!(
            res.pending_retries.len(),
            1,
            "S04-F1: unmatched receipts must NOT satisfy had_server_receipt"
        );
        server.await.unwrap();
    }
}
