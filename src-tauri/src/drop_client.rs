use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DropIndexEntry {
    pub id: String,
    pub channel: String,
    pub mtime: u64,
    pub size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexFetchResult {
    NotModified,
    Modified {
        entries: Vec<DropIndexEntry>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

pub fn validate_drop_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.starts_with("https://") {
        return Ok(());
    }
    #[cfg(test)]
    if trimmed.starts_with("http://127.0.0.1") || trimmed.starts_with("http://localhost") {
        return Ok(());
    }
    Err(format!("Drop URL must use HTTPS (got '{}')", url))
}

pub fn create_drop_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub async fn put_slot(
    client: &reqwest::Client,
    write_url: &str,
    write_token_hex: &str,
    channel: &str,
    blob_bytes: &[u8],
) -> Result<(), String> {
    validate_drop_url(write_url)?;
    let body_b64 = base64::engine::general_purpose::STANDARD.encode(blob_bytes);

    let resp = client
        .put(write_url)
        .header("X-Drop-Token", write_token_hex)
        .header("X-Channel", channel)
        .header("Content-Type", "text/plain")
        .body(body_b64)
        .send()
        .await
        .map_err(|e| format!("Network error sending slot: {}", e))?;

    if resp.status().as_u16() == 204 {
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} on slot PUT: {}",
            status,
            text.trim()
        ))
    }
}

pub async fn delete_slot(
    client: &reqwest::Client,
    write_url: &str,
    write_token_hex: &str,
    channel: &str,
) -> Result<(), String> {
    validate_drop_url(write_url)?;

    let resp = client
        .delete(write_url)
        .header("X-Drop-Token", write_token_hex)
        .header("X-Channel", channel)
        .send()
        .await
        .map_err(|e| format!("Network error deleting slot: {}", e))?;

    if resp.status().as_u16() == 204 {
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} on slot DELETE: {}",
            status,
            text.trim()
        ))
    }
}

pub async fn list_index(
    client: &reqwest::Client,
    read_url: &str,
    cached_etag: Option<&str>,
    cached_last_modified: Option<&str>,
) -> Result<IndexFetchResult, String> {
    validate_drop_url(read_url)?;
    let base = read_url.trim().trim_end_matches('/');
    let url = format!("{}/index.json", base);

    let mut req = client.get(&url);
    if let Some(etag) = cached_etag {
        req = req.header("If-None-Match", etag);
    }
    if let Some(lm) = cached_last_modified {
        req = req.header("If-Modified-Since", lm);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error fetching index: {}", e))?;

    let status = resp.status();
    if status.as_u16() == 304 {
        return Ok(IndexFetchResult::NotModified);
    }
    if status.as_u16() == 404 {
        return Ok(IndexFetchResult::Modified {
            entries: Vec::new(),
            etag: None,
            last_modified: None,
        });
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Server returned HTTP {} fetching index: {}",
            status,
            text.trim()
        ));
    }

    let new_etag = resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let new_last_modified = resp
        .headers()
        .get("last-modified")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let entries: Vec<DropIndexEntry> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse index.json: {}", e))?;

    Ok(IndexFetchResult::Modified {
        entries,
        etag: new_etag,
        last_modified: new_last_modified,
    })
}

pub async fn get_slot(
    client: &reqwest::Client,
    read_url: &str,
    id: &str,
    channel: &str,
) -> Result<Vec<u8>, String> {
    validate_drop_url(read_url)?;
    let base = read_url.trim().trim_end_matches('/');
    let url = format!("{}/{}.{}.bin", base, id, channel);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching slot: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Server returned HTTP {} fetching slot {}",
            resp.status(),
            id
        ));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read slot bytes: {}", e))?;

    Ok(bytes.to_vec())
}

pub async fn get_members(
    client: &reqwest::Client,
    read_url: &str,
    channel: &str,
) -> Result<Vec<String>, String> {
    validate_drop_url(read_url)?;
    let base = read_url.trim().trim_end_matches('/');
    let url = format!("{}/members.{}.json", base, channel);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching members: {}", e))?;

    if resp.status().as_u16() == 404 {
        return Ok(Vec::new());
    }

    if !resp.status().is_success() {
        return Err(format!(
            "Server returned HTTP {} fetching members",
            resp.status()
        ));
    }

    #[derive(Deserialize)]
    struct MembersPayload {
        ids: Vec<String>,
    }

    let payload: MembersPayload = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse members JSON: {}", e))?;

    Ok(payload.ids)
}

pub async fn push_members(
    client: &reqwest::Client,
    write_url: &str,
    admin_token_hex: &str,
    channel: &str,
    ids: &[String],
) -> Result<(), String> {
    validate_drop_url(write_url)?;
    let sep = if write_url.contains('?') { "&" } else { "?" };
    let url = format!("{}{}members&channel={}", write_url, sep, channel);

    #[derive(Serialize)]
    struct PushMembersPayload<'a> {
        ids: &'a [String],
    }

    let body_json = serde_json::to_string(&PushMembersPayload { ids })
        .map_err(|e| format!("Failed to serialize members payload: {}", e))?;

    let resp = client
        .put(&url)
        .header("X-Admin-Token", admin_token_hex)
        .header("Content-Type", "application/json")
        .body(body_json)
        .send()
        .await
        .map_err(|e| format!("Network error pushing members: {}", e))?;

    if resp.status().as_u16() == 204 {
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!(
            "Server returned HTTP {} pushing members: {}",
            status,
            text.trim()
        ))
    }
}
