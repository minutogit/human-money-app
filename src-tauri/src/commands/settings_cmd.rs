use crate::{models::{FrontendUserProfile, SealUploadData, FrontendError, FrontendRollbackReconciliationDto, FrontendVoucherL2StatusDto}, AppState, settings::AppSettings};
use log::{info, error};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const MAINNET_SEED_FALLBACK: &str = "https://raw.githubusercontent.com/minutogit/seeds/main/mainnet.json";
const TESTNET_SEED_FALLBACK: &str = "https://raw.githubusercontent.com/minutogit/seeds/main/testnet.json";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct L2BenchmarkResult {
    pub url: String,
    pub latency_ms: Option<u64>,
    pub reachable: bool,
}

#[tauri::command]
pub fn get_app_settings(state: tauri::State<AppState>) -> Result<AppSettings, FrontendError> {
    info!("Getting app settings from cache...");
    state.get_cached_settings().map_err(FrontendError::from)
}

#[tauri::command]
pub async fn save_app_settings(
    settings: AppSettings,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), FrontendError> {
    info!("Saving app settings to disk...");
    {
        let mut service = state.service.lock().unwrap();
        if let Some(policy)=crate::checkout::configured_policy(&settings)? {
            service.configure_regional_mesh_authority(policy,password.as_deref()).map_err(FrontendError::from)?;
        }
        *state.regional_views.lock().map_err(|_|FrontendError::from("Regional cache unavailable".to_string()))?=None;
        state.save_settings(&mut service, settings, password.as_deref()).map_err(FrontendError::from)?;
        drop(service);
    }
    if let Err(error)=crate::checkout::refresh_views(state.inner()).await {log::debug!("Proactive regional view refresh failed: {}",error.message);}
    Ok(())
}

#[tauri::command]
pub fn update_user_profile(
    profile: FrontendUserProfile,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    info!("Updating user profile...");
    let mut service = state.service.lock().unwrap();
    
    let core_profile = human_money_core::models::profile::PublicProfile {
        protocol_version: profile.protocol_version,
        id: profile.id,
        first_name: profile.first_name,
        last_name: profile.last_name,
        organization: profile.organization,
        community: profile.community,
        address: profile.address.map(|a| human_money_core::models::voucher::Address {
            street: a.street,
            house_number: a.house_number,
            zip_code: a.zip_code,
            city: a.city,
            country: a.country,
            full_address: a.full_address,
        }),
        gender: profile.gender,
        email: profile.email,
        phone: profile.phone,
        coordinates: profile.coordinates,
        url: profile.url,
        service_offer: profile.service_offer,
        needs: profile.needs,
        picture_url: profile.picture_url,
    };
    
    service.update_public_profile(core_profile, password.as_deref()).map_err(FrontendError::from)
}

#[tauri::command]
pub fn set_conflict_local_override(
    proof_id: String,
    value: bool,
    note: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    info!("Setting local override for conflict {} to {} with note: {:?}", proof_id, value, note);
    let mut service = state.service.lock().unwrap();
    
    if let Some(ref pwd) = password {
        if let Err(e) = service.unlock_session(pwd, 300) {
            error!("Failed to unlock session for local override: {}", e);
        }
    }
    
    service.set_conflict_local_override(&proof_id, value, note, password.as_deref()).map_err(FrontendError::from)
}

// --- PEX / Seed Resolver ---

async fn ping_url_3median(client: &reqwest::Client, url: &str) -> Option<u128> {
    let mut successes: Vec<u128> = Vec::new();
    let target = url.trim_end_matches('/');
    if target.is_empty() {
        return None;
    }
    for _ in 0..3 {
        let start = Instant::now();
        match client.get(target).send().await {
            Ok(resp) => {
                // Any response (even 404) counts as reachable; we just measure time to first byte
                let _ = resp.status();
                let elapsed = start.elapsed().as_millis();
                successes.push(elapsed);
            }
            Err(_) => {
                // failed attempt, ignore
            }
        }
    }
    if successes.is_empty() {
        None
    } else {
        successes.sort_unstable();
        Some(successes[successes.len() / 2])
    }
}

async fn filter_reachable_with_median(candidates: Vec<String>) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(1800))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Failed to build HTTP client for median ping: {}", e);
            return Vec::new();
        }
    };
    let mut results: Vec<(String, u128)> = Vec::new();
    for url in candidates {
        let trimmed = url.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(median) = ping_url_3median(&client, &trimmed).await {
            results.push((trimmed, median));
        } else {
            log::debug!("Node unreachable filtered: {}", trimmed);
        }
    }
    results.sort_by_key(|(_, lat)| *lat);
    results.into_iter().map(|(url, _)| url).collect()
}

async fn fetch_seed_peers(seed_url: &str) -> Option<Vec<String>> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(3000))
        .build()
        .ok()?;
    let mut resp = client.get(seed_url).send().await.ok()?;
    if !resp.status().is_success() {
        log::debug!("Seed fetch failed with status {} for {}", resp.status(), seed_url);
        return None;
    }
    if resp.content_length().is_some_and(|length| length > 65_536) { return None; }
    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await.ok()? {
        if bytes.len().saturating_add(chunk.len()) > 65_536 { return None; }
        bytes.extend_from_slice(&chunk);
    }
    // Try direct Vec<String>
    if let Ok(peers) = serde_json::from_slice::<Vec<String>>(&bytes) {
        let filtered: Vec<String> = peers.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        if !filtered.is_empty() {
            return Some(filtered);
        }
    }
    // Try object with known keys
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        let keys_to_try = ["peers", "nodes", "servers", "urls", "endpoints", "seedPeers", "seed_peers"];
        for key in keys_to_try {
            if let Some(arr) = val.get(key).and_then(|v| v.as_array()) {
                let peers: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect();
                if !peers.is_empty() {
                    return Some(peers);
                }
            }
        }
        if let Some(obj) = val.as_object() {
            for (_, v) in obj {
                if let Some(arr) = v.as_array() {
                    let peers: Vec<String> = arr.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect();
                    if !peers.is_empty() && peers.iter().any(|p| p.starts_with("http")) {
                        return Some(peers);
                    }
                }
            }
        }
    }
    None
}

/// Resolves the active L2 nodes based on AppSettings.
/// Steps:
/// 1. If exclusive_custom_servers == true: use only active custom_servers.
/// 2. If custom_servers are present: test these first.
/// 3. Otherwise: use cached_peers for active network or load seed (GitHub fallback).
/// 4. Uses 3-ping median measurement and filters unreachable nodes.
/// 5. Returns empty list if no nodes reachable (clean offline mode, no panics).
pub async fn resolve_active_l2_nodes_internal(settings: &AppSettings) -> Vec<String> {
    let active_custom: Vec<String> = settings
        .custom_servers
        .iter()
        .filter(|s| s.is_active)
        .map(|s| s.url.trim().to_string())
        .filter(|u| !u.is_empty())
        .collect();

    // 1. Exclusive mode: only active custom servers
    if settings.exclusive_custom_servers {
        if active_custom.is_empty() {
            return Vec::new();
        }
        return filter_reachable_with_median(active_custom).await;
    }

    // 2. Custom servers present: test these first
    if !active_custom.is_empty() {
        let reachable_custom = filter_reachable_with_median(active_custom.clone()).await;
        if !reachable_custom.is_empty() {
            return reachable_custom;
        }
        // if none reachable, fall through to seed/cached peers
    }

    // 3. Use cached peers for active network or load seed
    let is_testnet = settings.active_network.to_lowercase() == "testnet";
    let cached = if is_testnet {
        &settings.cached_peers_testnet
    } else {
        &settings.cached_peers_mainnet
    };

    let mut candidates: Vec<String> = if !cached.is_empty() {
        cached.clone()
    } else {
        let seed_url = settings
            .bootstrap_seed_url
            .clone()
            .unwrap_or_else(|| {
                if is_testnet {
                    TESTNET_SEED_FALLBACK.to_string()
                } else {
                    MAINNET_SEED_FALLBACK.to_string()
                }
            });
        match fetch_seed_peers(&seed_url).await {
            Some(peers) if !peers.is_empty() => peers,
            _ => Vec::new(),
        }
    };

    // Also consider legacy l2_node_urls as additional fallback if no candidates yet
    if candidates.is_empty() {
        if let Some(ref legacy) = settings.l2_node_urls {
            if !legacy.is_empty() {
                candidates = legacy.clone();
            }
        }
    }

    // Clean and dedup
    candidates = candidates
        .into_iter()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .collect();
    candidates.sort();
    candidates.dedup();

    if candidates.is_empty() {
        return Vec::new();
    }

    // 4. 3-ping median and filter unreachable
    filter_reachable_with_median(candidates).await
}

/// Tauri command that benchmarks L2 servers and measures latencies using 3-ping median.
/// If `urls` is Some, benchmarks those URLs directly; otherwise benchmarks the resolved active nodes.
#[tauri::command]
pub async fn benchmark_l2_servers(
    state: tauri::State<'_, AppState>,
    urls: Option<Vec<String>>,
) -> Result<Vec<L2BenchmarkResult>, FrontendError> {
    let candidates: Vec<String> = if let Some(provided) = urls {
        provided
            .into_iter()
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty())
            .collect()
    } else {
        let settings = state.get_cached_settings().map_err(FrontendError::from)?;
        let is_testnet = settings.active_network.to_lowercase() == "testnet";
        let mut all: Vec<String> = Vec::new();
        let active_custom: Vec<String> = settings
            .custom_servers
            .iter()
            .filter(|s| s.is_active)
            .map(|s| s.url.trim().to_string())
            .filter(|u| !u.is_empty())
            .collect();

        if settings.exclusive_custom_servers {
            all = active_custom;
        } else {
            all.extend(active_custom.clone());
            let cached = if is_testnet {
                &settings.cached_peers_testnet
            } else {
                &settings.cached_peers_mainnet
            };
            if !cached.is_empty() {
                all.extend(cached.clone());
            } else {
                let seed_url = settings
                    .bootstrap_seed_url
                    .clone()
                    .unwrap_or_else(|| {
                        if is_testnet {
                            TESTNET_SEED_FALLBACK.to_string()
                        } else {
                            MAINNET_SEED_FALLBACK.to_string()
                        }
                    });
                if let Some(peers) = fetch_seed_peers(&seed_url).await {
                    all.extend(peers);
                }
            }
            if let Some(ref legacy) = settings.l2_node_urls {
                all.extend(legacy.clone());
            }
        }
        all.into_iter().map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect()
    };

    let mut deduped = candidates;
    deduped.sort();
    deduped.dedup();
    if deduped.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(1800))
        .build()
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let mut results: Vec<L2BenchmarkResult> = Vec::new();
    for url in deduped {
        let median = ping_url_3median(&client, &url).await;
        results.push(L2BenchmarkResult {
            url: url.clone(),
            latency_ms: median.map(|m| m as u64),
            reachable: median.is_some(),
        });
    }
    results.sort_by(|a, b| match (a.latency_ms, b.latency_ms) {
        (Some(a_lat), Some(b_lat)) => a_lat.cmp(&b_lat),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    Ok(results)
}

// --- WalletSeal / Sync Commands ---

#[tauri::command]
pub fn get_seal_sync_status(state: tauri::State<AppState>) -> Result<String, FrontendError> {
    let service = state.service.lock().unwrap();
    let status = service.get_seal_sync_status().map_err(FrontendError::from)?;
    let status_str = match status {
        human_money_core::models::seal::SyncStatus::PendingUpload => "PendingUpload",
        human_money_core::models::seal::SyncStatus::Synced => "Synced",
    };
    Ok(status_str.to_string())
}

#[tauri::command]
pub fn get_seal_for_upload(state: tauri::State<AppState>) -> Result<Option<SealUploadData>, FrontendError> {
    let service = state.service.lock().unwrap();
    
    let data = service.get_seal_for_upload().map_err(FrontendError::from)?;
    if let Some(bytes) = data {
        let seal: human_money_core::models::seal::WalletSeal = serde_json::from_slice(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse seal for hashing: {}", e)))?;
        
        let hash = seal.compute_hash().map_err(FrontendError::from)?;
            
        Ok(Some(SealUploadData {
            seal_bytes: bytes,
            seal_hash: hash,
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn acknowledge_seal_sync(
    uploaded_seal_hash: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();
    service.acknowledge_seal_sync(&uploaded_seal_hash, password.as_deref()).map_err(FrontendError::from)
}

#[tauri::command]
pub async fn check_anchor_sync(
    state: tauri::State<'_, AppState>,
) -> Result<String, FrontendError> {
    log::debug!("Checking L2 anchor sync status...");
    let settings = match state.get_cached_settings() {
        Ok(s) => s,
        Err(_) => return Ok("Offline".to_string()),
    };
    let candidates = crate::gateway_resolver::cached_candidates(&settings);
    if candidates.is_empty() {
        log::debug!("No L2 servers configured for anchor sync");
        return Ok("NoServers".to_string());
    }

    let query = {
        let service = match state.service.lock() {
            Ok(s) => s,
            Err(_) => return Ok("Offline".to_string()),
        };
        match service.prepare_anchor_status_query() {
            Ok((data, _, _)) => data,
            Err(e) => {
                log::debug!("prepare_anchor_status_query returned: {}", e);
                return Ok("Offline".to_string());
            }
        }
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(2500))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::debug!("Failed to build HTTP client: {}", e);
            return Ok("Offline".to_string());
        }
    };

    let mut envelopes = Vec::new();
    for candidate in &candidates {
        let status_url = match crate::gateway_resolver::endpoint(candidate, "v1/status") {
            Some(u) => u,
            None => continue,
        };
        match client.post(status_url).json(&query).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(resp_bytes) = resp.bytes().await {
                    if let Ok(envelope) = serde_json::from_slice::<human_money_core::models::layer2_api::L2ResponseEnvelope>(&resp_bytes) {
                        envelopes.push(envelope);
                    }
                }
            }
            Ok(resp) => {
                log::debug!("Node {} returned HTTP {}", candidate, resp.status());
            }
            Err(e) => {
                log::debug!("Node {} connection failed: {}", candidate, e);
            }
        }
    }

    if envelopes.is_empty() {
        return Ok("Offline".to_string());
    }

    let service = match state.service.lock() {
        Ok(s) => s,
        Err(_) => return Ok("Offline".to_string()),
    };

    let verdict = match service.evaluate_verified_anchor_envelopes(&envelopes) {
        Ok(v) => v,
        Err(e) => {
            log::debug!("evaluate_verified_anchor_envelopes returned: {}", e);
            return Ok("Inconclusive".to_string());
        }
    };

    let status_str = match verdict {
        human_money_core::services::l2_gateway::AnchorSyncVerdict::Synchronized => "Synchronized",
        human_money_core::services::l2_gateway::AnchorSyncVerdict::RollbackDetected { .. } => "RollbackDetected",
        human_money_core::services::l2_gateway::AnchorSyncVerdict::ForkConflict { .. } => "ForkConflict",
        human_money_core::services::l2_gateway::AnchorSyncVerdict::PendingLeapLock { .. } => "PendingLeapLock",
        human_money_core::services::l2_gateway::AnchorSyncVerdict::GenesisRequired => "PendingLeapLock",
        human_money_core::services::l2_gateway::AnchorSyncVerdict::Inconclusive => "Inconclusive",
    };

    log::debug!("Anchor sync check evaluated to: {}", status_str);
    Ok(status_str.to_string())
}

#[tauri::command]
pub async fn reconcile_rollback(
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendRollbackReconciliationDto, FrontendError> {
    info!("Reconciling rollback with Layer 2 network...");
    if let Some(ref pwd) = password {
        let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = service.unlock_session(pwd, 300);
    }

    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let candidates = crate::gateway_resolver::cached_candidates(&settings);

    let voucher_queries = {
        let service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service.with_wallet_and_identity(|wallet, identity| {
            let mut queries = Vec::new();
            for (id, instance) in &wallet.voucher_store.vouchers {
                if instance.status == human_money_core::wallet::instance::VoucherStatus::Active {
                    if let Ok(mut query) = wallet.build_voucher_status_query(identity, id, 1) {
                        query.read_quorum = 1;
                        queries.push((id.clone(), query));
                    }
                }
            }
            queries
        }).map_err(FrontendError::from)?
    };

    let mut spent_voucher_ids = Vec::new();

    if !candidates.is_empty() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(2500))
            .build()
            .map_err(|_| FrontendError::from("HTTP client unavailable".to_string()))?;

        for (local_id, query) in voucher_queries {
            let body_bytes = match serde_json::to_vec(&query) {
                Ok(b) => b,
                Err(_) => continue,
            };

            for base in &candidates {
                let Some(url) = crate::gateway_resolver::endpoint(base, "v1/status") else { continue };
                let mut request = client.post(url.clone())
                    .header("Content-Type", "application/json")
                    .body(body_bytes.clone());

                if let Some(provider) = settings.custom_servers.iter().find(|p| {
                    p.is_active
                        && crate::gateway_resolver::endpoint(&p.url, "v1/status")
                            .is_some_and(|k| k.origin() == url.origin())
                }) {
                    if let Some(token) = &provider.auth_token {
                        request = request.bearer_auth(token);
                    }
                    if let Some(token) = &provider.peer_token {
                        if token.len() == 36 && token.starts_with("f2f_") && token[4..].bytes().all(|b| b.is_ascii_hexdigit()) {
                            request = request.header("X-Peer-Token", token);
                        }
                    }
                }

                let response = tokio::time::timeout(std::time::Duration::from_millis(2500), request.send()).await;
                let Ok(Ok(resp)) = response else { continue };
                if !resp.status().is_success() { continue; }
                let Ok(resp_bytes) = resp.bytes().await else { continue };

                let verdict_opt = if let Ok(envelope) = serde_json::from_slice::<human_money_core::models::layer2_api::L2ResponseEnvelope>(&resp_bytes) {
                    Some(envelope.verdict)
                } else {
                    serde_json::from_slice::<human_money_core::models::layer2_api::L2Verdict>(&resp_bytes).ok()
                };

                if let Some(verdict) = verdict_opt {
                    let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
                    match service.reconcile_voucher_verdict(&local_id, &verdict, password.as_deref()) {
                        Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::AlreadySpent { .. }) => {
                            spent_voucher_ids.push(local_id);
                            break;
                        }
                        Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::ConflictDetected { .. }) => {
                            spent_voucher_ids.push(local_id);
                            break;
                        }
                        Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::UnspentSafe) => {
                            break;
                        }
                        Err(e) => {
                            log::debug!("Voucher reconciliation rejected: {}", e);
                        }
                    }
                }
            }
        }
    }

    let report = {
        let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
            .reconcile_rollback_with_diagnostics(&spent_voucher_ids, None, password.as_deref())
            .map_err(FrontendError::from)?
    };

    Ok(report.into())
}

#[tauri::command]
pub async fn flush_pending_anchor_sync(
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<bool, FrontendError> {
    info!("Flushing pending anchor sync to L2...");
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let candidates = crate::gateway_resolver::cached_candidates(&settings);
    if candidates.is_empty() {
        return Ok(false);
    }

    let plan = {
        let service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        match service.prepare_anchor_sync_upload().map_err(FrontendError::from)? {
            Some(plan) => plan,
            None => return Ok(false),
        }
    };

    let req_bytes = serde_json::to_vec(&plan.chain_request).map_err(|e| FrontendError::from(e.to_string()))?;
    let http_status = crate::gateway_resolver::dispatch_exact(&settings, &candidates, &req_bytes)
        .await
        .map_err(FrontendError::from)?;

    if matches!(http_status, Some(200 | 201)) {
        let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service.complete_anchor_sync_upload(plan.target_checkpoint, password.as_deref()).map_err(FrontendError::from)
    } else {
        log::debug!("L2 anchor flush returned non-success HTTP status: {:?}", http_status);
        Ok(false)
    }
}

/// Explicit cold-path refresh. Public gateway directories select messengers;
/// independently pinned regional authority remains the checkout trust source.
#[tauri::command]
pub async fn refresh_l2_gateway_cache(password: Option<String>, state: tauri::State<'_, AppState>) -> Result<Vec<String>, FrontendError> {
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let candidates = crate::gateway_resolver::cached_candidates(&settings);
    if settings.exclusive_custom_servers { return Ok(candidates); }
    let owner = {
        let service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service.with_wallet(|wallet| wallet.get_user_id().to_string()).map_err(FrontendError::from)?
    };
    let network = settings.active_network.clone();
    let candidates = if candidates.is_empty() {
        let seed = settings.bootstrap_seed_url.clone().unwrap_or_else(|| if network.eq_ignore_ascii_case("testnet") { TESTNET_SEED_FALLBACK.into() } else { MAINNET_SEED_FALLBACK.into() });
        fetch_seed_peers(&seed).await.unwrap_or_default().into_iter().take(32).collect()
    } else { candidates };
    let mut refreshed = candidates.clone();
    for url in crate::gateway_resolver::discover_gateways(&candidates).await {
        if !refreshed.contains(&url) { refreshed.push(url); }
        if refreshed.len() == 32 { break; }
    }
    let mut settings = state.get_cached_settings().map_err(FrontendError::from)?;
    if settings.active_network != network { return Err(FrontendError::from("Network changed during discovery".to_string())); }
    let existing_peers = if network.eq_ignore_ascii_case("testnet") {
        &settings.cached_peers_testnet
    } else if network.eq_ignore_ascii_case("mainnet") {
        &settings.cached_peers_mainnet
    } else {
        return Err(FrontendError::from("Invalid L2 network configuration".to_string()));
    };
    if existing_peers != &refreshed {
        if network.eq_ignore_ascii_case("testnet") { settings.cached_peers_testnet = refreshed.clone(); }
        else { settings.cached_peers_mainnet = refreshed.clone(); }
        let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        if service.with_wallet(|wallet| wallet.get_user_id().to_string()).map_err(FrontendError::from)? != owner { return Err(FrontendError::from("Profile changed during discovery".to_string())); }
        state.save_settings(&mut service, settings, password.as_deref()).map_err(FrontendError::from)?;
    }
    Ok(refreshed)
}

/// Refreshes authenticated observation sources on the cold path only.
#[tauri::command]
pub async fn refresh_regional_mesh_views(state:tauri::State<'_,AppState>)->Result<bool,FrontendError> {
    crate::checkout::refresh_views(&state).await
}

#[tauri::command]
pub async fn check_voucher_l2_status(
    local_instance_id: String,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendVoucherL2StatusDto, FrontendError> {
    let (query, is_currently_archived) = {
        let service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service.with_wallet_and_identity(|wallet, identity| {
            let instance = wallet.voucher_store.vouchers.get(&local_instance_id)
                .ok_or_else(|| human_money_core::Error::App(human_money_core::error::AppError::InvariantViolation { message: "Voucher not found".into() }))?;
            let is_archived = matches!(instance.status, human_money_core::wallet::instance::VoucherStatus::Archived);
            let mut query = wallet.build_voucher_status_query(identity, &local_instance_id, 1)?;
            query.read_quorum = 1;
            Ok::<_, human_money_core::Error>((query, is_archived))
        }).map_err(FrontendError::from)?
        .map_err(FrontendError::from)?
    };

    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let candidates = crate::gateway_resolver::cached_candidates(&settings);
    if candidates.is_empty() {
        return Ok(FrontendVoucherL2StatusDto {
            status: "no_servers".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
        });
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(2500))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::debug!("Failed to build HTTP client: {}", e);
            return Ok(FrontendVoucherL2StatusDto {
                status: "offline".to_string(),
                archived: is_currently_archived,
                successor_t_id: None,
            });
        }
    };

    let body_bytes = match serde_json::to_vec(&query) {
        Ok(b) => b,
        Err(_) => return Ok(FrontendVoucherL2StatusDto {
            status: "offline".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
        }),
    };

    let mut at_least_one_responded = false;

    for base in &candidates {
        let Some(url) = crate::gateway_resolver::endpoint(base, "v1/status") else { continue };
        let mut request = client.post(url.clone())
            .header("Content-Type", "application/json")
            .body(body_bytes.clone());

        if let Some(provider) = settings.custom_servers.iter().find(|p| {
            p.is_active
                && crate::gateway_resolver::endpoint(&p.url, "v1/status")
                    .is_some_and(|k| k.origin() == url.origin())
        }) {
            if let Some(token) = &provider.auth_token {
                request = request.bearer_auth(token);
            }
            if let Some(token) = &provider.peer_token {
                if token.len() == 36 && token.starts_with("f2f_") && token[4..].bytes().all(|b| b.is_ascii_hexdigit()) {
                    request = request.header("X-Peer-Token", token);
                }
            }
        }

        let response = tokio::time::timeout(std::time::Duration::from_millis(2500), request.send()).await;
        let Ok(Ok(resp)) = response else { continue };
        at_least_one_responded = true;
        if !resp.status().is_success() { continue; }
        let Ok(resp_bytes) = resp.bytes().await else { continue };

        let verdict_opt = if let Ok(envelope) = serde_json::from_slice::<human_money_core::models::layer2_api::L2ResponseEnvelope>(&resp_bytes) {
            Some(envelope.verdict)
        } else {
            serde_json::from_slice::<human_money_core::models::layer2_api::L2Verdict>(&resp_bytes).ok()
        };

        if let Some(verdict) = verdict_opt {
            let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            match service.reconcile_voucher_verdict(&local_instance_id, &verdict, password.as_deref()) {
                Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::AlreadySpent { successor_t_id }) => {
                    return Ok(FrontendVoucherL2StatusDto {
                        status: "spent".to_string(),
                        archived: true,
                        successor_t_id: Some(successor_t_id),
                    });
                }
                Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::ConflictDetected { conflicting_lock }) => {
                    let successor_t_id = bs58::encode(conflicting_lock.t_id).into_string();
                    return Ok(FrontendVoucherL2StatusDto {
                        status: "conflict".to_string(),
                        archived: true,
                        successor_t_id: Some(successor_t_id),
                    });
                }
                Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::UnspentSafe) => {
                    return Ok(FrontendVoucherL2StatusDto {
                        status: "unspent".to_string(),
                        archived: is_currently_archived,
                        successor_t_id: None,
                    });
                }
                Err(e) => {
                    log::debug!("Voucher reconciliation rejected invalid verdict: {}", e);
                }
            }
        }
    }

    if at_least_one_responded {
        Ok(FrontendVoucherL2StatusDto {
            status: "unspent".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
        })
    } else {
        Ok(FrontendVoucherL2StatusDto {
            status: "offline".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontend_rollback_reconciliation_dto_serialization() {
        let report = human_money_core::models::seal::RollbackReconciliationReport {
            quarantined_voucher_ids: vec!["v1".to_string()],
            archived_voucher_ids: vec!["v2".to_string()],
            retained_voucher_ids: vec!["v3".to_string()],
            new_tx_nonce: 42,
            consolidated_state_hash: "hash123".to_string(),
            message: "Reconciled".to_string(),
        };
        let dto: FrontendRollbackReconciliationDto = report.into();
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"newTxNonce\":42"));
        assert!(json.contains("\"consolidatedStateHash\":\"hash123\""));
        assert!(json.contains("\"quarantinedVoucherIds\":[\"v1\"]"));
        assert!(json.contains("\"archivedVoucherIds\":[\"v2\"]"));
        assert!(json.contains("\"retainedVoucherIds\":[\"v3\"]"));
        assert!(json.contains("\"message\":\"Reconciled\""));
    }

    #[test]
    fn test_frontend_voucher_l2_status_dto_serialization() {
        let dto = FrontendVoucherL2StatusDto {
            status: "spent".to_string(),
            archived: true,
            successor_t_id: Some("succ123".to_string()),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"status\":\"spent\""));
        assert!(json.contains("\"archived\":true"));
        assert!(json.contains("\"successorTId\":\"succ123\""));

        let unspent_dto = FrontendVoucherL2StatusDto {
            status: "unspent".to_string(),
            archived: false,
            successor_t_id: None,
        };
        let unspent_json = serde_json::to_string(&unspent_dto).unwrap();
        assert!(!unspent_json.contains("successorTId"));
    }
}
