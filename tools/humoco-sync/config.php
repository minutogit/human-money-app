<?php // config.php – adjust to your needs
return [
    'admin_pubkey'         => '0000000000000000000000000000000000000000000000000000000000000000', // 64-hex Ed25519 VerifyingKey (D_ed)
    'data_dir'             => 'd-0000000000000000', // deterministic directory name, part of read_url
    'register_url'         => null,                 // optional registration URL for public sync services
    'max_members'          => 300,                  // max pubkeys per channel list
    'max_total_storage_mb' => 256,                  // total quota across all channels (soft warning at 80%, hard 507 at 100%)

    // Set to true (or export HUMOCO_DISABLE_RATE_LIMIT=1 in env) to disable the IP rate limiter
    // for local multi-wallet development and testing. Keep false in production.
    'disable_rate_limit'   => false,

    'channels' => [
        'card'   => ['max_bytes' => 131072,  'ttl_days' => 30],
        'wot'    => ['max_bytes' => 131072,  'ttl_days' => 30],
        'backup' => ['max_bytes' => 5242880, 'ttl_days' => 180],
        'post'   => ['max_bytes' => 524288,  'ttl_days' => 21],
        'media'  => [
            'max_bytes'      => 102400, // max 100 KB per encrypted image blob
            'max_user_count' => 5,      // max 5 images per member
            'max_user_bytes' => 409600, // max 400 KB total media per member
            'max_channel_mb' => 100,    // max 100 MB across all media blobs
            'ttl_days'       => 180,    // 180 days (matches max Ad duration)
        ],
    ],
];
