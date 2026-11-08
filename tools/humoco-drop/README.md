# HuMoCo Drop Server (`humoco-drop`)

The **HuMoCo Drop Server** is an ultra-lightweight, zero-dependency PHP script designed for privacy-preserving, asynchronous contact card exchange, Web of Trust synchronization, encrypted off-site backups, and dual-delivery peer-to-peer messaging (payments, co-signing, attestations) in the decentralized **Human Money (HuMoCo)** ecosystem.

---

## 1. Architecture & Security Invariants

* **Zero Dependencies:** Pure PHP (>= 7.4 with core `ext-sodium`). No Composer, no SQL or SQLite database, no background daemons, and no external extensions.
* **Zero Plaintext Knowledge:** The server operator, hoster, and network observers see only encrypted binary blobs (`SlotBlob` / `PostBoxEntry` / `.hmcbackup`). The server cannot decrypt contact cards, marketplace listings, trust lists, or messages.
* **Strict 64-Hex Ed25519 Request Signatures:** Every mutating request is signed with detached Ed25519 signatures validated via `sodium_crypto_sign_verify_detached`:
  - **Private Drops:** Members sign with a stealth key derived via ECDH + `HUMOCO_DROP_STEALTH_SEED_V1` ($P$).
  - **Public Drops:** Users sign with a deterministic registration key ($R$) derived from mnemonic + provider canonical URL.
* **Direct Static Reads (Zero PHP overhead on reads):** Clients poll `<read_url>/index.json` and `<read_url>/<pubkey64>.<channel>.bin` directly via static web server delivery (Apache / Nginx / Caddy), completely bypassing PHP execution.
* **Fixed 1-Inode IP Rate Limiter (`ratelimit.bin.php`):** A single 65,564-byte file with 4,096 buckets (16 bytes each) handles IP lockout under `flock`. It never allocates extra files, preventing inode exhaustion on shared web hosting.
* **Proxy-Safe Member Exemption:** Authenticated Ed25519-signed requests bypass IP failure throttling (avoiding Cloudflare/Carrier-NAT lockout) and are rate-limited per pubkey via a dedicated token bucket.
* **Tamper-Resistant 180-Day Key Rotation:** Admins can rotate keys ($D_{\text{alt}} \to D_{\text{neu}}$) using a cryptographically signed rotation certificate broadcast in the `X-Humoco-Drop-Rotation` header during a 180-day grace period.

---

## 2. Directory Structure

```
tools/humoco-drop/
├── drop.php             # Core API endpoint
├── config.php           # Server configuration (admin pubkey, storage limits, channels)
├── .htaccess            # Hardening: directory indexing off, internal files protected
├── .state.php           # Internal server state (salt, admin pubkey, rotation, nonces)
├── ratelimit.bin.php    # Fixed 64 KiB 1-inode rate limiter storage
├── members.<ch>.php     # Authorized member pubkey lists per channel
├── robots.txt           # Crawling prevention (User-agent: * / Disallow: /)
├── index.html           # Empty blank landing page
├── README.md            # Installation and documentation
└── tests/run.sh         # Automated test suite using `php -S` and curl
```

`<data_dir>` contains **only**:
- `index.json`: Public catalog of published slots (`[{"id": "<pubkey64>", "channel": "<ch>", "mtime": <int>, "size": <int>, "etag": "<sha256>"}]`)
- `<pubkey64>.<channel>.bin`: Encrypted binary slot files.

---

## 3. Configuration Reference (`config.php`)

```php
<?php
return [
    // 64-hex lowercase Ed25519 VerifyingKey of the drop admin
    'admin_pubkey'         => '0000000000000000000000000000000000000000000000000000000000000000',

    // Deterministic directory name, part of read_url
    'data_dir'             => 'd-0000000000000000',

    // Optional URL where new users can register on public drops
    'register_url'         => null,

    // Maximum number of member public keys per channel
    'max_members'          => 300,

    // Total storage quota in megabytes across all channels
    'max_total_storage_mb' => 256,

    // Configured channels: max byte sizes and retention TTL
    'channels' => [
        'card'   => ['max_bytes' => 65536,   'ttl_days' => 30],
        'wot'    => ['max_bytes' => 131072,  'ttl_days' => 30],
        'backup' => ['max_bytes' => 5242880, 'ttl_days' => 180],
        'post'   => ['max_bytes' => 524288,  'ttl_days' => 21],
    ],
];
```

---

## 4. API Protocol & Endpoints

All mutating requests must use custom HTTP headers:

| Method & Route | Required Headers | Description |
|---|---|---|
| `GET drop.php?info` | – | Self-description: returns `drop_pub`, `read_url`, `register_url`, `channels`. |
| `HEAD drop.php` | `X-Drop-Pubkey`, `X-Channel` | Capability check: returns `200 OK` with `X-Drop-Max-Bytes`, `X-Drop-TTL-Days`, `X-Drop-Slot-Etag` if member; `403` otherwise. |
| `PUT drop.php` | `X-Drop-Pubkey`, `X-Channel`, `X-Drop-Timestamp`, `X-Drop-Nonce`, `X-Drop-Signature`, optional `If-Match` | Raw binary slot upload. Enforces content length, replay nonce, `If-Match` concurrency, and global quota. |
| `DELETE drop.php` | `X-Drop-Pubkey`, `X-Channel`, `X-Drop-Timestamp`, `X-Drop-Nonce`, `X-Drop-Signature` | Member slot deletion. |
| `PUT drop.php?members&channel=<name>` | `X-Drop-Pubkey`, `X-Drop-Timestamp`, `X-Drop-Nonce`, `X-Drop-Signature`, Body: `{"pubkeys": [...]}` | Admin updates member list. Prunes slots for removed keys. |
| `POST drop.php?rotate` | `X-Drop-Pubkey: <new>`, `X-Drop-Timestamp`, `X-Drop-Nonce`, `X-Drop-Signature`, Body: `DropRotationCertificate` | Admin activates 180-day key rotation with old admin signature. |
| `GET drop.php?status` | `X-Drop-Pubkey`, `X-Drop-Timestamp`, `X-Drop-Nonce`, `X-Drop-Signature` | Admin dashboard status (used bytes, quotas, slot counts). |
