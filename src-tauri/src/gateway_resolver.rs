//! Exact-byte HTTP dispatch with provider-scoped credentials. Core owns intent
//! durability; this module owns transport only and cannot authorize release.
use crate::settings::AppSettings;
use serde::Serialize;

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentDispatchObservation {
    pub local_instance_id: String,
    pub status: String,
    pub http_status: Option<u16>,
}
pub(crate) fn endpoint(raw: &str, path: &str) -> Option<reqwest::Url> {
    let mut url = reqwest::Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    url.set_path(&format!(
        "{}/{}",
        url.path().trim_end_matches('/'),
        path.trim_start_matches('/')
    ));
    Some(url)
}
/// Every attempt builds a new request and derives credentials ONLY from that
/// destination's configured origin. Redirects cannot carry secrets elsewhere.
pub(crate) async fn dispatch_exact(settings:&AppSettings,candidates:&[String],bytes:&[u8])->Result<Option<u16>,String> {
    dispatch_path(settings,candidates,bytes,"v1/lock",tokio::time::Instant::now()+std::time::Duration::from_millis(800)).await
}
/// Each transfer input gets its own transport budget. Causal chains must
/// persist and quorum-verify every hop, so they receive 800 ms per hop with a
/// 3000 ms floor; single locks keep the 800 ms checkout budget. None of these
/// status observations can authorize goods release or synthesize a replacement
/// spend.
pub(crate) async fn dispatch_intents(settings:&AppSettings,candidates:&[String],intents:&[human_money_core::models::l2_intent::L2SendIntent])->Vec<IntentDispatchObservation> {
    let mut observations = Vec::new();
    for intent in intents {
        let (bytes,path,hops) = if intent.chain_request_bytes.is_empty() {
            (&intent.request_bytes,"v1/lock",1u64)
        } else {
            let hops = serde_json::from_slice::<human_money_core::models::layer2_api::L2ChainLockRequest>(&intent.chain_request_bytes)
                .map(|chain|chain.chain.len().max(1) as u64).unwrap_or(1);
            (&intent.chain_request_bytes,"v1/lock/chain",hops)
        };
        let budget_ms = if path == "v1/lock" {800} else {hops.saturating_mul(800).max(3000)};
        let deadline = tokio::time::Instant::now()+std::time::Duration::from_millis(budget_ms);
        let status = if tokio::time::Instant::now()<deadline {
            dispatch_path(settings,candidates,bytes,path,deadline).await.ok().flatten()
        } else { None };
        observations.push(IntentDispatchObservation {local_instance_id:intent.local_instance_id.clone(),status:"pending".into(),http_status:status});
    }
    observations
}
#[derive(serde::Deserialize)]
struct LiveContextHolder {
    live_context: human_money_core::humoco_proto::live::LiveStateContext,
}

#[derive(serde::Deserialize)]
struct PowChallengeBody {
    challenge: Option<String>,
    difficulty: Option<u32>,
    expires_at: Option<u64>,
}

pub(crate) fn extract_pow_parent_lock(request_bytes: &[u8]) -> [u8; 32] {
    if let Ok(req) = serde_json::from_slice::<human_money_core::models::layer2_api::L2CheckoutRequest>(request_bytes) {
        return req.live_context.parent_lock;
    }
    if let Ok(holder) = serde_json::from_slice::<LiveContextHolder>(request_bytes) {
        return holder.live_context.parent_lock;
    }
    if let Ok(req) = serde_json::from_slice::<human_money_core::models::layer2_api::L2ChainLockRequest>(request_bytes) {
        if let Some(last) = req.chain.last() {
            if last.is_genesis {
                return [0u8; 32];
            } else {
                return *blake3::hash(last.ds_tag.as_deref().unwrap_or_default().as_bytes()).as_bytes();
            }
        }
    }
    if let Ok(req) = serde_json::from_slice::<human_money_core::models::layer2_api::L2LockRequest>(request_bytes) {
        if req.is_genesis {
            return [0u8; 32];
        } else {
            return *blake3::hash(req.ds_tag.as_deref().unwrap_or_default().as_bytes()).as_bytes();
        }
    }
    [0u8; 32]
}

pub(crate) async fn bounded_body(mut response: reqwest::Response, ceiling: usize) -> Option<Vec<u8>> {
    if response.content_length().is_some_and(|len| len > ceiling as u64) {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len().saturating_add(chunk.len()) > ceiling {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

async fn fetch_fallback_pow_challenge(
    client: &reqwest::Client,
    base: &str,
    deadline: tokio::time::Instant,
) -> Option<(String, u32, Option<u64>)> {
    let url = endpoint(base, "v1/pow-challenge")?;
    let response = tokio::time::timeout_at(deadline, client.get(url).send()).await.ok()?.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = tokio::time::timeout_at(deadline, bounded_body(response, 4096)).await.ok()??;
    let body: PowChallengeBody = serde_json::from_slice(&bytes).ok()?;
    let ch = body.challenge?;
    let diff = body.difficulty?;
    Some((ch, diff, body.expires_at))
}

async fn solve_public_pow(
    client: &reqwest::Client,
    base: &str,
    parent_lock: &[u8; 32],
    response_body: Option<Vec<u8>>,
    deadline: tokio::time::Instant,
) -> Option<(String, u64)> {
    let (challenge_hex, difficulty, expires_at) = if let Some(bytes) = response_body {
        if let Ok(body) = serde_json::from_slice::<PowChallengeBody>(&bytes) {
            if let (Some(ch), Some(diff)) = (body.challenge, body.difficulty) {
                (ch, diff, body.expires_at)
            } else {
                fetch_fallback_pow_challenge(client, base, deadline).await?
            }
        } else {
            fetch_fallback_pow_challenge(client, base, deadline).await?
        }
    } else {
        fetch_fallback_pow_challenge(client, base, deadline).await?
    };

    if difficulty > 24 {
        return None;
    }
    let challenge_raw = human_money_core::hex::decode(&challenge_hex).ok()?;
    let challenge_bytes: [u8; 32] = challenge_raw.try_into().ok()?;

    let now_sec = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    use human_money_core::humoco_proto::crypto::{
        compute_stateless_pow_challenge, solve_stateless_pow, DEFAULT_POW_EPOCH_DURATION_SEC,
    };

    let base_slot = expires_at
        .map(|e| (e / DEFAULT_POW_EPOCH_DURATION_SEC).saturating_sub(1))
        .unwrap_or_else(|| now_sec / DEFAULT_POW_EPOCH_DURATION_SEC);

    let candidate_slots = [
        base_slot,
        base_slot.saturating_sub(1),
        base_slot.saturating_add(1),
    ];

    let mut slot_opt = None;
    for &slot in &candidate_slots {
        if compute_stateless_pow_challenge(parent_lock, slot) == challenge_bytes {
            slot_opt = Some(slot);
            break;
        }
    }
    let slot = slot_opt?;

    let start = uuid::Uuid::new_v4().as_u128() as u64;
    for batch in 0..256u64 {
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        let batch_start = start.wrapping_add(batch.wrapping_mul(4096));
        if let Some(nonce) = solve_stateless_pow(parent_lock, slot, difficulty, batch_start, 4096) {
            return Some((challenge_hex, nonce));
        }
        tokio::task::yield_now().await;
    }
    None
}

async fn dispatch_path(
    settings: &AppSettings,
    candidates: &[String],
    bytes: &[u8],
    path: &str,
    deadline: tokio::time::Instant,
) -> Result<Option<u16>, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "HTTP client unavailable".to_string())?;
    let parent_lock = extract_pow_parent_lock(bytes);
    for base in candidates.iter().take(4) {
        let Some(url) = endpoint(base, path) else { continue };
        let mut pow: Option<(String, u64)> = None;
        for attempt in 0..2 {
            let mut request = client.post(url.clone()).header("Content-Type", "application/json").body(bytes.to_vec());
            if let Some(provider) = settings.custom_servers.iter().find(|provider| {
                provider.is_active && endpoint(&provider.url, path).is_some_and(|known| known.origin() == url.origin())
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
            if let Some((challenge, nonce)) = &pow {
                request = request.header("X-PoW-Challenge", challenge).header("X-PoW-Nonce", nonce.to_string());
            }
            match tokio::time::timeout_at(deadline, request.send()).await {
                Ok(Ok(response)) if response.status().as_u16() == 401 && attempt == 0 => {
                    let body = tokio::time::timeout_at(deadline, bounded_body(response, 4096)).await.ok().flatten();
                    pow = solve_public_pow(&client, base, &parent_lock, body, deadline).await;
                    if pow.is_some() {
                        continue;
                    } else {
                        return Ok(Some(401));
                    }
                }
                Ok(Ok(response)) if matches!(response.status().as_u16(), 429 | 500..=599) => break,
                Ok(Ok(response)) => return Ok(Some(response.status().as_u16())),
                Ok(Err(_)) => break,
                Err(_) => return Ok(None),
            }
        }
    }
    Ok(None)
}

pub(crate) async fn fetch_checkout_proof(
    settings: &AppSettings,
    candidates: &[String],
    req: &human_money_core::models::layer2_api::L2CheckoutRequest,
) -> Result<Option<human_money_core::models::layer2_api::L2CheckoutResponse>, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "HTTP client unavailable".to_string())?;

    let hops = req.lock_chain.as_ref().map(|c| c.chain.len()).unwrap_or(0);
    let budget_ms = if hops > 1 { 1200 } else { 800 };
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(budget_ms);
    let bytes = serde_json::to_vec(req).map_err(|e| e.to_string())?;
    let parent_lock = req.live_context.parent_lock;

    for base in candidates.iter().take(4) {
        let Some(url) = endpoint(base, "v1/checkout") else { continue };
        let mut pow: Option<(String, u64)> = None;
        for attempt in 0..2 {
            let mut request = client.post(url.clone()).header("Content-Type", "application/json").body(bytes.clone());
            if let Some(provider) = settings.custom_servers.iter().find(|provider| {
                provider.is_active && endpoint(&provider.url, "v1/checkout").is_some_and(|known| known.origin() == url.origin())
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
            if let Some((challenge, nonce)) = &pow {
                request = request.header("X-PoW-Challenge", challenge).header("X-PoW-Nonce", nonce.to_string());
            }
            match tokio::time::timeout_at(deadline, request.send()).await {
                Ok(Ok(response)) if response.status().as_u16() == 401 && attempt == 0 => {
                    let body = tokio::time::timeout_at(deadline, bounded_body(response, 4096)).await.ok().flatten();
                    pow = solve_public_pow(&client, base, &parent_lock, body, deadline).await;
                    if pow.is_some() {
                        continue;
                    } else {
                        break;
                    }
                }
                Ok(Ok(response)) if response.status().as_u16() == 200 => {
                    let body = match tokio::time::timeout_at(deadline, bounded_body(response, 131_072)).await {
                        Ok(Some(bytes)) => bytes,
                        _ => break,
                    };
                    if let Ok(resp) = serde_json::from_slice::<human_money_core::models::layer2_api::L2CheckoutResponse>(&body) {
                        return Ok(Some(resp));
                    } else {
                        break;
                    }
                }
                Ok(Ok(response)) if matches!(response.status().as_u16(), 429 | 500..=599) => break,
                Ok(Ok(_)) => break,
                Ok(Err(_)) => break,
                Err(_) => return Ok(None),
            }
        }
    }
    Ok(None)
}
/// One payment gateway collects signatures. Authority comes from the recipient's
/// independent cold cache and Core's stored purchase, never from this response.
pub(crate) async fn fetch_live_proof(candidates:&[String],context:&human_money_core::humoco_proto::live::LiveStateContext)
    ->Result<Option<human_money_core::humoco_proto::live::LiveStateProof>,String> {
    let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().map_err(|_|"HTTP client unavailable".to_string())?;
    let deadline=tokio::time::Instant::now()+std::time::Duration::from_millis(800);
    let bytes=serde_json::to_vec(context).map_err(|_|"Invalid live context".to_string())?;
    for base in candidates.iter().take(4) {
        let Some(url)=endpoint(base,"v1/verify-current") else {continue};
        let result=tokio::time::timeout_at(deadline,client.post(url).header("Content-Type","application/json").body(bytes.clone()).send()).await;
        let response=match result {Ok(Ok(response)) if response.status().is_success()=>response,Ok(_)=>continue,Err(_)=>return Ok(None)};
        let body=match tokio::time::timeout_at(deadline,bounded_body(response,65_536)).await {Ok(Some(bytes))=>bytes,_=>continue};
        if let Ok(proof)=serde_json::from_slice(&body) {return Ok(Some(proof));}
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::CustomServerDto;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn server(status: u16) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut block = [0; 1024];
            loop {
                let n = stream.read(&mut block).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&block[..n]);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let size = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length: ")
                                .and_then(|size| size.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + size {
                        break;
                    }
                }
                assert!(bytes.len() < 65_536);
            }
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            bytes
        });
        (format!("http://{addr}"), task)
    }
    #[tokio::test]
    async fn failover_reuses_exact_body_and_never_reuses_other_origins_tokens() {
        let (first, a) = server(503).await;
        let (second, b) = server(201).await;
        let mut settings = AppSettings::default();
        settings.custom_servers.push(CustomServerDto {
            url: first.clone(),
            auth_token: Some("private-provider-secret".into()),
            peer_token: Some("f2f_0123456789abcdef0123456789abcdef".into()),
            ..Default::default()
        });
        let bytes = b"{\"created_at_ms\":1234,\"signed\":\"exact\"}";
        assert_eq!(
            dispatch_exact(&settings, &[first, second], bytes)
                .await
                .unwrap(),
            Some(201)
        );
        let a = a.await.unwrap();
        let b = b.await.unwrap();
        assert!(a.ends_with(bytes) && b.ends_with(bytes));
        assert!(String::from_utf8_lossy(&a).contains("private-provider-secret"));
        assert!(!String::from_utf8_lossy(&b).contains("private-provider-secret"));
        assert!(!String::from_utf8_lossy(&b)
            .to_lowercase()
            .contains("x-peer-token"));
    }
    #[tokio::test]
    async fn stateless_pow_challenge_in_401_body_is_solved_and_sent_on_retry() {
        use human_money_core::humoco_proto::crypto::{
            check_pow_difficulty, compute_stateless_pow_challenge, compute_stateless_pow_work,
            DEFAULT_POW_EPOCH_DURATION_SEC,
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_url = format!("http://{addr}");

        let parent_lock = [0x5au8; 32];
        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let slot = now_sec / DEFAULT_POW_EPOCH_DURATION_SEC;
        let expires_at = (slot + 1) * DEFAULT_POW_EPOCH_DURATION_SEC;
        let challenge_bytes = compute_stateless_pow_challenge(&parent_lock, slot);
        let challenge_hex = human_money_core::hex::encode(challenge_bytes);
        let difficulty = 8u32;

        let server_task = tokio::spawn(async move {
            let mut recorded_requests = Vec::new();
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut block = [0; 1024];
                loop {
                    let n = stream.read(&mut block).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&block[..n]);
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let size = headers
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("content-length: ")
                                    .and_then(|size| size.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + size {
                            break;
                        }
                    }
                    assert!(bytes.len() < 65_536);
                }
                let req_text = String::from_utf8_lossy(&bytes).to_string();
                recorded_requests.push(req_text);

                if attempt == 0 {
                    let body = format!(
                        r#"{{"error":"PoWRequired","message":"Proof-of-Work challenge response required","challenge":"{challenge_hex}","difficulty":{difficulty},"expires_at":{expires_at}}}"#
                    );
                    let resp = format!(
                        "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(resp.as_bytes()).await.unwrap();
                } else {
                    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    stream.write_all(resp.as_bytes()).await.unwrap();
                }
            }
            recorded_requests
        });

        let settings = AppSettings::default();
        let payload = serde_json::json!({
            "live_context": {
                "version": 1,
                "network": 0,
                "shard_id": 0,
                "voucher_id": "v1",
                "lookup_tag": "tag1",
                "parent_lock": parent_lock,
                "child_lock": vec![1u8; 32],
                "root_valid_until": 0,
                "state_digest": vec![0u8; 32],
                "recipient_nonce": vec![1u8; 32],
                "committee": vec![vec![0u8; 32]; 20],
            }
        });
        let req_bytes = serde_json::to_vec(&payload).unwrap();

        let status = dispatch_exact(&settings, &[server_url], &req_bytes)
            .await
            .unwrap();
        assert_eq!(status, Some(200));

        let requests = server_task.await.unwrap();
        assert_eq!(requests.len(), 2);

        // Attempt 0 should NOT have PoW headers
        assert!(!requests[0].to_lowercase().contains("x-pow-challenge:"));
        assert!(!requests[0].to_lowercase().contains("x-pow-nonce:"));

        // Attempt 1 MUST have X-PoW-Challenge and X-PoW-Nonce satisfying stateless work
        let req1_lower = requests[1].to_lowercase();
        assert!(req1_lower.contains("x-pow-challenge:"));
        assert!(req1_lower.contains("x-pow-nonce:"));

        let nonce_str = requests[1]
            .lines()
            .find_map(|line| {
                let lower = line.to_lowercase();
                if lower.starts_with("x-pow-nonce:") {
                    Some(line["x-pow-nonce:".len()..].trim().to_string())
                } else {
                    None
                }
            })
            .expect("must have X-PoW-Nonce header");

        let nonce: u64 = nonce_str.parse().expect("valid u64 nonce");
        let work = compute_stateless_pow_work(&parent_lock, slot, nonce);
        assert!(check_pow_difficulty(&work, difficulty));
    }
}

/// Bounded public read: no bearer or friend token is attached to reconciliation.
pub(crate) async fn fetch_retained(
    candidates: &[String],
    tag: &str,
) -> Result<Option<Vec<u8>>, String> {
    if tag.is_empty() || tag.len() > 128 {
        return Err("Invalid retained lookup target".into());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .map_err(|_| "HTTP client unavailable".to_string())?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(800);
    for base in candidates.iter().take(4) {
        let Some(mut url) = endpoint(base, "v1/retained-choice") else {
            continue;
        };
        url.path_segments_mut()
            .map_err(|_| "Invalid gateway path".to_string())?
            .push(tag);
        let read = async {
            let mut response = client
                .get(url)
                .send()
                .await
                .map_err(|_| "Retained read unavailable".to_string())?;
            if !response.status().is_success() {
                return Ok::<Option<Vec<u8>>, String>(None);
            }
            if response.content_length().is_some_and(|size| size > 65_536) {
                return Err("Retained response too large".into());
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "Retained read interrupted".to_string())?
            {
                if bytes.len().saturating_add(chunk.len()) > 65_536 {
                    return Err("Retained response too large".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(Some(bytes))
        };
        if let Ok(Ok(Some(bytes))) = tokio::time::timeout_at(deadline, read).await {
            return Ok(Some(bytes));
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }
    Ok(None)
}

/// Cold-path discovery supplies routing candidates only, never committee trust.
/// This synchronous selection is the only resolver used inside the send budget.
pub(crate) fn cached_candidates(settings: &AppSettings) -> Vec<String> {
    let mut candidates = Vec::new();
    for server in settings.custom_servers.iter().filter(|s| s.is_active) {
        if endpoint(&server.url, "").is_some() && !candidates.contains(&server.url) {
            candidates.push(server.url.clone());
        }
        if candidates.len() == 32 { return candidates; }
    }
    if !settings.exclusive_custom_servers {
        let cached = if settings.active_network.eq_ignore_ascii_case("testnet") { &settings.cached_peers_testnet } else { &settings.cached_peers_mainnet };
        for base in cached.iter().chain(settings.l2_node_urls.iter().flatten()) {
            if endpoint(base, "").is_some() && !candidates.contains(base) { candidates.push(base.clone()); }
            if candidates.len() == 32 { break; }
        }
    }
    candidates
}

/// Public directory reads carry no provider credentials and follow no redirects.
/// Bound bytes and a single deadline include the whole response stream.
pub(crate) async fn discover_gateways(candidates: &[String]) -> Vec<String> {
    let Ok(client) = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(2)).build() else { return Vec::new(); };
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut found = Vec::new();
    for base in candidates.iter().take(4) {
        let Some(url) = endpoint(base, "v1/gateways") else { continue; };
        let read = async {
            let mut response = client.get(url).send().await.ok()?;
            if !response.status().is_success() || response.content_length().is_some_and(|n| n > 65_536) { return None; }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.ok()? {
                if bytes.len().saturating_add(chunk.len()) > 65_536 { return None; }
                bytes.extend_from_slice(&chunk);
            }
            let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            Some(value.get("gateways")?.as_array()?.iter().take(32).filter_map(|v| v.as_str().map(str::to_owned)).collect::<Vec<_>>())
        };
        let Ok(Some(urls)) = tokio::time::timeout_at(deadline, read).await else { continue; };
        for url in urls {
            if url.len() <= 2048 && endpoint(&url, "").is_some() && !found.contains(&url) { found.push(url); }
            if found.len() == 32 { return found; }
        }
    }
    found
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn public_directory_is_bounded_and_never_carries_provider_credentials() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let size = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..size]);
            assert!(request.starts_with("GET /v1/gateways "));
            assert!(!request.to_lowercase().contains("authorization:"));
            assert!(!request.to_lowercase().contains("x-peer-token:"));
            let body = r#"{"gateways":["https://one.example","https://one.example","https://user:secret@two.example","file:///tmp/private","https://three.example"]}"#;
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
        assert_eq!(discover_gateways(&[url]).await, vec!["https://one.example", "https://three.example"]);
        task.await.unwrap();
    }
    #[tokio::test]
    async fn redirects_never_forward_configured_provider_secrets() {
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_url = format!("http://{}", target.local_addr().unwrap());
        let first = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let first_url = format!("http://{}", first.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = first.accept().await.unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).await.unwrap();
            stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: {target_url}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        });
        let mut settings = AppSettings::default();
        settings.custom_servers.push(crate::settings::CustomServerDto { url:first_url.clone(), auth_token:Some("provider-secret".into()), peer_token:Some("f2f_0123456789abcdef0123456789abcdef".into()), ..Default::default() });
        assert_eq!(dispatch_exact(&settings, &[first_url], b"{}").await.unwrap(), Some(302));
        assert!(tokio::time::timeout(std::time::Duration::from_millis(100), target.accept()).await.is_err());
        task.await.unwrap();
    }
    #[test]
    fn cached_selection_is_local_network_specific_bounded_and_respects_exclusive_mode() {
        let mut settings = AppSettings {
            active_network: "testnet".into(),
            cached_peers_testnet: vec!["https://test.example".into()],
            cached_peers_mainnet: vec!["https://main.example".into()],
            ..Default::default()
        };
        assert_eq!(cached_candidates(&settings), vec!["https://test.example"]);
        settings.exclusive_custom_servers = true;
        assert!(cached_candidates(&settings).is_empty());
        settings.custom_servers.push(crate::settings::CustomServerDto { url: "https://custom.example".into(), ..Default::default() });
        assert_eq!(cached_candidates(&settings), vec!["https://custom.example"]);
    }
}
