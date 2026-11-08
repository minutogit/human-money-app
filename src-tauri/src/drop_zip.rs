// src-tauri/src/drop_zip.rs
use crc32fast::Hasher;

const DROP_PHP: &[u8] = include_bytes!("../../tools/humoco-drop/drop.php");
const HTACCESS: &[u8] = include_bytes!("../../tools/humoco-drop/.htaccess");
const ROBOTS_TXT: &[u8] = include_bytes!("../../tools/humoco-drop/robots.txt");
const INDEX_HTML: &[u8] = include_bytes!("../../tools/humoco-drop/index.html");
const README_MD: &[u8] = include_bytes!("../../tools/humoco-drop/README.md");

pub fn generate_drop_config_php(
    admin_pubkey: &str,
    data_dir: &str,
    max_members: usize,
    max_total_storage_mb: usize,
    register_url: Option<&str>,
) -> String {
    let register_url_val = match register_url {
        Some(url) => format!("'{}'", url.replace('\'', "\\'")),
        None => "null".to_string(),
    };
    format!(
r#"<?php // config.php – adjust to your needs
return [
    // 64-hex lowercase Ed25519 VerifyingKey of the drop admin
    'admin_pubkey'         => '{}',

    // Deterministic directory name, part of read_url
    'data_dir'             => '{}',

    // Optional URL where new users can register on public drops
    'register_url'         => {},

    // Maximum number of member public keys per channel
    'max_members'          => {},

    // Total storage quota in megabytes across all channels
    'max_total_storage_mb' => {},

    // Configured channels: max byte sizes and retention TTL
    'channels' => [
        'card'   => ['max_bytes' => 65536,   'ttl_days' => 30],
        'wot'    => ['max_bytes' => 131072,  'ttl_days' => 30],
        'backup' => ['max_bytes' => 5242880, 'ttl_days' => 180],
        'post'   => ['max_bytes' => 524288,  'ttl_days' => 21],
    ],
];
"#,
        admin_pubkey, data_dir, register_url_val, max_members, max_total_storage_mb
    )
}

struct ZipEntry<'a> {
    name: &'a str,
    data: &'a [u8],
}

pub fn build_drop_zip_bytes(
    admin_pubkey: &str,
    data_dir: &str,
    max_members: usize,
    max_total_storage_mb: usize,
    register_url: Option<&str>,
) -> Vec<u8> {
    let config_php = generate_drop_config_php(
        admin_pubkey,
        data_dir,
        max_members,
        max_total_storage_mb,
        register_url,
    );
    let config_bytes = config_php.as_bytes();

    let entries = [
        ZipEntry {
            name: "drop.php",
            data: DROP_PHP,
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
        zip_bytes.extend_from_slice(&[0x00, 0x00]); // Date
        zip_bytes.extend_from_slice(&crc.to_le_bytes()); // CRC-32
        zip_bytes.extend_from_slice(&size.to_le_bytes()); // Compressed size
        zip_bytes.extend_from_slice(&size.to_le_bytes()); // Uncompressed size
        zip_bytes.extend_from_slice(&name_len.to_le_bytes()); // Name len
        zip_bytes.extend_from_slice(&[0x00, 0x00]); // Extra field len
        zip_bytes.extend_from_slice(name_bytes); // File name
        zip_bytes.extend_from_slice(entry.data); // File content

        // Central directory entry: 46 bytes + name_len
        central_dir.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]); // Signature
        central_dir.extend_from_slice(&[0x3f, 0x00]); // Version made by: Unix (0x3f)
        central_dir.extend_from_slice(&[0x14, 0x00]); // Version needed: 2.0
        central_dir.extend_from_slice(&[0x00, 0x08]); // Flags: UTF-8
        central_dir.extend_from_slice(&[0x00, 0x00]); // Method: STORE
        central_dir.extend_from_slice(&[0x00, 0x00]); // Time
        central_dir.extend_from_slice(&[0x00, 0x00]); // Date
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

    #[test]
    fn test_pkzip_builder_and_extraction() {
        let admin_pubkey = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let data_dir = "d-test1234";
        let max_members = 250;
        let max_total_storage_mb = 256;
        let register_url = Some("https://example.com/register");

        let zip_bytes = build_drop_zip_bytes(
            admin_pubkey,
            data_dir,
            max_members,
            max_total_storage_mb,
            register_url,
        );
        assert!(!zip_bytes.is_empty());

        // Check local header magic of first file
        assert_eq!(&zip_bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]);

        // Parse archive manually to verify PKZIP structure
        let mut offset = 0;
        let mut extracted_files = std::collections::HashMap::new();

        while offset + 30 <= zip_bytes.len() {
            let sig = &zip_bytes[offset..offset + 4];
            if sig == [0x50, 0x4b, 0x01, 0x02] || sig == [0x50, 0x4b, 0x05, 0x06] {
                // Central directory or EOCD reached
                break;
            }
            assert_eq!(sig, [0x50, 0x4b, 0x03, 0x04], "Invalid local header signature at {}", offset);

            let crc = u32::from_le_bytes(zip_bytes[offset + 14..offset + 18].try_into().unwrap());
            let c_size = u32::from_le_bytes(zip_bytes[offset + 18..offset + 22].try_into().unwrap()) as usize;
            let u_size = u32::from_le_bytes(zip_bytes[offset + 22..offset + 26].try_into().unwrap()) as usize;
            let name_len = u16::from_le_bytes(zip_bytes[offset + 26..offset + 28].try_into().unwrap()) as usize;
            let extra_len = u16::from_le_bytes(zip_bytes[offset + 28..offset + 30].try_into().unwrap()) as usize;

            assert_eq!(c_size, u_size);
            let name_start = offset + 30;
            let name = std::str::from_utf8(&zip_bytes[name_start..name_start + name_len]).unwrap();
            let data_start = name_start + name_len + extra_len;
            let data = &zip_bytes[data_start..data_start + c_size];

            let mut hasher = Hasher::new();
            hasher.update(data);
            assert_eq!(crc, hasher.finalize(), "CRC mismatch for {}", name);

            extracted_files.insert(name.to_string(), data.to_vec());
            offset = data_start + c_size;
        }

        // Verify all 6 files are present
        assert_eq!(extracted_files.len(), 6);
        assert!(extracted_files.contains_key("drop.php"));
        assert!(extracted_files.contains_key("config.php"));
        assert!(extracted_files.contains_key(".htaccess"));
        assert!(extracted_files.contains_key("robots.txt"));
        assert!(extracted_files.contains_key("index.html"));
        assert!(extracted_files.contains_key("README.md"));

        // Verify embedded files equality
        assert_eq!(extracted_files["drop.php"], DROP_PHP);
        assert_eq!(extracted_files[".htaccess"], HTACCESS);
        assert_eq!(extracted_files["robots.txt"], ROBOTS_TXT);
        assert_eq!(extracted_files["index.html"], INDEX_HTML);
        assert_eq!(extracted_files["README.md"], README_MD);

        // Verify config.php contents
        let cfg_str = std::str::from_utf8(&extracted_files["config.php"]).unwrap();
        assert!(cfg_str.contains(admin_pubkey));
        assert!(cfg_str.contains(data_dir));
        assert!(cfg_str.contains("250"));
        assert!(cfg_str.contains("256"));
        assert!(cfg_str.contains("https://example.com/register"));
        assert!(cfg_str.contains("'card'"));
        assert!(cfg_str.contains("'wot'"));
        assert!(cfg_str.contains("'backup'"));
        assert!(cfg_str.contains("'post'"));
    }
}
