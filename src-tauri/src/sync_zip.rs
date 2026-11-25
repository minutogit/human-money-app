// src-tauri/src/sync_zip.rs
use crc32fast::Hasher;

const SYNC_PHP: &[u8] = include_bytes!("../../../humoco-sync/php/sync.php");
const LOCK_HANDLER_PHP: &[u8] = include_bytes!("../../../humoco-sync/php/lock_handler.php");
const LOCK_ROTATION_PHP: &[u8] = include_bytes!("../../../humoco-sync/php/lock_rotation.php");
const HTACCESS: &[u8] = include_bytes!("../../../humoco-sync/php/.htaccess");
const ROBOTS_TXT: &[u8] = include_bytes!("../../../humoco-sync/php/robots.txt");
const INDEX_HTML: &[u8] = include_bytes!("../../../humoco-sync/php/index.html");
const README_MD: &[u8] = include_bytes!("../../../humoco-sync/php/README.md");

fn escape_php_single_quoted(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_control() {
            continue;
        }
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            c => out.push(c),
        }
    }
    out
}

pub fn generate_sync_config_php(
    admin_pubkey: &str,
    data_dir: &str,
    max_members: usize,
    max_total_storage_mb: usize,
    max_user_storage_mb: usize,
    register_url: Option<&str>,
) -> String {
    let esc_admin = escape_php_single_quoted(admin_pubkey);
    let esc_dir = escape_php_single_quoted(data_dir);
    let register_url_val = match register_url.map(str::trim).filter(|u| !u.is_empty()) {
        Some(url) => format!("'{}'", escape_php_single_quoted(url)),
        None => "null".to_string(),
    };
    let max_media_user_bytes: u64 =
        6_291_456u64.min((max_user_storage_mb as u64).saturating_mul(1024 * 1024));
    format!(
        r#"<?php // config.php – adjust to your needs
return [
    // 64-hex lowercase Ed25519 VerifyingKey of the sync admin
    'admin_pubkey'         => '{}',

    // Deterministic directory name, part of read_url
    'data_dir'             => '{}',

    // Optional URL where new users can register on public sync services
    'register_url'         => {},

    // Maximum number of member public keys per channel
    'max_members'          => {},

    // Total storage quota in megabytes across all channels
    'max_total_storage_mb' => {},

    // Per-member storage quota in megabytes across all channels (min 10 MB)
    'max_user_storage_mb'  => {},

    // Set to true (or export HUMOCO_DISABLE_RATE_LIMIT=1 in env) to disable the IP rate limiter
    // for local multi-wallet development and testing. Keep false in production.
    'disable_rate_limit'   => false,

    // Configured channels: max byte sizes and retention TTL
    'channels' => [
        'card'   => ['max_bytes' => 524288,  'ttl_days' => 30],
        'wot'    => ['max_bytes' => 524288,  'ttl_days' => 30],
        'backup' => ['max_bytes' => 5242880, 'ttl_days' => 180],
        'post'   => ['max_bytes' => 524288,  'ttl_days' => 21],
        'media'  => [
            'max_bytes'      => 524288,
            'max_user_count' => 12,
            'max_user_bytes' => {},
            'max_channel_mb' => 512,
            'ttl_days'       => 90,
        ],
    ],

    // Lock channel settings (SPEC.md Section 6, Concept Sections 5 & 6)
    'lock' => [
        'enabled'               => true,
        'max_retention_quarter' => 8159,
        'max_inodes'            => 50000,
        'pow_base_bits'         => 8,
        'allow_pow_tickets'     => false,
        'ticket_pow_bits'       => 16,
    ],
];
"#,
        esc_admin,
        esc_dir,
        register_url_val,
        max_members,
        max_total_storage_mb,
        max_user_storage_mb,
        max_media_user_bytes
    )
}

fn escape_toml_basic_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn generate_rust_sync_config_toml(
    admin_pubkey: &str,
    data_dir: &str,
    max_members: usize,
    max_total_storage_mb: usize,
    max_user_storage_mb: usize,
    register_url: Option<&str>,
) -> String {
    let esc_admin = escape_toml_basic_string(admin_pubkey);
    let esc_dir = escape_toml_basic_string(data_dir);
    let max_free_pool_mb = (max_total_storage_mb / 4).max(1).min(max_total_storage_mb);
    let max_user_bytes: u64 = (max_user_storage_mb as u64).saturating_mul(1024 * 1024);
    let max_friends_media_user_bytes: u64 = 6_291_456u64.min(max_user_bytes);
    let public_base_url = register_url
        .and_then(|u| {
            let trimmed = u.trim();
            if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
                return None;
            }
            let parsed = url::Url::parse(trimmed).ok()?;
            if !matches!(parsed.scheme(), "http" | "https")
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return None;
            }
            let host = parsed.host_str()?;
            let port_part = parsed.port().map(|p| format!(":{}", p)).unwrap_or_default();
            Some(format!("{}://{}{}", parsed.scheme(), host, port_part))
        })
        .unwrap_or_else(|| "https://sync.example.org".to_string());
    let esc_base_url = escape_toml_basic_string(&public_base_url);

    format!(
        r#"# HuMoCo Rust Sync Server Configuration (generated by Human Money App)
# Copy this file to config.toml next to humoco-sync-server.

[server]
bind = "127.0.0.1:8080"
public_base_url = "{esc_base_url}"
data_dir = "{esc_dir}"
db_path = "./data/humoco-sync.redb"
admin_pubkey = "{esc_admin}"
server_pepper_hex = ""
soft_warning_percent = 80
trusted_proxy_hops = 1

[mode]
enable_admin_app = true
admin_app_profile = "friends"
max_admin_members = {max_members}
max_admin_user_storage_mb = {max_user_storage_mb}

# Toggle to true to also allow public/community self-registration on the same server
enable_public_registration = false
default_public_profile = "free"
registration_gates = ["email", "code"]
enable_builtin_ui = true
service_name = "HuMoCo Personal Sync"
operator_contact = ""
register_path = "/register"

[storage]
max_total_storage_mb = {max_total_storage_mb}
max_free_pool_mb = {max_free_pool_mb}
downgrade_grace_days = 7

[media]
max_bytes = 524288
max_user_count = 12
max_user_bytes = 3145728
max_channel_mb = 512
ttl_days = 90

[lock]
enabled = true
max_retention_quarter = 8159
max_inodes = 50000
pow_base_bits = 8
allow_pow_tickets = false
ticket_pow_bits = 16
pre_freeze_ttl_secs = 1800
proposal_timelock_secs = 259200
creator_inactivity_secs = 2592000

[security]
# Generate via: humoco-sync-server hash-token
admin_token_hash = ""
api_bearer_token_hash = ""
pow_difficulty_bits = 20
max_registrations_per_ipv4_24_per_day = 10
max_registrations_per_ipv6_48_per_day = 10
ip_requests_per_minute = 120
pubkey_requests_per_minute = 60

[email]
enabled = false
transport = "log"
smtp_host = "mail.example.org"
smtp_port = 587
smtp_username = ""
smtp_password = ""
smtp_tls_mode = "starttls"
from_address = "noreply@example.org"
from_name = "HuMoCo Sync"
token_ttl_minutes = 30
sendmail_path = "/usr/sbin/sendmail"

[profiles.free]
display_name = "Community Free"
max_total_bytes = 1572864
is_free_pool = true
store_contact_email = false

[profiles.free.channels.card]
max_bytes = 524288
ttl_days = 90

[profiles.free.channels.wot]
max_bytes = 131072
ttl_days = 90

[profiles.free.channels.backup]
max_bytes = 524288
ttl_days = 180

[profiles.free.channels.post]
max_bytes = 131072
ttl_days = 30

[profiles.free.channels.media]
max_bytes = 524288
ttl_days = 60
max_user_count = 6
max_user_bytes = 1048576

[profiles.friends]
display_name = "Friends & Host Circle"
max_total_bytes = {max_user_bytes}
is_free_pool = false
store_contact_email = false

[profiles.friends.channels.card]
max_bytes = 1048576
ttl_days = 0

[profiles.friends.channels.wot]
max_bytes = 262144
ttl_days = 0

[profiles.friends.channels.backup]
max_bytes = 4194304
ttl_days = 0

[profiles.friends.channels.post]
max_bytes = 262144
ttl_days = 90

[profiles.friends.channels.media]
max_bytes = 524288
ttl_days = 90
max_user_count = 16
max_user_bytes = {max_friends_media_user_bytes}

[profiles.pro]
display_name = "Supporter Pro"
max_total_bytes = 104857600
is_free_pool = false
store_contact_email = true

[profiles.pro.channels.card]
max_bytes = 2097152
ttl_days = 365

[profiles.pro.channels.wot]
max_bytes = 524288
ttl_days = 365

[profiles.pro.channels.backup]
max_bytes = 16777216
ttl_days = 365

[profiles.pro.channels.post]
max_bytes = 524288
ttl_days = 180

[profiles.pro.channels.media]
max_bytes = 2097152
ttl_days = 365
max_user_count = 64
max_user_bytes = 52428800
"#,
    )
}

struct ZipEntry<'a> {
    name: &'a str,
    data: &'a [u8],
}

pub fn build_sync_zip_bytes(
    admin_pubkey: &str,
    data_dir: &str,
    max_members: usize,
    max_total_storage_mb: usize,
    max_user_storage_mb: usize,
    register_url: Option<&str>,
) -> Vec<u8> {
    let config_php = generate_sync_config_php(
        admin_pubkey,
        data_dir,
        max_members,
        max_total_storage_mb,
        max_user_storage_mb,
        register_url,
    );
    let config_bytes = config_php.as_bytes();

    let entries = [
        ZipEntry {
            name: "sync.php",
            data: SYNC_PHP,
        },
        ZipEntry {
            name: "lock_handler.php",
            data: LOCK_HANDLER_PHP,
        },
        ZipEntry {
            name: "lock_rotation.php",
            data: LOCK_ROTATION_PHP,
        },
        ZipEntry {
            name: "config.php",
            data: config_bytes,
        },
        ZipEntry {
            name: ".htaccess",
            data: HTACCESS,
        },
        ZipEntry {
            name: "robots.txt",
            data: ROBOTS_TXT,
        },
        ZipEntry {
            name: "index.html",
            data: INDEX_HTML,
        },
        ZipEntry {
            name: "README.md",
            data: README_MD,
        },
    ];

    build_pkzip_archive(&entries)
}

fn build_pkzip_archive(entries: &[ZipEntry]) -> Vec<u8> {
    let mut zip_bytes = Vec::new();
    let mut central_dir = Vec::new();

    for entry in entries {
        let offset = zip_bytes.len() as u32;

        let mut hasher = Hasher::new();
        hasher.update(entry.data);
        let crc = hasher.finalize();

        let size = entry.data.len() as u32;
        let name_bytes = entry.name.as_bytes();
        let name_len = name_bytes.len() as u16;

        // Local file header: 30 bytes + name_len + size
        zip_bytes.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]); // Signature
        zip_bytes.extend_from_slice(&[0x14, 0x00]); // Version needed: 2.0
        zip_bytes.extend_from_slice(&[0x00, 0x08]); // Flags: UTF-8 (bit 11)
        zip_bytes.extend_from_slice(&[0x00, 0x00]); // Method: STORE (0)
        zip_bytes.extend_from_slice(&[0x00, 0x00]); // Time
        zip_bytes.extend_from_slice(&[0x21, 0x00]); // Date: 1980-01-01
        zip_bytes.extend_from_slice(&crc.to_le_bytes()); // CRC-32
        zip_bytes.extend_from_slice(&size.to_le_bytes()); // Compressed size
        zip_bytes.extend_from_slice(&size.to_le_bytes()); // Uncompressed size
        zip_bytes.extend_from_slice(&name_len.to_le_bytes()); // Name len
        zip_bytes.extend_from_slice(&[0x00, 0x00]); // Extra field len
        zip_bytes.extend_from_slice(name_bytes); // File name
        zip_bytes.extend_from_slice(entry.data); // File content

        // Central directory entry: 46 bytes + name_len
        central_dir.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]); // Signature
        central_dir.extend_from_slice(&[0x14, 0x03]); // Version made by: ZIP 2.0 (0x14), Unix (0x03)
        central_dir.extend_from_slice(&[0x14, 0x00]); // Version needed: 2.0
        central_dir.extend_from_slice(&[0x00, 0x08]); // Flags: UTF-8
        central_dir.extend_from_slice(&[0x00, 0x00]); // Method: STORE
        central_dir.extend_from_slice(&[0x00, 0x00]); // Time
        central_dir.extend_from_slice(&[0x21, 0x00]); // Date: 1980-01-01
        central_dir.extend_from_slice(&crc.to_le_bytes()); // CRC-32
        central_dir.extend_from_slice(&size.to_le_bytes()); // Compressed size
        central_dir.extend_from_slice(&size.to_le_bytes()); // Uncompressed size
        central_dir.extend_from_slice(&name_len.to_le_bytes()); // Name len
        central_dir.extend_from_slice(&[0x00, 0x00]); // Extra len
        central_dir.extend_from_slice(&[0x00, 0x00]); // Comment len
        central_dir.extend_from_slice(&[0x00, 0x00]); // Disk number start
        central_dir.extend_from_slice(&[0x00, 0x00]); // Internal attributes
                                                      // External attributes: 0o100644 << 16 = 0x81a40000 -> little endian [0x00, 0x00, 0xa4, 0x81]
        central_dir.extend_from_slice(&[0x00, 0x00, 0xa4, 0x81]);
        central_dir.extend_from_slice(&offset.to_le_bytes()); // Relative offset of local header
        central_dir.extend_from_slice(name_bytes);
    }

    let central_dir_offset = zip_bytes.len() as u32;
    let central_dir_size = central_dir.len() as u32;
    let entry_count = entries.len() as u16;

    zip_bytes.extend_from_slice(&central_dir);

    // End of Central Directory (EOCD): 22 bytes
    zip_bytes.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]); // EOCD signature
    zip_bytes.extend_from_slice(&[0x00, 0x00]); // Number of this disk
    zip_bytes.extend_from_slice(&[0x00, 0x00]); // Disk where central directory starts
    zip_bytes.extend_from_slice(&entry_count.to_le_bytes()); // Total entries on this disk
    zip_bytes.extend_from_slice(&entry_count.to_le_bytes()); // Total entries
    zip_bytes.extend_from_slice(&central_dir_size.to_le_bytes()); // Size of central directory
    zip_bytes.extend_from_slice(&central_dir_offset.to_le_bytes()); // Offset of central directory
    zip_bytes.extend_from_slice(&[0x00, 0x00]); // Comment len

    zip_bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_local_zip_entries(
        zip_bytes: &[u8],
    ) -> (std::collections::HashMap<String, Vec<u8>>, usize) {
        let mut offset = 0;
        let mut extracted_files = std::collections::HashMap::new();
        while offset + 30 <= zip_bytes.len() {
            let sig = &zip_bytes[offset..offset + 4];
            if sig == [0x50, 0x4b, 0x01, 0x02] || sig == [0x50, 0x4b, 0x05, 0x06] {
                break;
            }
            assert_eq!(sig, [0x50, 0x4b, 0x03, 0x04], "Invalid local header at {offset}");
            let crc = u32::from_le_bytes(zip_bytes[offset + 14..offset + 18].try_into().unwrap());
            let c_size =
                u32::from_le_bytes(zip_bytes[offset + 18..offset + 22].try_into().unwrap()) as usize;
            let u_size =
                u32::from_le_bytes(zip_bytes[offset + 22..offset + 26].try_into().unwrap()) as usize;
            let name_len =
                u16::from_le_bytes(zip_bytes[offset + 26..offset + 28].try_into().unwrap()) as usize;
            let extra_len =
                u16::from_le_bytes(zip_bytes[offset + 28..offset + 30].try_into().unwrap()) as usize;
            assert_eq!(c_size, u_size);
            let name_start = offset + 30;
            let name = std::str::from_utf8(&zip_bytes[name_start..name_start + name_len]).unwrap();
            let data_start = name_start + name_len + extra_len;
            let data = &zip_bytes[data_start..data_start + c_size];
            let mut hasher = Hasher::new();
            hasher.update(data);
            assert_eq!(crc, hasher.finalize(), "CRC mismatch for {name}");
            extracted_files.insert(name.to_string(), data.to_vec());
            offset = data_start + c_size;
        }
        (extracted_files, offset)
    }

    #[test]
    fn test_pkzip_builder_local_headers_and_embedded_files() {
        let admin_pubkey = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let data_dir = "d-test1234";
        let zip_bytes = build_sync_zip_bytes(
            admin_pubkey,
            data_dir,
            250,
            2048,
            25,
            Some("https://example.com/register"),
        );
        assert_eq!(&zip_bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]);

        let (extracted_files, _) = extract_local_zip_entries(&zip_bytes);
        assert_eq!(extracted_files.len(), 8);
        assert_eq!(extracted_files["sync.php"], SYNC_PHP);
        assert_eq!(extracted_files["lock_handler.php"], LOCK_HANDLER_PHP);
        assert_eq!(extracted_files["lock_rotation.php"], LOCK_ROTATION_PHP);
        assert_eq!(extracted_files[".htaccess"], HTACCESS);
        assert_eq!(extracted_files["robots.txt"], ROBOTS_TXT);
        assert_eq!(extracted_files["index.html"], INDEX_HTML);
        assert_eq!(extracted_files["README.md"], README_MD);

        let cfg_str = std::str::from_utf8(&extracted_files["config.php"]).unwrap();
        assert!(cfg_str.contains(admin_pubkey));
        assert!(cfg_str.contains(data_dir));
        assert!(cfg_str.contains("'max_members'          => 250"));
        assert!(cfg_str.contains("'max_total_storage_mb' => 2048"));
        assert!(cfg_str.contains("'max_user_storage_mb'  => 25"));
        assert!(cfg_str.contains("'max_user_bytes' => 6291456"));
        assert!(cfg_str.contains("'disable_rate_limit'"));
        assert!(cfg_str.contains("https://example.com/register"));
        for marker in ["'card'", "'wot'", "'backup'", "'post'", "'media'", "'lock'"] {
            assert!(cfg_str.contains(marker), "missing {marker}");
        }
    }

    #[test]
    fn test_pkzip_central_directory_and_eocd_structure() {
        let zip_bytes = build_sync_zip_bytes("aa".repeat(32).as_str(), "d-cd", 50, 100, 10, None);
        let (extracted_files, cd_start) = extract_local_zip_entries(&zip_bytes);
        let mut cd_offset = cd_start;
        let mut cd_count = 0usize;
        while cd_offset + 46 <= zip_bytes.len()
            && zip_bytes[cd_offset..cd_offset + 4] == [0x50, 0x4b, 0x01, 0x02]
        {
            assert_eq!(&zip_bytes[cd_offset + 4..cd_offset + 6], &[0x14, 0x03]);
            assert_eq!(&zip_bytes[cd_offset + 14..cd_offset + 16], &[0x21, 0x00]);
            assert_eq!(
                &zip_bytes[cd_offset + 38..cd_offset + 42],
                &[0x00, 0x00, 0xa4, 0x81]
            );
            let cd_crc = u32::from_le_bytes(zip_bytes[cd_offset + 16..cd_offset + 20].try_into().unwrap());
            let cd_csize = u32::from_le_bytes(zip_bytes[cd_offset + 20..cd_offset + 24].try_into().unwrap());
            let cd_usize = u32::from_le_bytes(zip_bytes[cd_offset + 24..cd_offset + 28].try_into().unwrap());
            let cd_name_len = u16::from_le_bytes(zip_bytes[cd_offset + 28..cd_offset + 30].try_into().unwrap()) as usize;
            let cd_extra_len = u16::from_le_bytes(zip_bytes[cd_offset + 30..cd_offset + 32].try_into().unwrap()) as usize;
            let cd_comment_len = u16::from_le_bytes(zip_bytes[cd_offset + 32..cd_offset + 34].try_into().unwrap()) as usize;
            let local_off = u32::from_le_bytes(zip_bytes[cd_offset + 42..cd_offset + 46].try_into().unwrap()) as usize;
            let name = std::str::from_utf8(&zip_bytes[cd_offset + 46..cd_offset + 46 + cd_name_len]).unwrap();
            let file_data = extracted_files.get(name).expect("CD name must match local entry");
            assert_eq!(cd_csize, file_data.len() as u32);
            assert_eq!(cd_usize, file_data.len() as u32);
            let mut hasher = Hasher::new();
            hasher.update(file_data);
            assert_eq!(cd_crc, hasher.finalize());
            assert_eq!(&zip_bytes[local_off..local_off + 4], &[0x50, 0x4b, 0x03, 0x04]);
            cd_offset += 46 + cd_name_len + cd_extra_len + cd_comment_len;
            cd_count += 1;
        }
        assert_eq!(cd_count, 8);
        assert_eq!(cd_offset + 22, zip_bytes.len());
        assert_eq!(&zip_bytes[cd_offset..cd_offset + 4], &[0x50, 0x4b, 0x05, 0x06]);
        assert_eq!(u16::from_le_bytes(zip_bytes[cd_offset + 8..cd_offset + 10].try_into().unwrap()), 8);
        assert_eq!(u16::from_le_bytes(zip_bytes[cd_offset + 10..cd_offset + 12].try_into().unwrap()), 8);
        assert_eq!(
            u32::from_le_bytes(zip_bytes[cd_offset + 12..cd_offset + 16].try_into().unwrap()) as usize,
            cd_offset - cd_start
        );
        assert_eq!(
            u32::from_le_bytes(zip_bytes[cd_offset + 16..cd_offset + 20].try_into().unwrap()) as usize,
            cd_start
        );
    }

    #[test]
    fn test_s08_f8_php_toml_escaping_and_origin_validation() {
        let php_cfg = generate_sync_config_php(
            "aa\n';phpinfo();//",
            "d-1\r\\'",
            50,
            100,
            2,
            Some("https://evil.example/\\'<?php system('id'); ?>\\"),
        );
        assert!(!php_cfg.contains("aa\n"));
        assert!(php_cfg.contains("'admin_pubkey'         => 'aa\\';phpinfo();//'"));
        assert!(php_cfg.contains("'data_dir'             => 'd-1\\\\\\''"));
        assert!(php_cfg.contains("'max_user_bytes' => 2097152"));
        assert!(php_cfg.contains(
            "'register_url'         => 'https://evil.example/\\\\\\'<?php system(\\'id\\'); ?>\\\\'"
        ));

        for reg in [None, Some(""), Some("   ")] {
            let php_blank = generate_sync_config_php("aa", "d-1", 50, 100, 10, reg);
            assert!(php_blank.contains("'register_url'         => null"));
        }

        let toml_ctrl = generate_rust_sync_config_toml(
            "ab\\\"\x00\x07\x08\x0c\n\r\t\x1b\x7fcd",
            "d-\x1b[31m",
            10,
            3,
            10,
            Some("https://sync.example.org:8443/register?ref=1#frag"),
        );
        assert!(
            !toml_ctrl
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t'),
            "TOML output must not contain unescaped control chars"
        );
        assert!(toml_ctrl.contains(r#"admin_pubkey = "ab\\\"\u0000\u0007\b\f\n\r\t\u001B\u007Fcd""#));
        assert!(toml_ctrl.contains(r#"data_dir = "d-\u001B[31m""#));
        assert!(toml_ctrl.contains(r#"public_base_url = "https://sync.example.org:8443""#));
        assert!(toml_ctrl.contains("max_total_storage_mb = 3"));
        assert!(toml_ctrl.contains("max_free_pool_mb = 1\n"));
        assert!(toml_ctrl.contains("max_total_bytes = 10485760\n"));
        assert!(toml_ctrl.contains("max_user_bytes = 6291456\n"));

        let toml_20 = generate_rust_sync_config_toml(
            "aa",
            "d-20",
            10,
            20,
            4,
            Some("http://local.example/reg"),
        );
        assert!(toml_20.contains("max_free_pool_mb = 5\n"));
        assert!(toml_20.contains("max_total_bytes = 4194304\n"));
        assert!(toml_20.contains("max_user_bytes = 4194304\n"));
        assert!(toml_20.contains(r#"public_base_url = "http://local.example""#));

        for bad_reg in [
            "",
            "https://evil.example/ reg",
            "https://evil.example/\x07reg",
            "javascript://evil.example/bar",
            "https://user@evil.example/reg",
            "https://:pass@evil.example/reg",
            "https://user:pass@evil.example/reg",
        ] {
            let toml_bad = generate_rust_sync_config_toml("aa", "d-1", 10, 100, 10, Some(bad_reg));
            assert!(
                toml_bad.contains(r#"public_base_url = "https://sync.example.org""#),
                "expected fallback base url for '{}'",
                bad_reg
            );
        }
    }

    #[test]
    fn test_generate_rust_sync_config_toml() {
        let admin_pubkey = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let toml_cfg = generate_rust_sync_config_toml(
            admin_pubkey,
            "d-test1234",
            300,
            2048,
            25,
            Some("https://sync.example.org/register"),
        );
        assert!(toml_cfg.contains(&format!("admin_pubkey = \"{}\"", admin_pubkey)));
        assert!(toml_cfg.contains("data_dir = \"d-test1234\""));
        assert!(toml_cfg.contains("public_base_url = \"https://sync.example.org\""));
        assert!(toml_cfg.contains("max_admin_members = 300"));
        assert!(toml_cfg.contains("max_admin_user_storage_mb = 25"));
        assert!(toml_cfg.contains("max_total_storage_mb = 2048"));
        assert!(toml_cfg.contains("max_free_pool_mb = 512"));
        assert!(toml_cfg.contains("[lock]"));
        assert!(toml_cfg.contains("[profiles.free]"));
        assert!(toml_cfg.contains("[profiles.friends]"));
        assert!(toml_cfg.contains("[profiles.pro]"));
    }
}
