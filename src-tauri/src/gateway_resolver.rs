//! Exact-byte HTTP dispatch with provider-scoped credentials. Core owns intent
//! durability; this module owns transport only and cannot authorize release.
use crate::settings::AppSettings;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentDispatchObservation {
    pub local_instance_id: String,
    pub status: String,
    pub http_status: Option<u16>,
}
fn endpoint(raw: &str, path: &str) -> Option<reqwest::Url> {
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
pub(crate) async fn dispatch_exact(
    settings: &AppSettings,
    candidates: &[String],
    bytes: &[u8],
) -> Result<Option<u16>, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .map_err(|_| "HTTP client unavailable".to_string())?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(800);
    for base in candidates.iter().take(4) {
        let Some(url) = endpoint(base, "v1/lock") else {
            continue;
        };
        let mut request = client
            .post(url.clone())
            .header("Content-Type", "application/json")
            .body(bytes.to_vec());
        if let Some(provider) = settings.custom_servers.iter().find(|provider| {
            provider.is_active
                && endpoint(&provider.url, "v1/lock")
                    .is_some_and(|known| known.origin() == url.origin())
        }) {
            if let Some(token) = &provider.auth_token {
                request = request.bearer_auth(token);
            }
            if let Some(token) = &provider.peer_token {
                if token.len() == 36
                    && token.starts_with("f2f_")
                    && token[4..].bytes().all(|b| b.is_ascii_hexdigit())
                {
                    request = request.header("X-Peer-Token", token);
                }
            }
        }
        let result = tokio::time::timeout_at(deadline, request.send()).await;
        match result {
            Ok(Ok(response)) if matches!(response.status().as_u16(), 429 | 500..=599) => continue,
            Ok(Ok(response)) => return Ok(Some(response.status().as_u16())),
            Ok(Err(_)) => continue,
            Err(_) => return Ok(None),
        }
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
        let mut settings = AppSettings::default();
        settings.active_network = "testnet".into();
        settings.cached_peers_testnet = vec!["https://test.example".into()];
        settings.cached_peers_mainnet = vec!["https://main.example".into()];
        assert_eq!(cached_candidates(&settings), vec!["https://test.example"]);
        settings.exclusive_custom_servers = true;
        assert!(cached_candidates(&settings).is_empty());
        settings.custom_servers.push(crate::settings::CustomServerDto { url: "https://custom.example".into(), ..Default::default() });
        assert_eq!(cached_candidates(&settings), vec!["https://custom.example"]);
    }
}
