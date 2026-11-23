const MAX_ERROR_BODY_BYTES: usize = 4 * 1024;
const MAX_ERROR_DETAIL_CHARS: usize = 256;
const MAX_JSON_META_BODY_BYTES: usize = 256 * 1024;
const MAX_LOCK_BATCH_BODY_BYTES: usize = 2 * 1024 * 1024;
const MAX_INDEX_JSON_BODY_BYTES: usize = 4 * 1024 * 1024;
const MAX_FREEZE_JSON_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_SLOT_BLOB_BODY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_MEDIA_BLOB_BODY_BYTES: usize = humoco_market::MAX_AD_MEDIA_BYTES + 1024;
const MIN_RETRY_AFTER_SECS: u64 = 1;
const MAX_RETRY_AFTER_SECS: u64 = 3600;
const POW_START_NONCE_MASK: u64 = (1u64 << 52) - 1;

mod http;
mod lock_api;
mod revocation;
mod slots;
mod ssrf;
mod types;

#[cfg(test)]
mod tests;

pub use http::{current_unix_timestamp, extract_etag, generate_sync_nonce, parse_retry_after};
pub use lock_api::{
    anchor_manifest_candidate_urls, fetch_anchor_manifest_bin, fetch_static_lock_entry,
    generate_lock_probe_nonce, get_lock_freeze_status, normalize_lock_read_base, post_lock_batch,
    post_lock_commit_finalize, post_lock_commit_sign, post_lock_freeze_action, post_lock_import,
    probe_lock_server, request_genesis_ticket, verify_lock_delegation, verify_lock_probe_response,
    verify_lock_selftest,
};
pub use revocation::{
    canonical_revocation_message, fetch_revocation_notice, push_lock_delegation,
    push_revocation_notice, sign_revocation_notice, verify_revocation_notice,
};
pub use slots::{
    delete_account, delete_media_blob, delete_slot, fetch_admin_status, fetch_sync_info,
    fetch_text_bounded, get_media_blob, get_slot, get_slot_bounded, head_slot_capability,
    list_index, push_members, put_admin_config, put_media_blob, put_slot,
};
pub use ssrf::{
    create_sync_http_client, create_sync_probe_client, sync_origin_key, validate_sync_read_url,
    validate_sync_url, validate_sync_url_strict,
};
pub use types::{
    FrontendLockServerProbeResult, GenesisTicketResponse, IndexFetchResult, LockFreezeStatusDto,
    LockTicketAuth, PutSlotOutcome, SlotAckReceipt, SlotFetchResult, SyncCapabilityResult,
    SyncClientError, SyncIndexEntry, SyncLockBatchOutcome, SyncLockDelegationDto, SyncLockInfoDto,
    SyncLockProbeResponseDto, SyncLockSelftestChecksDto, SyncLockSelftestReportDto,
    SyncReplacementTarget, SyncRevocationNotice, SyncServerAdminStatus, SyncServerInfo,
};
