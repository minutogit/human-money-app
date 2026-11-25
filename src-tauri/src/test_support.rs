use crate::AppState;
use human_money_core::app_service::AppService;
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(crate) static WRITE_FAULT_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(crate) const ALICE_PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
pub(crate) const BOB_PHRASE: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
pub(crate) const CHARLIE_PHRASE: &str =
    "legal winner thank year wave sausage worth useful legal winner thank yellow";
pub(crate) const READER1_PHRASE: &str =
    "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
pub(crate) const READER2_PHRASE: &str =
    "vessel ladder alter error federal sibling chat ability sun glass valve picture";

pub(crate) fn test_ed25519_pub_hex(seed: u8) -> String {
    hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
            .verifying_key()
            .as_bytes(),
    )
}

pub(crate) fn temp_test_dir(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "humoco_sync_test_{}_{}",
        name,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

pub(crate) fn setup_test_app(
    dir: &Path,
    profile_name: &str,
    phrase: &str,
    password: &str,
) -> AppService {
    let mut service = AppService::new(dir).unwrap();
    service
        .create_profile(
            profile_name,
            phrase,
            None,
            Some("pc"),
            password,
            human_money_core::MnemonicLanguage::English,
            "device-drops-test".into(),
            true,
        )
        .unwrap();
    service
}

pub(crate) fn create_test_state(service: AppService) -> AppState {
    AppState::new(service)
}

#[allow(dead_code)]
pub(crate) struct HttpPauseGate {
    pub path_substr: String,
    pub hit_tx: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    pub release_rx: tokio::sync::watch::Receiver<bool>,
}

#[allow(dead_code)]
pub(crate) struct MockSyncServer {
    pub base_url: String,
    pub data_dir: String,
    pub lock_pubkey: String,
    pub lock_signing_key: ed25519_dalek::SigningKey,
    pub admin_pubkey: Arc<Mutex<String>>,
    pub members: Arc<Mutex<Vec<String>>>,
    pub channel_members: Arc<Mutex<HashMap<String, Vec<String>>>>,
    pub slots: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    pub media_owners: Arc<Mutex<HashMap<String, String>>>,
    pub etags: Arc<Mutex<HashMap<String, String>>>,
    pub ttl_days: Arc<Mutex<u32>>,
    pub put_members_count: Arc<Mutex<usize>>,
    pub admin_config: Arc<Mutex<(u64, usize, u64)>>,
    pub pause_gate: Arc<Mutex<Option<Arc<HttpPauseGate>>>>,
    pub _shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

fn strip_etag_quotes(raw: &str) -> &str {
    let trimmed = raw.trim();
    let no_weak = trimmed
        .strip_prefix("W/")
        .or_else(|| trimmed.strip_prefix("w/"))
        .unwrap_or(trimmed);
    no_weak.trim_matches('"')
}

fn authorized_channels_for_pubkey(
    channel_members: &HashMap<String, Vec<String>>,
    members: &[String],
    pubkey: &str,
) -> Vec<&'static str> {
    if channel_members.is_empty() {
        if members.iter().any(|m| m == pubkey) {
            vec!["card", "wot", "backup", "post"]
        } else {
            Vec::new()
        }
    } else {
        ["card", "wot", "backup", "post"]
            .into_iter()
            .filter(|ch| {
                channel_members
                    .get(*ch)
                    .is_some_and(|pks| pks.iter().any(|m| m == pubkey))
            })
            .collect()
    }
}

fn is_pubkey_authorized_for_channel(
    channel_members: &HashMap<String, Vec<String>>,
    members: &[String],
    pubkey: &str,
    channel: &str,
) -> bool {
    if channel_members.is_empty() {
        return members.iter().any(|m| m == pubkey);
    }
    if channel == "media" {
        if let Some(pks) = channel_members.get("media") {
            return pks.iter().any(|m| m == pubkey);
        }
        return channel_members
            .get("card")
            .is_some_and(|pks| pks.iter().any(|m| m == pubkey));
    }
    channel_members
        .get(channel)
        .is_some_and(|pks| pks.iter().any(|m| m == pubkey))
}

impl MockSyncServer {
    /// Arms a one-shot pause before responding to the next request whose path contains `path_substr`.
    #[allow(dead_code)]
    pub fn arm_pause_gate(
        &self,
        path_substr: &str,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::watch::Sender<bool>,
    ) {
        let (hit_tx, hit_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::watch::channel(false);
        *self.pause_gate.lock().unwrap() = Some(Arc::new(HttpPauseGate {
            path_substr: path_substr.to_string(),
            hit_tx: Mutex::new(Some(hit_tx)),
            release_rx,
        }));
        (hit_rx, release_tx)
    }

    pub async fn start(data_dir: String, admin_pubkey: String) -> Self {
        use ed25519_dalek::{Signer, SigningKey};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{}", port);

        let lock_signing_key = SigningKey::from_bytes(&[0x77u8; 32]);
        let lock_pubkey = hex::encode(lock_signing_key.verifying_key().as_bytes());
        let admin_pub = Arc::new(Mutex::new(admin_pubkey));
        let members = Arc::new(Mutex::new(Vec::<String>::new()));
        let channel_members = Arc::new(Mutex::new(HashMap::<String, Vec<String>>::new()));
        let slots = Arc::new(Mutex::new(HashMap::<String, Vec<u8>>::new()));
        let media_owners = Arc::new(Mutex::new(HashMap::<String, String>::new()));
        let etags = Arc::new(Mutex::new(HashMap::<String, String>::new()));
        let ttl_days = Arc::new(Mutex::new(30u32));
        let put_members_count = Arc::new(Mutex::new(0));
        let admin_config = Arc::new(Mutex::new((2048u64, 300usize, 25u64)));
        let pause_gate: Arc<Mutex<Option<Arc<HttpPauseGate>>>> = Arc::new(Mutex::new(None));
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let s_data_dir = data_dir.clone();
        let s_base_url = base_url.clone();
        let s_lock_sk = lock_signing_key.clone();
        let s_lock_pub = lock_pubkey.clone();
        let s_admin_pub = Arc::clone(&admin_pub);
        let s_members = Arc::clone(&members);
        let s_channel_members = Arc::clone(&channel_members);
        let s_slots = Arc::clone(&slots);
        let s_media_owners = Arc::clone(&media_owners);
        let s_etags = Arc::clone(&etags);
        let s_ttl_days = Arc::clone(&ttl_days);
        let s_put_members = Arc::clone(&put_members_count);
        let s_admin_config = Arc::clone(&admin_config);
        let s_pause_gate = Arc::clone(&pause_gate);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    Ok((mut stream, _)) = listener.accept() => {
                        let data_dir = s_data_dir.clone();
                        let base_url = s_base_url.clone();
                        let lock_sk = s_lock_sk.clone();
                        let lock_pub_hex = s_lock_pub.clone();
                        let admin_pub = Arc::clone(&s_admin_pub);
                        let members = Arc::clone(&s_members);
                        let channel_members = Arc::clone(&s_channel_members);
                        let slots = Arc::clone(&s_slots);
                        let media_owners = Arc::clone(&s_media_owners);
                        let etags = Arc::clone(&s_etags);
                        let ttl_days_arc = Arc::clone(&s_ttl_days);
                        let put_members = Arc::clone(&s_put_members);
                        let admin_config = Arc::clone(&s_admin_config);
                        let pause_gate = Arc::clone(&s_pause_gate);

                        tokio::spawn(async move {
                            use tokio::io::{AsyncReadExt, AsyncWriteExt};
                            let mut buf = vec![0u8; 1_048_576];
                            let mut read_bytes = 0;

                            while read_bytes < buf.len() {
                                let n = match stream.read(&mut buf[read_bytes..]).await {
                                    Ok(0) => break,
                                    Ok(n) => n,
                                    Err(_) => break,
                                };
                                read_bytes += n;
                                if buf[..read_bytes].windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }

                            if read_bytes == 0 {
                                return;
                            }

                            let header_end = buf[..read_bytes]
                                .windows(4)
                                .position(|w| w == b"\r\n\r\n")
                                .unwrap_or(read_bytes);
                            let header_str = String::from_utf8_lossy(&buf[..header_end]);
                            let mut lines = header_str.lines();
                            let req_line = lines.next().unwrap_or("");
                            let mut parts = req_line.split_whitespace();
                            let method = parts.next().unwrap_or("");
                            let path = parts.next().unwrap_or("");

                            let matched_gate = {
                                let mut guard = pause_gate.lock().unwrap();
                                if guard
                                    .as_ref()
                                    .is_some_and(|g| path.contains(&g.path_substr))
                                {
                                    guard.take()
                                } else {
                                    None
                                }
                            };
                            if let Some(gate) = matched_gate {
                                if let Some(tx) = gate.hit_tx.lock().unwrap().take() {
                                    let _ = tx.send(());
                                }
                                let mut rx = gate.release_rx.clone();
                                while !*rx.borrow() {
                                    if rx.changed().await.is_err() {
                                        break;
                                    }
                                }
                            }

                            let mut headers = HashMap::new();
                            for l in lines {
                                if let Some((k, v)) = l.split_once(':') {
                                    headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                                }
                            }

                            let content_len: usize = headers
                                .get("content-length")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                            let body_start = header_end + 4;
                            let mut body = buf[body_start..read_bytes].to_vec();

                            while body.len() < content_len {
                                let mut chunk = vec![0u8; content_len - body.len()];
                                match stream.read(&mut chunk).await {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        body.extend_from_slice(&chunk[..n]);
                                    }
                                    Err(_) => break,
                                }
                            }

                            let response: Vec<u8> = if method == "GET" && path.contains("info") {
                                let cur_admin = admin_pub.lock().unwrap().clone();
                                let info_json = serde_json::json!({
                                    "drop_pub": cur_admin,
                                    "read_url": format!("{}/{}", base_url, data_dir),
                                    "register_url": null,
                                    "channels": ["card", "wot", "backup", "post"],
                                    "lock": {
                                        "lock_pub": lock_pub_hex,
                                        "delegation": null,
                                        "max_retention_quarter": 8159,
                                        "max_inodes": 50000,
                                        "used_inodes": 0,
                                        "inode_warning": false,
                                        "pow_base_bits": 8,
                                        "allow_pow_tickets": false,
                                        "ticket_pow_bits": 16
                                    }
                                });
                                let body_str = serde_json::to_string(&info_json).unwrap();
                                format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                    body_str.len(), body_str
                                ).into_bytes()
                            } else if method == "HEAD" && path.starts_with("/sync.php") {
                                let req_pub = headers.get("x-drop-pubkey").cloned().unwrap_or_default();
                                let ch = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                let prefix_opt = headers.get("x-drop-prefix").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                                let key = match prefix_opt {
                                    Some(ref pfx) => format!("{}_{}.{}", req_pub, pfx, ch),
                                    None => format!("{}.{}", req_pub, ch),
                                };
                                let auth_chs = {
                                    let cm = channel_members.lock().unwrap();
                                    let m = members.lock().unwrap();
                                    authorized_channels_for_pubkey(&cm, &m, &req_pub)
                                };
                                let is_auth = !auth_chs.is_empty();
                                let occupied = slots.lock().unwrap().contains_key(&key);
                                let etag_hdr = if is_auth && occupied {
                                    etags
                                        .lock()
                                        .unwrap()
                                        .get(&key)
                                        .map(|e| format!("ETag: {}\r\n", e))
                                        .unwrap_or_default()
                                } else {
                                    String::new()
                                };
                                let cur_ttl = *ttl_days_arc.lock().unwrap();
                                let ttl_hdr = if cur_ttl > 0 {
                                    format!("X-Channel-Ttl-Days: {}\r\n", cur_ttl)
                                } else {
                                    String::new()
                                };
                                format!(
                                    "HTTP/1.1 200 OK\r\nCache-Control: no-store\r\nX-Drop-Authorized: {}\r\nX-Drop-Channels: {}\r\nX-Channel-Max-Bytes: 524288\r\n{}X-Slot-Occupied: {}\r\n{}Content-Length: 0\r\n\r\n",
                                    if is_auth { "true" } else { "false" },
                                    auth_chs.join(","),
                                    ttl_hdr,
                                    if occupied { "true" } else { "false" },
                                    etag_hdr
                                ).into_bytes()
                            } else if method == "PUT" && path.starts_with("/sync.php") && path.contains("members") {
                                *put_members.lock().unwrap() += 1;
                                #[derive(serde::Deserialize)]
                                struct MembersJson {
                                    pubkeys: Vec<String>,
                                }
                                if let Ok(mj) = serde_json::from_slice::<MembersJson>(&body) {
                                    let ch = if let Some((_, rest)) = path.split_once("channel=") {
                                        rest.split('&').next().unwrap_or("card")
                                    } else {
                                        "card"
                                    };
                                    channel_members.lock().unwrap().insert(ch.to_string(), mj.pubkeys);
                                    let cm_guard = channel_members.lock().unwrap();
                                    let mut m = members.lock().unwrap();
                                    let mut union_members = HashSet::new();
                                    for pks in cm_guard.values() {
                                        for pk in pks {
                                            union_members.insert(pk.clone());
                                        }
                                    }
                                    *m = union_members.into_iter().collect();
                                    let mut s = slots.lock().unwrap();
                                    let mut mo = media_owners.lock().unwrap();
                                    s.retain(|k, _| {
                                        if k.ends_with(".media") {
                                            if let Some(owner_pk) = mo.get(k) {
                                                return cm_guard.get("media").or_else(|| cm_guard.get("card")).map_or_else(
                                                    || m.iter().any(|id| id == owner_pk),
                                                    |pks| pks.iter().any(|id| id == owner_pk),
                                                );
                                            }
                                            if let Some(base_pk) = k.strip_suffix(".media") {
                                                return cm_guard.get("media").or_else(|| cm_guard.get("card")).map_or_else(
                                                    || m.iter().any(|id| id == base_pk),
                                                    |pks| pks.iter().any(|id| id == base_pk),
                                                );
                                            }
                                            return false;
                                        }
                                        if let Some((id_part, slot_ch)) = k.split_once('.') {
                                            let base_pk = id_part.split_once('_').map(|(pk, _)| pk).unwrap_or(id_part);
                                            cm_guard.get(slot_ch).map_or_else(
                                                || m.iter().any(|id| id == base_pk),
                                                |pks| pks.iter().any(|id| id == base_pk),
                                            )
                                        } else {
                                            false
                                        }
                                    });
                                    mo.retain(|k, _| s.contains_key(k));
                                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n".to_vec()
                                } else {
                                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_vec()
                                }
                            } else if method == "GET" && path.starts_with("/sync.php") && path.contains("status") {
                                let cur_admin = admin_pub.lock().unwrap().clone();
                                let s_map = slots.lock().unwrap();
                                let (max_total_mb, max_mem, max_user_mb) = *admin_config.lock().unwrap();
                                let max_bytes = max_total_mb * 1024 * 1024;
                                let status_json = serde_json::json!({
                                    "used_bytes": s_map.values().map(|v| v.len()).sum::<usize>(),
                                    "max_bytes": max_bytes,
                                    "soft_limit_bytes": max_bytes * 4 / 5,
                                    "soft_warning": false,
                                    "max_members": max_mem,
                                    "max_user_bytes": max_user_mb * 1024 * 1024,
                                    "slot_count": s_map.len(),
                                    "channel_counts": {"card": s_map.len()},
                                    "admin_pubkey": cur_admin
                                });
                                let body_str = serde_json::to_string(&status_json).unwrap();
                                format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                    body_str.len(), body_str
                                ).into_bytes()
                            } else if method == "PUT" && path.starts_with("/sync.php") && path.contains("admin_config") {
                                #[derive(serde::Deserialize)]
                                struct AdminCfgJson {
                                    max_total_storage_mb: u64,
                                    max_members: usize,
                                    max_user_storage_mb: u64,
                                }
                                match serde_json::from_slice::<AdminCfgJson>(&body) {
                                    Ok(cfg_in)
                                        if (1..=100_000).contains(&cfg_in.max_members)
                                            && cfg_in.max_user_storage_mb >= 10
                                            && cfg_in.max_total_storage_mb >= cfg_in.max_user_storage_mb =>
                                    {
                                        *admin_config.lock().unwrap() = (
                                            cfg_in.max_total_storage_mb,
                                            cfg_in.max_members,
                                            cfg_in.max_user_storage_mb,
                                        );
                                        let cur_admin = admin_pub.lock().unwrap().clone();
                                        let s_map = slots.lock().unwrap();
                                        let max_bytes = cfg_in.max_total_storage_mb * 1024 * 1024;
                                        let status_json = serde_json::json!({
                                            "used_bytes": s_map.values().map(|v| v.len()).sum::<usize>(),
                                            "max_bytes": max_bytes,
                                            "soft_limit_bytes": max_bytes * 4 / 5,
                                            "soft_warning": false,
                                            "max_members": cfg_in.max_members,
                                            "max_user_bytes": cfg_in.max_user_storage_mb * 1024 * 1024,
                                            "slot_count": s_map.len(),
                                            "channel_counts": {"card": s_map.len()},
                                            "admin_pubkey": cur_admin
                                        });
                                        let body_str = serde_json::to_string(&status_json).unwrap();
                                        format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                            body_str.len(), body_str
                                        ).into_bytes()
                                    }
                                    _ => b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_vec(),
                                }
                            } else if method == "PUT" && path.starts_with("/sync.php") {
                                let pubkey = headers.get("x-drop-pubkey").cloned().unwrap_or_default();
                                let channel = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                let prefix_opt = headers.get("x-drop-prefix").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                                let media_id_opt = headers.get("x-media-id").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                                let is_member = {
                                    let cm = channel_members.lock().unwrap();
                                    let m = members.lock().unwrap();
                                    is_pubkey_authorized_for_channel(&cm, &m, &pubkey, &channel)
                                };
                                if is_member {
                                    let key = if channel == "media" {
                                        match media_id_opt {
                                            Some(ref mid) => format!("{}.media", mid),
                                            None => format!("{}.media", pubkey),
                                        }
                                    } else {
                                        match prefix_opt {
                                            Some(ref pfx) => format!("{}_{}.{}", pubkey, pfx, channel),
                                            None => format!("{}.{}", pubkey, channel),
                                        }
                                    };
                                    let if_match_hdr = headers.get("if-match").cloned();
                                    let slot_exists = slots.lock().unwrap().contains_key(&key);
                                    let cur_etag_opt = etags.lock().unwrap().get(&key).cloned();
                                    let precondition_failed = if let Some(ref im) = if_match_hdr {
                                        let im_trim = strip_etag_quotes(im);
                                        if im_trim == "none" || im_trim.is_empty() {
                                            slot_exists
                                        } else {
                                            match cur_etag_opt.as_deref() {
                                                Some(cur) => strip_etag_quotes(cur) != im_trim,
                                                None => true,
                                            }
                                        }
                                    } else {
                                        false
                                    };
                                    if precondition_failed {
                                        let hdr_etag = cur_etag_opt.unwrap_or_else(|| "\"\"".to_string());
                                        format!(
                                            "HTTP/1.1 412 Precondition Failed\r\nETag: {}\r\nContent-Length: 0\r\n\r\n",
                                            hdr_etag
                                        ).into_bytes()
                                    } else {
                                        let new_len = body.len();
                                        let etag_hex = hex::encode(sha2::Sha256::digest(&body));
                                        let new_etag = format!("\"{}\"", etag_hex);
                                        slots.lock().unwrap().insert(key.clone(), body);
                                        if channel == "media" {
                                            media_owners.lock().unwrap().insert(key.clone(), pubkey.clone());
                                        }
                                        etags.lock().unwrap().insert(key, new_etag.clone());
                                        if channel == "media" {
                                            format!(
                                                "HTTP/1.1 200 OK\r\nETag: {}\r\nContent-Length: 0\r\n\r\n",
                                                new_etag
                                            ).into_bytes()
                                        } else {
                                            let ts_now = crate::sync_client::current_unix_timestamp();
                                            let pfx_str = prefix_opt.as_deref().unwrap_or("");
                                            let ack_msg = format!(
                                                "HUMOCO-SLOT-ACK-V1\n{}\n{}\n{}\n{}\n{}\n{}",
                                                lock_pub_hex, pubkey, pfx_str, channel, etag_hex, ts_now
                                            );
                                            let ack_sig = hex::encode(lock_sk.sign(ack_msg.as_bytes()).to_bytes());
                                            let resp_json = serde_json::json!({
                                                "status": "ok",
                                                "channel": channel,
                                                "bytes": new_len,
                                                "etag": etag_hex,
                                                "slot_ack": {
                                                    "lock_pub": lock_pub_hex,
                                                    "pubkey": pubkey,
                                                    "prefix": prefix_opt,
                                                    "channel": channel,
                                                    "etag": etag_hex,
                                                    "timestamp": ts_now,
                                                    "sig": ack_sig,
                                                }
                                            })
                                            .to_string();
                                            format!(
                                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: {}\r\nContent-Length: {}\r\n\r\n{}",
                                                new_etag, resp_json.len(), resp_json
                                            ).into_bytes()
                                        }
                                    }
                                } else {
                                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".to_vec()
                                }
                            } else if method == "DELETE" && path.starts_with("/sync.php") {
                                let pubkey = headers.get("x-drop-pubkey").cloned().unwrap_or_default();
                                if path.contains("account") {
                                    let dot_prefix = format!("{}.", pubkey);
                                    let under_prefix = format!("{}_", pubkey);
                                    let owned_media: HashSet<String> = {
                                        let mut mo = media_owners.lock().unwrap();
                                        let owned: HashSet<String> = mo
                                            .iter()
                                            .filter(|(_, owner)| owner.as_str() == pubkey.as_str())
                                            .map(|(k, _)| k.clone())
                                            .collect();
                                        mo.retain(|k, _| !owned.contains(k));
                                        owned
                                    };
                                    slots.lock().unwrap().retain(|k, _| {
                                        !k.starts_with(&dot_prefix)
                                            && !k.starts_with(&under_prefix)
                                            && !owned_media.contains(k)
                                    });
                                    etags.lock().unwrap().retain(|k, _| {
                                        !k.starts_with(&dot_prefix)
                                            && !k.starts_with(&under_prefix)
                                            && !owned_media.contains(k)
                                    });
                                } else {
                                    let channel = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                    let prefix_opt = headers.get("x-drop-prefix").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                                    let media_id_opt = headers.get("x-media-id").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                                    if channel == "media" && media_id_opt.is_none() {
                                        let media_dot = format!("{}.media", pubkey);
                                        let owned_media: HashSet<String> = {
                                            let mut mo = media_owners.lock().unwrap();
                                            let owned: HashSet<String> = mo
                                                .iter()
                                                .filter(|(_, owner)| owner.as_str() == pubkey.as_str())
                                                .map(|(k, _)| k.clone())
                                                .collect();
                                            mo.retain(|k, _| !owned.contains(k));
                                            owned
                                        };
                                        slots.lock().unwrap().retain(|k, _| k != &media_dot && !owned_media.contains(k));
                                        etags.lock().unwrap().retain(|k, _| k != &media_dot && !owned_media.contains(k));
                                    } else if channel == "media" {
                                        let key = format!("{}.media", media_id_opt.unwrap());
                                        let is_owner = media_owners
                                            .lock()
                                            .unwrap()
                                            .get(&key)
                                            .is_none_or(|o| o == &pubkey);
                                        if is_owner {
                                            media_owners.lock().unwrap().remove(&key);
                                            slots.lock().unwrap().remove(&key);
                                            etags.lock().unwrap().remove(&key);
                                        }
                                    } else {
                                        let key = match prefix_opt {
                                            Some(ref pfx) => format!("{}_{}.{}", pubkey, pfx, channel),
                                            None => format!("{}.{}", pubkey, channel),
                                        };
                                        slots.lock().unwrap().remove(&key);
                                        etags.lock().unwrap().remove(&key);
                                    }
                                }
                                b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n".to_vec()
                            } else if method == "GET" && path.contains("index.json") {
                                let s_map = slots.lock().unwrap();
                                let e_map = etags.lock().unwrap();
                                let mut entries = Vec::new();
                                for (k, v) in s_map.iter() {
                                    if let Some((id_part, ch)) = k.split_once('.') {
                                        if ch == "media" {
                                            continue;
                                        }
                                        let (id, prefix) = match id_part.split_once('_') {
                                            Some((pk, pfx)) => (pk, Some(pfx)),
                                            None => (id_part, None),
                                        };
                                        let slot_etag = e_map
                                            .get(k)
                                            .map(|e| e.trim_matches('"').to_string())
                                            .unwrap_or_else(|| hex::encode(&sha2::Sha256::digest(v)[..16]));
                                        let mut obj = serde_json::json!({
                                            "id": id,
                                            "channel": ch,
                                            "mtime": 1000,
                                            "size": v.len(),
                                            "etag": slot_etag
                                        });
                                        if let Some(pfx) = prefix {
                                            obj["prefix"] = serde_json::Value::String(pfx.to_string());
                                        }
                                        entries.push(obj);
                                    }
                                }
                                let body_str = serde_json::to_string(&entries).unwrap();
                                let idx_hash = hex::encode(&sha2::Sha256::digest(body_str.as_bytes())[..16]);
                                let idx_etag = format!("\"{}\"", idx_hash);
                                let not_modified = headers
                                    .get("if-none-match")
                                    .is_some_and(|inm| strip_etag_quotes(inm) == idx_hash || inm.trim() == "*");
                                if not_modified {
                                    format!("HTTP/1.1 304 Not Modified\r\nETag: {}\r\nContent-Length: 0\r\n\r\n", idx_etag).into_bytes()
                                } else {
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: {}\r\nContent-Length: {}\r\n\r\n{}",
                                        idx_etag, body_str.len(), body_str
                                    ).into_bytes()
                                }
                            } else if method == "GET" && path.ends_with(".bin") {
                                let fname = path.rsplit('/').next().unwrap_or("");
                                let slot_key = fname.strip_suffix(".bin").unwrap_or(fname);
                                let s_map = slots.lock().unwrap();
                                if let Some(data) = s_map.get(slot_key) {
                                    let cur_etag = etags
                                        .lock()
                                        .unwrap()
                                        .get(slot_key)
                                        .cloned()
                                        .unwrap_or_else(|| format!("\"{}\"", hex::encode(&sha2::Sha256::digest(data)[..16])));
                                    let clean_etag = strip_etag_quotes(&cur_etag);
                                    let not_modified = headers
                                        .get("if-none-match")
                                        .is_some_and(|inm| strip_etag_quotes(inm) == clean_etag || inm.trim() == "*");
                                    if not_modified {
                                        format!("HTTP/1.1 304 Not Modified\r\nETag: {}\r\nContent-Length: 0\r\n\r\n", cur_etag).into_bytes()
                                    } else {
                                        let mut resp = format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nETag: {}\r\nContent-Length: {}\r\n\r\n",
                                            cur_etag, data.len()
                                        ).into_bytes();
                                        resp.extend_from_slice(data);
                                        resp
                                    }
                                } else {
                                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
                                }
                            } else {
                                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
                            };

                            if let Some(crlf_pos) = response.windows(2).position(|w| w == b"\r\n") {
                                let insert_at = crlf_pos + 2;
                                let mut framed = Vec::with_capacity(response.len() + 19);
                                framed.extend_from_slice(&response[..insert_at]);
                                framed.extend_from_slice(b"Connection: close\r\n");
                                framed.extend_from_slice(&response[insert_at..]);
                                let _ = stream.write_all(&framed).await;
                            } else {
                                let _ = stream.write_all(&response).await;
                            }
                            let _ = stream.shutdown().await;
                        });
                    }
                }
            }
        });

        Self {
            base_url,
            data_dir,
            lock_pubkey,
            lock_signing_key,
            admin_pubkey: admin_pub,
            members,
            channel_members,
            slots,
            media_owners,
            etags,
            ttl_days,
            put_members_count,
            admin_config,
            pause_gate,
            _shutdown_tx: Some(shutdown_tx),
        }
    }
}

#[cfg(test)]
mod s11_tests {
    use super::MockSyncServer;

    #[tokio::test]
    async fn s11_f8_mock_sync_server_media_roundtrip_channel_auth_and_weak_etag() {
        let admin_pub = "11".repeat(32);
        let alice_pub = "22".repeat(32);
        let server = MockSyncServer::start("data".to_string(), admin_pub).await;
        server
            .channel_members
            .lock()
            .unwrap()
            .insert("card".to_string(), vec![alice_pub.clone()]);

        let client = crate::sync_client::create_sync_http_client();
        let write_url = format!("{}/sync.php", server.base_url);
        let read_url = format!("{}/{}", server.base_url, server.data_dir);
        let signed = humoco_market::SignedDropHeaders {
            pubkey_hex: alice_pub.clone(),
            prefix: None,
            timestamp: 1_700_000_000,
            nonce_hex: "aa".repeat(16),
            signature_hex: "bb".repeat(64),
        };

        // 1. Media roundtrip works under <blob_id>.media
        let blob_id = "cc".repeat(32);
        let payload = vec![0x42u8; 128];
        crate::sync_client::put_media_blob(&client, &write_url, &signed, &blob_id, &payload)
            .await
            .unwrap();
        let fetched_media = crate::sync_client::get_media_blob(&client, &read_url, &blob_id)
            .await
            .unwrap();
        assert_eq!(fetched_media, Some(payload));

        // 2. Per-channel enforcement on HEAD and PUT
        let head_resp = client
            .head(&write_url)
            .header("X-Drop-Pubkey", &alice_pub)
            .send()
            .await
            .unwrap();
        assert_eq!(
            head_resp
                .headers()
                .get("x-drop-channels")
                .and_then(|v| v.to_str().ok()),
            Some("card")
        );

        match crate::sync_client::put_slot(&client, &write_url, &signed, "post", b"no", None).await
        {
            Err(crate::sync_client::SyncClientError::HttpStatus { status, .. }) => {
                assert_eq!(status, 403, "Alice is not in channel_members['post']");
            }
            other => panic!("expected HttpStatus 403 for unauthorized post channel, got {other:?}"),
        }
        crate::sync_client::put_slot(
            &client,
            &write_url,
            &signed,
            "card",
            b"card-slot-bytes",
            None,
        )
        .await
        .unwrap();

        // 3. Weak W/ ETags trigger 304 Not Modified on index.json and .bin
        let idx_url = format!("{}/index.json", read_url);
        let idx_resp = client.get(&idx_url).send().await.unwrap();
        let idx_etag = idx_resp
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let idx_304 = client
            .get(&idx_url)
            .header("If-None-Match", format!("W/{}", idx_etag))
            .send()
            .await
            .unwrap();
        assert_eq!(idx_304.status().as_u16(), 304);

        let slot_url = format!("{}/{}.card.bin", read_url, alice_pub);
        let slot_resp = client.get(&slot_url).send().await.unwrap();
        let slot_etag = slot_resp
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let slot_304 = client
            .get(&slot_url)
            .header("If-None-Match", format!("W/{}", slot_etag))
            .send()
            .await
            .unwrap();
        assert_eq!(slot_304.status().as_u16(), 304);
    }
}
