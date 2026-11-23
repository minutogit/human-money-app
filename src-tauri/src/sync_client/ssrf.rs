use super::*;

/// Loopback/localhost sync servers are allowed in unit tests and explicit debug builds only;
/// release builds never honor `HUMOCO_ALLOW_LOOPBACK_SYNC` to avoid env-var SSRF/HTTP downgrades.
pub(super) fn loopback_sync_allowed() -> bool {
    cfg!(test)
        || (cfg!(debug_assertions)
            && std::env::var("HUMOCO_ALLOW_LOOPBACK_SYNC").ok().as_deref() == Some("1"))
}

fn is_blocked_v4(v4: std::net::Ipv4Addr, allow_loopback: bool) -> bool {
    if v4.is_loopback() {
        return !allow_loopback;
    }
    let o = v4.octets();
    v4.is_private()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0b1100_0000) == 64)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        || (o[0] == 192 && o[1] == 0 && o[2] == 2)
        || (o[0] == 192 && o[1] == 88 && o[2] == 99)
        || (o[0] == 198 && (o[1] & 0b1111_1110) == 18)
        || (o[0] == 198 && o[1] == 51 && o[2] == 100)
        || (o[0] == 203 && o[1] == 0 && o[2] == 113)
        || o[0] >= 240
}

pub(crate) fn is_blocked_ip(ip: std::net::IpAddr, allow_loopback: bool) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => is_blocked_v4(v4, allow_loopback),
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_v4(v4, allow_loopback);
            }
            let seg = v6.segments();
            // NAT64 well-known (64:ff9b::/96), IPv4-translated (::ffff:0:0:0/96),
            // and IPv4-compatible IPv6 (::/96 except :: and ::1)
            if (seg[0] == 0x0064 && seg[1] == 0xff9b && seg[2..6] == [0, 0, 0, 0])
                || (seg[0..4] == [0, 0, 0, 0] && seg[4] == 0xffff && seg[5] == 0)
                || (seg[0..6] == [0, 0, 0, 0, 0, 0] && (seg[6] != 0 || seg[7] > 1))
            {
                let embedded_v4 = std::net::Ipv4Addr::new(
                    (seg[6] >> 8) as u8,
                    (seg[6] & 0xff) as u8,
                    (seg[7] >> 8) as u8,
                    (seg[7] & 0xff) as u8,
                );
                return is_blocked_v4(embedded_v4, allow_loopback);
            }
            if v6.is_loopback() {
                !allow_loopback
            } else {
                let is_global_unicast = (seg[0] & 0xe000) == 0x2000;
                if !is_global_unicast {
                    return true;
                }
                // Block non-global / special-purpose sub-ranges inside 2000::/3:
                // 2001::/23 (Teredo, IETF protocol assignments, Benchmarking, ORCHID),
                // 2001:db8::/32 (Documentation), 2002::/16 (6to4), 3fff::/20 (Documentation)
                (seg[0] == 0x2001 && ((seg[1] & 0xfe00) == 0x0000 || seg[1] == 0x0db8))
                    || seg[0] == 0x2002
                    || (seg[0] == 0x3fff && seg[1] < 0x1000)
            }
        }
    }
}

pub(crate) fn filter_resolved_addrs(
    addrs: Vec<std::net::SocketAddr>,
    host: &str,
    allow_loopback: bool,
) -> std::io::Result<Vec<std::net::SocketAddr>> {
    if addrs.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("DNS resolution for '{}' returned no addresses", host),
        ));
    }
    for addr in &addrs {
        if is_blocked_ip(addr.ip(), allow_loopback) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "SSRF guard blocked private/internal IP {} for host '{}'",
                    addr.ip(),
                    host
                ),
            ));
        }
    }
    Ok(addrs)
}

pub(super) struct SsrfSafeDnsResolver {
    allow_loopback: bool,
}

impl SsrfSafeDnsResolver {
    pub(super) fn new(allow_loopback: bool) -> Self {
        Self { allow_loopback }
    }
}

impl reqwest::dns::Resolve for SsrfSafeDnsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        let allow_loopback = self.allow_loopback;
        Box::pin(async move {
            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
                .collect();
            let verified = filter_resolved_addrs(addrs, &host, allow_loopback)
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
            let iter: reqwest::dns::Addrs = Box::new(verified.into_iter());
            Ok(iter)
        })
    }
}

pub fn validate_sync_url(url: &str) -> Result<(), SyncClientError> {
    validate_sync_url_strict(url, loopback_sync_allowed())
}

pub fn validate_sync_read_url(url: &str) -> Result<(), SyncClientError> {
    validate_sync_url(url)?;
    let parsed = reqwest::Url::parse(url.trim()).map_err(|e| {
        SyncClientError::LocalValidation(format!("Invalid Sync read URL '{}': {}", url, e))
    })?;
    if parsed.query().is_some() {
        return Err(SyncClientError::LocalValidation(format!(
            "Sync read URL must not contain a query string: '{}'",
            url
        )));
    }
    Ok(())
}

pub fn validate_sync_url_strict(url: &str, allow_loopback: bool) -> Result<(), SyncClientError> {
    let trimmed = url.trim();
    if trimmed.is_empty()
        || trimmed.len() > 2048
        || trimmed.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(SyncClientError::LocalValidation(format!(
            "Invalid Sync URL '{}'",
            url
        )));
    }
    let parsed = reqwest::Url::parse(trimmed).map_err(|e| {
        SyncClientError::LocalValidation(format!("Invalid Sync URL '{}': {}", url, e))
    })?;
    if !parsed.username().is_empty() || parsed.password().is_some() || parsed.fragment().is_some() {
        return Err(SyncClientError::LocalValidation(format!(
            "Sync URL must not contain credentials or fragment: '{}'",
            url
        )));
    }
    let Some(host) = parsed.host_str() else {
        return Err(SyncClientError::LocalValidation(format!(
            "Sync URL missing host: '{}'",
            url
        )));
    };
    let host_lower = host
        .trim_matches(|c| c == '[' || c == ']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let is_loopback_name = host_lower == "localhost"
        || host_lower.ends_with(".localhost")
        || host_lower == "127.0.0.1"
        || host_lower == "::1";

    if let Ok(ip) = host_lower.parse::<std::net::IpAddr>() {
        if is_blocked_ip(ip, allow_loopback) {
            return Err(SyncClientError::LocalValidation(format!(
                "Sync URL targets blocked internal/private address '{}'",
                url
            )));
        }
    } else if host_lower.ends_with(".local") || host_lower.ends_with(".internal") {
        return Err(SyncClientError::LocalValidation(format!(
            "Sync URL targets internal hostname '{}'",
            url
        )));
    } else if is_loopback_name && !allow_loopback {
        return Err(SyncClientError::LocalValidation(format!(
            "Sync URL loopback is forbidden in production: '{}'",
            url
        )));
    }

    match parsed.scheme() {
        "https" => Ok(()),
        "http" if allow_loopback && is_loopback_name => Ok(()),
        _ => Err(SyncClientError::LocalValidation(format!(
            "Sync URL must use HTTPS (or allowed dev loopback, got '{}')",
            url
        ))),
    }
}

pub fn create_sync_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(std::sync::Arc::new(SsrfSafeDnsResolver::new(
            loopback_sync_allowed(),
        )))
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("failed to build SSRF-safe sync HTTP client")
}

pub fn create_sync_probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(std::sync::Arc::new(SsrfSafeDnsResolver::new(
            loopback_sync_allowed(),
        )))
        .connect_timeout(std::time::Duration::from_secs(4))
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .expect("failed to build SSRF-safe sync probe HTTP client")
}

/// Canonical `(scheme, lowercase host, effective port)` key for per-origin rate-limit cooldowns.
pub fn sync_origin_key(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url.trim()).ok()?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    let port = parsed.port_or_known_default()?;
    Some(format!("{}://{}:{}", scheme, host, port))
}
