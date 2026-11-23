use super::http::normalize_etag_token;
use super::revocation::has_outer_ws_or_control;
use super::*;
use ed25519_dalek::SigningKey;
use human_money_core::humoco_proto::{
    crypto::verify_ed25519_strict, decode_hex_fixed, AnchorManifest, GenesisTicket,
    MigrationProposal, SnapshotExport,
};
use human_money_core::models::layer2_api::SyncLockBatchResponse;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncClientError {
    LocalValidation(String),
    Transport(String),
    RateLimited {
        status: u16,
        retry_after_secs: u64,
        context: String,
        snippet: String,
    },
    HttpStatus {
        status: u16,
        retry_after_secs: Option<u64>,
        context: String,
        snippet: String,
    },
    ResponseTooLarge {
        max_bytes: usize,
        context: String,
    },
    InvalidPayload(String),
}

impl SyncClientError {
    pub fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::RateLimited {
                retry_after_secs, ..
            } => Some((*retry_after_secs).clamp(MIN_RETRY_AFTER_SECS, MAX_RETRY_AFTER_SECS)),
            Self::HttpStatus {
                status,
                retry_after_secs,
                ..
            } => retry_after_secs
                .or((*status == 429).then_some(60))
                .map(|s| s.clamp(MIN_RETRY_AFTER_SECS, MAX_RETRY_AFTER_SECS)),
            _ => None,
        }
    }

    pub fn http_status(&self) -> Option<u16> {
        match self {
            Self::RateLimited { status, .. } | Self::HttpStatus { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn is_local_validation(&self) -> bool {
        matches!(self, Self::LocalValidation(_))
    }

    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimited { .. })
            || matches!(self, Self::HttpStatus { status: 429, .. })
    }
}

impl std::fmt::Display for SyncClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LocalValidation(msg) | Self::Transport(msg) | Self::InvalidPayload(msg) => {
                f.write_str(msg)
            }
            Self::RateLimited {
                status,
                retry_after_secs,
                context,
                snippet,
            } => {
                let clamped = (*retry_after_secs).clamp(MIN_RETRY_AFTER_SECS, MAX_RETRY_AFTER_SECS);
                if context.is_empty() {
                    if *status == 429 {
                        write!(f, "Rate limited (HTTP 429). Retry-After: {}s", clamped)?;
                    } else {
                        write!(
                            f,
                            "Rate limited (HTTP {}, HTTP 429). Retry-After: {}s",
                            status, clamped
                        )?;
                    }
                } else if *status == 429 {
                    write!(
                        f,
                        "Rate limited {} (HTTP 429). Retry-After: {}s",
                        context, clamped
                    )?;
                } else {
                    write!(
                        f,
                        "Rate limited {} (HTTP {}, HTTP 429). Retry-After: {}s",
                        context, status, clamped
                    )?;
                }
                if !snippet.is_empty() {
                    write!(f, ": {}", snippet)?;
                }
                Ok(())
            }
            Self::HttpStatus {
                status,
                context,
                snippet,
                ..
            } => {
                if *status == 413 && context.starts_with("Payload too large:") {
                    write!(f, "{} (HTTP 413)", context)?;
                } else if *status == 507 && context.starts_with("Insufficient storage:") {
                    write!(f, "{} (HTTP 507)", context)?;
                } else if context.is_empty() {
                    write!(f, "Server returned HTTP {}", status)?;
                } else {
                    write!(f, "Server returned HTTP {} {}", status, context)?;
                }
                if !snippet.is_empty() {
                    write!(f, ": {}", snippet)?;
                }
                Ok(())
            }
            Self::ResponseTooLarge { max_bytes, context } => {
                if context.is_empty() {
                    write!(
                        f,
                        "Response body exceeds maximum allowed size ({} bytes)",
                        max_bytes
                    )
                } else {
                    write!(
                        f,
                        "Response body {} exceeds maximum allowed size ({} bytes)",
                        context, max_bytes
                    )
                }
            }
        }
    }
}

impl std::error::Error for SyncClientError {}

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
    pub write_url: String,
    pub read_url: String,
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
    pub ticket_pow_bits: u32,
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
    pub ticket_pow_bits: u32,
    pub inode_warning: bool,
    pub used_inodes: u64,
    pub max_inodes: u64,
    pub probed_at_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<SyncLockDelegationDto>,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SlotAckReceipt {
    pub lock_pub: String,
    pub pubkey: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub channel: String,
    pub etag: String,
    pub timestamp: u64,
    pub sig: String,
}

impl SlotAckReceipt {
    pub fn verify(
        &self,
        expected_lock_pub: &str,
        expected_pubkey: &str,
        expected_prefix: Option<&str>,
        expected_channel: &str,
        expected_etag: &str,
    ) -> bool {
        if self.timestamp == 0 || has_outer_ws_or_control(&self.lock_pub) {
            return false;
        }
        let norm_expected_pfx = expected_prefix.filter(|s| !s.is_empty());
        let norm_self_pfx = self.prefix.as_deref().filter(|s| !s.is_empty());
        let Some(clean_expected_etag) = normalize_etag_token(expected_etag) else {
            return false;
        };
        let Some(clean_self_etag) = normalize_etag_token(&self.etag) else {
            return false;
        };
        if !self.lock_pub.eq_ignore_ascii_case(expected_lock_pub.trim())
            || self.pubkey != expected_pubkey
            || norm_self_pfx != norm_expected_pfx
            || self.channel != expected_channel
            || clean_self_etag != clean_expected_etag
        {
            return false;
        }
        let Ok(pk_bytes) = decode_hex_fixed::<32>(&self.lock_pub) else {
            return false;
        };
        let Ok(sig_bytes) = decode_hex_fixed::<64>(&self.sig) else {
            return false;
        };
        let msg = format!(
            "HUMOCO-SLOT-ACK-V1\n{}\n{}\n{}\n{}\n{}\n{}",
            self.lock_pub,
            self.pubkey,
            norm_self_pfx.unwrap_or(""),
            self.channel,
            self.etag,
            self.timestamp
        );
        verify_ed25519_strict(&pk_bytes, msg.as_bytes(), &sig_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutSlotOutcome {
    Saved {
        etag: Option<String>,
        slot_ack: Option<SlotAckReceipt>,
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
    pub max_members: usize,
    pub max_user_bytes: u64,
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
    EpochFrozen {
        status_code: u16,
        error: String,
        latest_manifest: Option<Box<AnchorManifest>>,
    },
    Revoked {
        status_code: u16,
        notice: Option<Box<SyncRevocationNotice>>,
    },
    Rejected {
        status_code: u16,
        error: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LockFreezeStatusDto {
    pub anchor_id: String,
    pub bucket: String,
    pub active_manifest: Option<AnchorManifest>,
    pub last_creator_activity_secs: Option<u64>,
    pub bucket_freeze_state: serde_json::Value,
    pub voted_proposal: Option<MigrationProposal>,
    pub snapshot_export: Option<SnapshotExport>,
}

#[derive(Clone, Debug)]
pub enum LockTicketAuth {
    Direct(SigningKey),
    Stealth {
        wallet_ed_sk: SigningKey,
        drop_pub_hex: String,
    },
    Pow {
        pow_bits: u32,
    },
}
