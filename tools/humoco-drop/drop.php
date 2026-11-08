<?php
/**
 * HuMoCo Drop Server – Hardened Privacy-Preserving HTTP Drop (Multi-Channel V1)
 *
 * Requirements: PHP >= 7.4 with ext-sodium, zero database, zero Composer.
 * Strictly enforces 64-hex lowercase Ed25519 signatures, 1-inode rate limiting,
 * atomic index rebuilds, and tamper-resistant key rotation.
 */

// Load configuration
$configFile = __DIR__ . '/config.php';
if (PHP_SAPI === 'cli-server') {
    $envConfig = getenv('HUMOCO_DROP_CONFIG');
    if ($envConfig !== false && $envConfig !== '') {
        $configFile = $envConfig;
    }
}

if (!is_file($configFile)) {
    http_response_code(500);
    header('Content-Type: text/plain; charset=utf-8');
    echo "Configuration file missing.\n";
    exit;
}

$config = require $configFile;
if (!is_array($config)) {
    http_response_code(500);
    header('Content-Type: text/plain; charset=utf-8');
    echo "Invalid configuration format.\n";
    exit;
}

$channels = isset($config['channels']) && is_array($config['channels']) ? $config['channels'] : [];
$maxTotalStorageMb = isset($config['max_total_storage_mb']) ? (int)$config['max_total_storage_mb'] : 256;
$maxTotalBytes = $maxTotalStorageMb * 1024 * 1024;
$softLimitBytes = (int)floor($maxTotalBytes * 0.8);

// Resolve data directory
$dataDirSetting = isset($config['data_dir']) ? (string)$config['data_dir'] : 'data';
if (strpos($dataDirSetting, '..') !== false) {
    http_response_code(500);
    header('Content-Type: text/plain; charset=utf-8');
    echo "Invalid data directory path.\n";
    exit;
}

if ($dataDirSetting !== '' && ($dataDirSetting[0] === '/' || $dataDirSetting[0] === '\\' || (strlen($dataDirSetting) > 2 && $dataDirSetting[1] === ':'))) {
    $dataDir = $dataDirSetting;
} else {
    $dataDir = dirname($configFile) . DIRECTORY_SEPARATOR . $dataDirSetting;
}

if (!is_dir($dataDir)) {
    if (!mkdir($dataDir, 0755, true) && !is_dir($dataDir)) {
        http_response_code(500);
        header('Content-Type: text/plain; charset=utf-8');
        echo "Failed to create data directory.\n";
        exit;
    }
}

$dataHtaccess = $dataDir . DIRECTORY_SEPARATOR . '.htaccess';
if (!file_exists($dataHtaccess)) {
    $htaccessContent = "# Disable PHP execution and script handlers in data directory\n"
        . "Options -Indexes\n\n"
        . "<IfModule mod_php7.c>\n"
        . "    php_flag engine off\n"
        . "</IfModule>\n"
        . "<IfModule mod_php.c>\n"
        . "    php_flag engine off\n"
        . "</IfModule>\n\n"
        . "<IfModule mod_mime.c>\n"
        . "    RemoveHandler .php .phtml .php3 .php4 .php5 .php7 .phps .cgi .pl .py\n"
        . "    RemoveType .php .phtml .php3 .php4 .php5 .php7 .phps .cgi .pl .py\n"
        . "    AddType application/octet-stream .bin\n"
        . "    AddType application/json .json\n"
        . "</IfModule>\n\n"
        . "# Deny direct access to lock and temporary files\n"
        . "<FilesMatch \"^(\\..*|.*\\.tmp.*)\">\n"
        . "    <IfModule mod_authz_core.c>\n"
        . "        Require all denied\n"
        . "    </IfModule>\n"
        . "<IfModule !mod_authz_core.c>\n"
        . "        Order allow,deny\n"
        . "        Deny from all\n"
        . "    </IfModule>\n"
        . "</FilesMatch>\n";
    @file_put_contents($dataHtaccess, $htaccessContent);
}

// Global flock on .lock in script directory across all requests
$lockFile = dirname($configFile) . DIRECTORY_SEPARATOR . '.lock';
$lockFp = fopen($lockFile, 'c+');
if ($lockFp === false) {
    http_response_code(500);
    header('Content-Type: text/plain; charset=utf-8');
    echo "Failed to open lock file.\n";
    exit;
}
if (!flock($lockFp, LOCK_EX)) {
    fclose($lockFp);
    http_response_code(500);
    header('Content-Type: text/plain; charset=utf-8');
    echo "Failed to acquire lock.\n";
    exit;
}

// Response helper
function drop_respond($statusCode, $lockFp, $message = null, array $headers = []) {
    if ($lockFp !== null && is_resource($lockFp)) {
        flock($lockFp, LOCK_UN);
        fclose($lockFp);
    }
    http_response_code($statusCode);
    foreach ($headers as $k => $v) {
        header($k . ': ' . $v);
    }
    if ($message !== null && $message !== '') {
        if (!isset($headers['Content-Type'])) {
            header('Content-Type: text/plain; charset=utf-8');
        }
        echo $message . "\n";
    }
    exit;
}

// Verify Ed25519 detached signature
function verify_ed25519_detached($sigHex, $message, $pubkeyHex) {
    if (strlen($sigHex) !== 128 || !ctype_xdigit($sigHex)) {
        return false;
    }
    if (strlen($pubkeyHex) !== 64 || !ctype_xdigit($pubkeyHex)) {
        return false;
    }
    $sigBytes = hex2bin(strtolower($sigHex));
    $pkBytes = hex2bin(strtolower($pubkeyHex));
    if ($sigBytes === false || $pkBytes === false) {
        return false;
    }
    return sodium_crypto_sign_verify_detached($sigBytes, $message, $pkBytes);
}

// Load and manage .state.php
$stateFile = dirname($configFile) . DIRECTORY_SEPARATOR . '.state.php';
$now = time();
$state = [];

if (file_exists($stateFile)) {
    $rawState = file_get_contents($stateFile);
    if ($rawState !== false && strlen($rawState) > 28) {
        $jsonState = substr($rawState, 28);
        $state = json_decode($jsonState, true);
        if (!is_array($state)) {
            $state = [];
        }
    }
}

if (!isset($state['pepper']) || !is_string($state['pepper']) || strlen($state['pepper']) !== 32) {
    $state['pepper'] = bin2hex(random_bytes(16));
}
if (!isset($state['admin_pubkey']) || !is_string($state['admin_pubkey']) || strlen($state['admin_pubkey']) !== 64) {
    $state['admin_pubkey'] = isset($config['admin_pubkey']) ? strtolower(trim((string)$config['admin_pubkey'])) : str_repeat('0', 64);
}
if (!isset($state['rotation']) || !is_array($state['rotation'])) {
    $state['rotation'] = null;
}
if (!isset($state['nonces']) || !is_array($state['nonces'])) {
    $state['nonces'] = [];
}
if (!isset($state['member_buckets']) || !is_array($state['member_buckets'])) {
    $state['member_buckets'] = [];
}
if (!isset($state['used_bytes'])) {
    $totalUsed = 0;
    $existingBins = glob($dataDir . DIRECTORY_SEPARATOR . '*.*.bin');
    if ($existingBins !== false) {
        foreach ($existingBins as $bf) {
            $sz = filesize($bf);
            if ($sz !== false) {
                $totalUsed += $sz;
            }
        }
    }
    $state['used_bytes'] = $totalUsed;
}

// Rotation expiration check
if ($state['rotation'] !== null) {
    $retireAt = isset($state['rotation']['retire_at']) ? (int)$state['rotation']['retire_at'] : 0;
    if ($now >= $retireAt) {
        $state['rotation'] = null;
    }
}

// Prune nonces older than 120 seconds
foreach ($state['nonces'] as $n => $ts) {
    if ($ts < $now - 120) {
        unset($state['nonces'][$n]);
    }
}

// Prune member buckets idle for over 3600 seconds
foreach ($state['member_buckets'] as $pk => $b) {
    if (isset($b[0]) && $b[0] < $now - 3600) {
        unset($state['member_buckets'][$pk]);
    }
}

function save_state($stateFile, array $state) {
    $tmp = $stateFile . '.tmp.' . getmypid() . '.' . mt_rand();
    $payload = "<?php __halt_compiler(); ?>\n" . json_encode($state, JSON_UNESCAPED_SLASHES);
    if (file_put_contents($tmp, $payload) !== false) {
        rename($tmp, $stateFile);
    }
}

// 1-Inode IP rate limiter file
$rlFile = dirname($configFile) . DIRECTORY_SEPARATOR . 'ratelimit.bin.php';
$rlFp = fopen($rlFile, 'c+b');
if ($rlFp === false) {
    drop_respond(500, $lockFp, 'Internal Server Error: Rate limit store failure');
}
$rlSize = fstat($rlFp)['size'];
if ($rlSize < 65564) {
    fseek($rlFp, 0);
    fwrite($rlFp, "<?php __halt_compiler(); ?>\n" . str_repeat("\0", 65536));
    fflush($rlFp);
}

$clientIp = isset($_SERVER['REMOTE_ADDR']) ? (string)$_SERVER['REMOTE_ADDR'] : '127.0.0.1';
$ipHash = hash_hmac('sha256', $clientIp, gmdate('Y-m-d') . '|' . $state['pepper'], true);
$ipBucketIdx = unpack('V', substr($ipHash, 0, 4))[1] & 4095;
$ipBucketTag = unpack('V', substr($ipHash, 4, 4))[1];
if ($ipBucketTag === 0) {
    $ipBucketTag = 1;
}

fseek($rlFp, 28 + $ipBucketIdx * 16);
$rawBucket = fread($rlFp, 16);
if ($rawBucket !== false && strlen($rawBucket) === 16) {
    $bucket = unpack('VipTag/VlastTs/ftokens/VfailCount', $rawBucket);
} else {
    $bucket = ['ipTag' => 0, 'lastTs' => 0, 'tokens' => 30.0, 'failCount' => 0];
}

if ($bucket['ipTag'] !== $ipBucketTag) {
    $bucket = ['ipTag' => $ipBucketTag, 'lastTs' => $now, 'tokens' => 30.0, 'failCount' => 0];
} else {
    $elapsed = max(0, $now - $bucket['lastTs']);
    $bucket['tokens'] = min(30.0, $bucket['tokens'] + $elapsed * 0.5);
    $bucket['lastTs'] = $now;
}

function write_ip_bucket($rlFp, $idx, array $bucket) {
    fseek($rlFp, 28 + $idx * 16);
    fwrite($rlFp, pack('VVfV', $bucket['ipTag'], $bucket['lastTs'], (float)$bucket['tokens'], (int)$bucket['failCount']));
    fflush($rlFp);
}

function ip_is_locked(array $bucket, $now) {
    return $bucket['failCount'] >= 10 && ($now - $bucket['lastTs'] < 900);
}

// Rebuild index.json atomically
function rebuild_index($dataDir, array $channels) {
    $binFiles = glob($dataDir . DIRECTORY_SEPARATOR . '*.*.bin');
    $indexEntries = [];
    if ($binFiles !== false) {
        foreach ($binFiles as $file) {
            $base = basename($file);
            if (!preg_match('/^([0-9a-f]{64})\.([a-z0-9_]{1,32})\.bin$/', $base, $matches)) {
                continue;
            }
            $id = $matches[1];
            $ch = $matches[2];
            if (!isset($channels[$ch])) {
                continue;
            }
            clearstatcache(true, $file);
            $mtime = filemtime($file);
            $size = filesize($file);
            if ($mtime === false || $size === false) {
                continue;
            }
            $etag = hash_file('sha256', $file);
            $indexEntries[] = [
                'id' => $id,
                'channel' => $ch,
                'mtime' => $mtime,
                'size' => $size,
                'etag' => $etag,
            ];
        }
    }

    usort($indexEntries, function ($a, $b) {
        $c = strcmp($a['channel'], $b['channel']);
        if ($c !== 0) {
            return $c;
        }
        return strcmp($a['id'], $b['id']);
    });

    $indexFile = $dataDir . DIRECTORY_SEPARATOR . 'index.json';
    $tmpIndex = $indexFile . '.tmp.' . getmypid() . '.' . mt_rand();
    $indexPayload = json_encode($indexEntries, JSON_UNESCAPED_SLASHES);
    if (file_put_contents($tmpIndex, $indexPayload) !== false) {
        rename($tmpIndex, $indexFile);
    }
}

// Lazy prune expired slots
function lazy_prune_slots($dataDir, array $channels, array &$state, $now) {
    $binFiles = glob($dataDir . DIRECTORY_SEPARATOR . '*.*.bin');
    if ($binFiles === false || empty($binFiles)) {
        return;
    }
    $changed = false;
    foreach ($binFiles as $file) {
        $base = basename($file);
        if (!preg_match('/^([0-9a-f]{64})\.([a-z0-9_]{1,32})\.bin$/', $base, $matches)) {
            continue;
        }
        $ch = $matches[2];
        if (!isset($channels[$ch]['ttl_days'])) {
            continue;
        }
        $ttlDays = (int)$channels[$ch]['ttl_days'];
        if ($ttlDays <= 0) {
            continue;
        }
        clearstatcache(true, $file);
        $mtime = filemtime($file);
        if ($mtime !== false && ($now - $mtime > $ttlDays * 86400)) {
            $size = filesize($file);
            @unlink($file);
            if ($size !== false) {
                $state['used_bytes'] = max(0, $state['used_bytes'] - $size);
            }
            $changed = true;
        }
    }
    if ($changed) {
        rebuild_index($dataDir, $channels);
    }
}

// Rotation header helper
$commonHeaders = [];
if ($state['rotation'] !== null && $now < $state['rotation']['retire_at']) {
    $rot = $state['rotation'];
    $commonHeaders['X-Humoco-Drop-Rotation'] = "{$rot['old_pubkey']}:{$rot['new_pubkey']}:{$rot['retire_at']}:{$rot['cert_sig']}";
}

// Load members for a channel
function load_channel_members($configFile, $channel) {
    $mFile = dirname($configFile) . DIRECTORY_SEPARATOR . 'members.' . $channel . '.php';
    if (!file_exists($mFile)) {
        return [];
    }
    $raw = file_get_contents($mFile);
    if ($raw === false || strlen($raw) <= 28) {
        return [];
    }
    $json = substr($raw, 28);
    $data = json_decode($json, true);
    if (!is_array($data) || !isset($data['pubkeys']) || !is_array($data['pubkeys'])) {
        return [];
    }
    return $data['pubkeys'];
}

$method = isset($_SERVER['REQUEST_METHOD']) ? strtoupper($_SERVER['REQUEST_METHOD']) : '';

// =========================================================================
// ROUTE 1: GET drop.php?info
// =========================================================================
if ($method === 'GET' && isset($_GET['info'])) {
    if (ip_is_locked($bucket, $now)) {
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(429, $lockFp, 'Too Many Requests', array_merge($commonHeaders, ['Retry-After' => '900']));
    }
    if ($bucket['tokens'] < 1.0) {
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(429, $lockFp, 'Too Many Requests', array_merge($commonHeaders, ['Retry-After' => '2']));
    }
    $bucket['tokens'] -= 1.0;
    write_ip_bucket($rlFp, $ipBucketIdx, $bucket);

    if (isset($config['read_url']) && is_string($config['read_url']) && $config['read_url'] !== '') {
        $readUrl = $config['read_url'];
    } else {
        $isHttps = (!empty($_SERVER['HTTPS']) && $_SERVER['HTTPS'] !== 'off')
            || (isset($_SERVER['SERVER_PORT']) && (int)$_SERVER['SERVER_PORT'] === 443)
            || (isset($_SERVER['HTTP_X_FORWARDED_PROTO']) && strtolower($_SERVER['HTTP_X_FORWARDED_PROTO']) === 'https');
        $scheme = $isHttps ? 'https' : 'http';
        $host = isset($_SERVER['HTTP_HOST']) ? $_SERVER['HTTP_HOST'] : (isset($_SERVER['SERVER_NAME']) ? $_SERVER['SERVER_NAME'] : 'localhost');
        $scriptDir = rtrim(dirname(isset($_SERVER['SCRIPT_NAME']) ? $_SERVER['SCRIPT_NAME'] : '/'), '/\\');
        $readUrl = $scheme . '://' . $host . ($scriptDir !== '' ? $scriptDir : '') . '/' . $dataDirSetting;
    }

    $infoResponse = [
        'drop_pub'     => $state['admin_pubkey'],
        'read_url'     => $readUrl,
        'register_url' => isset($config['register_url']) ? $config['register_url'] : null,
        'channels'     => array_keys($channels),
    ];

    drop_respond(200, $lockFp, json_encode($infoResponse, JSON_UNESCAPED_SLASHES), array_merge($commonHeaders, [
        'Content-Type' => 'application/json; charset=utf-8',
    ]));
}

// =========================================================================
// ROUTE 2: HEAD drop.php
// =========================================================================
if ($method === 'HEAD') {
    if (ip_is_locked($bucket, $now)) {
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(429, $lockFp, null, array_merge($commonHeaders, ['Retry-After' => '900']));
    }

    $pubkey = isset($_SERVER['HTTP_X_DROP_PUBKEY']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_PUBKEY'])) : '';
    $channel = isset($_SERVER['HTTP_X_CHANNEL']) ? trim((string)$_SERVER['HTTP_X_CHANNEL']) : '';

    if (!preg_match('/^[0-9a-f]{64}$/', $pubkey) || !preg_match('/^[a-z0-9_]{1,32}$/', $channel) || !isset($channels[$channel])) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, null, $commonHeaders);
    }

    $channelMembers = load_channel_members($configFile, $channel);
    if (!in_array($pubkey, $channelMembers, true)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, null, $commonHeaders);
    }

    write_ip_bucket($rlFp, $ipBucketIdx, $bucket);

    $slotPath = $dataDir . DIRECTORY_SEPARATOR . $pubkey . '.' . $channel . '.bin';
    $slotEtag = '';
    if (file_exists($slotPath)) {
        clearstatcache(true, $slotPath);
        $mtime = filemtime($slotPath);
        $ttlDays = isset($channels[$channel]['ttl_days']) ? (int)$channels[$channel]['ttl_days'] : 30;
        if ($mtime !== false && ($now - $mtime <= $ttlDays * 86400)) {
            $slotEtag = hash_file('sha256', $slotPath);
        }
    }

    $respHeaders = array_merge($commonHeaders, [
        'X-Drop-Max-Bytes'  => (string)$channels[$channel]['max_bytes'],
        'X-Drop-TTL-Days'   => (string)$channels[$channel]['ttl_days'],
        'X-Drop-Slot-Etag'  => $slotEtag !== false ? $slotEtag : '',
    ]);

    drop_respond(200, $lockFp, null, $respHeaders);
}

// =========================================================================
// ROUTE 3: PUT drop.php?members&channel=<name> (Admin Update Member Pubkeys)
// =========================================================================
if ($method === 'PUT' && isset($_GET['members'])) {
    $channel = isset($_GET['channel']) && $_GET['channel'] !== '' ? (string)$_GET['channel'] : (isset($_SERVER['HTTP_X_CHANNEL']) ? (string)$_SERVER['HTTP_X_CHANNEL'] : '');
    if (!preg_match('/^[a-z0-9_]{1,32}$/', $channel) || !isset($channels[$channel])) {
        drop_respond(404, $lockFp, 'Not Found', $commonHeaders);
    }

    $adminPub = isset($_SERVER['HTTP_X_DROP_PUBKEY']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_PUBKEY'])) : '';
    $timestamp = isset($_SERVER['HTTP_X_DROP_TIMESTAMP']) ? (int)$_SERVER['HTTP_X_DROP_TIMESTAMP'] : 0;
    $nonce = isset($_SERVER['HTTP_X_DROP_NONCE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_NONCE'])) : '';
    $signature = isset($_SERVER['HTTP_X_DROP_SIGNATURE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_SIGNATURE'])) : '';

    if (abs($now - $timestamp) > 60 || !preg_match('/^[0-9a-f]{32}$/', $nonce) || !preg_match('/^[0-9a-f]{128}$/', $signature)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(400, $lockFp, 'Bad Request: Invalid headers', $commonHeaders);
    }

    $isCurrentAdmin = hash_equals($state['admin_pubkey'], $adminPub);
    $isGraceAdmin = ($state['rotation'] !== null && $now < $state['rotation']['retire_at'] && hash_equals($state['rotation']['old_pubkey'], $adminPub));

    if (!$isCurrentAdmin && !$isGraceAdmin) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, 'Forbidden: Unauthorized admin pubkey', $commonHeaders);
    }

    $rawInput = file_get_contents('php://input');
    $canonicalMsg = "HUMOCO-DROP-ADMIN-V1\nPUT_MEMBERS\n{$channel}\n{$timestamp}\n{$nonce}\n" . hash('sha256', $rawInput);

    if (!verify_ed25519_detached($signature, $canonicalMsg, $adminPub)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, 'Forbidden: Invalid admin signature', $commonHeaders);
    }

    if (isset($state['nonces'][$nonce])) {
        drop_respond(409, $lockFp, 'Conflict: Replay detected', $commonHeaders);
    }
    $state['nonces'][$nonce] = $now;

    $bodyData = json_decode($rawInput, true);
    if (!is_array($bodyData) || !isset($bodyData['pubkeys']) || !is_array($bodyData['pubkeys'])) {
        drop_respond(400, $lockFp, 'Bad Request: Expected pubkeys array', $commonHeaders);
    }

    $maxMembers = isset($config['max_members']) ? (int)$config['max_members'] : 300;
    if (count($bodyData['pubkeys']) > $maxMembers) {
        drop_respond(507, $lockFp, 'Insufficient Storage: Exceeds max_members limit', $commonHeaders);
    }

    $deduped = [];
    foreach ($bodyData['pubkeys'] as $pk) {
        if (!is_string($pk) || !preg_match('/^[0-9a-f]{64}$/', $pk)) {
            drop_respond(400, $lockFp, 'Bad Request: Invalid pubkey format (must be 64 lowercase hex)', $commonHeaders);
        }
        $deduped[strtolower($pk)] = true;
    }
    $newMemberPubkeys = array_keys($deduped);
    sort($newMemberPubkeys);

    $membersFile = dirname($configFile) . DIRECTORY_SEPARATOR . 'members.' . $channel . '.php';
    $tmpMembers = $membersFile . '.tmp.' . getmypid() . '.' . mt_rand();
    $membersPayload = "<?php __halt_compiler(); ?>\n" . json_encode(['pubkeys' => $newMemberPubkeys], JSON_UNESCAPED_SLASHES);
    if (file_put_contents($tmpMembers, $membersPayload) === false || !rename($tmpMembers, $membersFile)) {
        @unlink($tmpMembers);
        drop_respond(500, $lockFp, 'Internal Server Error: Failed to write members file', $commonHeaders);
    }

    // Prune slots for this channel if pubkey is no longer a member
    $memberSet = array_flip($newMemberPubkeys);
    $existingSlots = glob($dataDir . DIRECTORY_SEPARATOR . '*.' . $channel . '.bin');
    if ($existingSlots !== false) {
        $suffix = '.' . $channel . '.bin';
        $suffixLen = strlen($suffix);
        foreach ($existingSlots as $sFile) {
            $bName = basename($sFile);
            if (strlen($bName) > $suffixLen && substr($bName, -$suffixLen) === $suffix) {
                $slotPk = substr($bName, 0, -$suffixLen);
                if (!isset($memberSet[$slotPk])) {
                    $sz = filesize($sFile);
                    @unlink($sFile);
                    if ($sz !== false) {
                        $state['used_bytes'] = max(0, $state['used_bytes'] - $sz);
                    }
                }
            }
        }
    }

    lazy_prune_slots($dataDir, $channels, $state, $now);
    rebuild_index($dataDir, $channels);
    save_state($stateFile, $state);

    drop_respond(204, $lockFp, null, $commonHeaders);
}

// =========================================================================
// ROUTE 4: POST drop.php?rotate (Admin 180-Day Key Rotation)
// =========================================================================
if ($method === 'POST' && isset($_GET['rotate'])) {
    $outerPubkey = isset($_SERVER['HTTP_X_DROP_PUBKEY']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_PUBKEY'])) : '';
    $timestamp = isset($_SERVER['HTTP_X_DROP_TIMESTAMP']) ? (int)$_SERVER['HTTP_X_DROP_TIMESTAMP'] : 0;
    $nonce = isset($_SERVER['HTTP_X_DROP_NONCE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_NONCE'])) : '';
    $signature = isset($_SERVER['HTTP_X_DROP_SIGNATURE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_SIGNATURE'])) : '';

    if (abs($now - $timestamp) > 60 || !preg_match('/^[0-9a-f]{32}$/', $nonce) || !preg_match('/^[0-9a-f]{128}$/', $signature)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(400, $lockFp, 'Bad Request: Invalid headers', $commonHeaders);
    }

    $rawInput = file_get_contents('php://input');
    $canonicalMsg = "HUMOCO-DROP-ADMIN-V1\nROTATE\n\n{$timestamp}\n{$nonce}\n" . hash('sha256', $rawInput);

    if (!verify_ed25519_detached($signature, $canonicalMsg, $outerPubkey)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, 'Forbidden: Invalid outer signature', $commonHeaders);
    }

    if (isset($state['nonces'][$nonce])) {
        drop_respond(409, $lockFp, 'Conflict: Replay detected', $commonHeaders);
    }
    $state['nonces'][$nonce] = $now;

    $cert = json_decode($rawInput, true);
    if (!is_array($cert) || !isset($cert['old_pubkey'], $cert['new_pubkey'], $cert['retire_at'], $cert['cert_sig'])) {
        drop_respond(400, $lockFp, 'Bad Request: Invalid certificate format', $commonHeaders);
    }

    $certOld = strtolower(trim((string)$cert['old_pubkey']));
    $certNew = strtolower(trim((string)$cert['new_pubkey']));
    $certRetire = (int)$cert['retire_at'];
    $certSig = strtolower(trim((string)$cert['cert_sig']));

    if (!hash_equals($state['admin_pubkey'], $certOld)) {
        drop_respond(403, $lockFp, 'Forbidden: Old pubkey does not match current admin pubkey', $commonHeaders);
    }
    if (!hash_equals($outerPubkey, $certNew)) {
        drop_respond(400, $lockFp, 'Bad Request: New pubkey does not match request pubkey', $commonHeaders);
    }
    if (hash_equals($certOld, $certNew)) {
        drop_respond(400, $lockFp, 'Bad Request: Old and new pubkeys must differ', $commonHeaders);
    }
    if ($certRetire <= $now || $certRetire > $now + 181 * 86400) {
        drop_respond(400, $lockFp, 'Bad Request: retire_at must be within 180 days', $commonHeaders);
    }

    $certCanonical = "HUMOCO-DROP-ROTATE-V1\n{$certOld}\n{$certNew}\n{$certRetire}";
    if (!verify_ed25519_detached($certSig, $certCanonical, $certOld)) {
        drop_respond(403, $lockFp, 'Forbidden: Invalid rotation certificate signature', $commonHeaders);
    }

    $state['admin_pubkey'] = $certNew;
    $state['rotation'] = [
        'old_pubkey' => $certOld,
        'new_pubkey' => $certNew,
        'retire_at'  => $certRetire,
        'cert_sig'   => $certSig,
    ];
    save_state($stateFile, $state);

    $rotHeaderVal = "{$certOld}:{$certNew}:{$certRetire}:{$certSig}";
    drop_respond(200, $lockFp, json_encode(['ok' => true]), [
        'Content-Type'           => 'application/json; charset=utf-8',
        'X-Humoco-Drop-Rotation' => $rotHeaderVal,
    ]);
}

// =========================================================================
// ROUTE 5: GET drop.php?status (Admin Storage Dashboard Info)
// =========================================================================
if ($method === 'GET' && isset($_GET['status'])) {
    $adminPub = isset($_SERVER['HTTP_X_DROP_PUBKEY']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_PUBKEY'])) : '';
    $timestamp = isset($_SERVER['HTTP_X_DROP_TIMESTAMP']) ? (int)$_SERVER['HTTP_X_DROP_TIMESTAMP'] : 0;
    $nonce = isset($_SERVER['HTTP_X_DROP_NONCE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_NONCE'])) : '';
    $signature = isset($_SERVER['HTTP_X_DROP_SIGNATURE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_SIGNATURE'])) : '';

    if (abs($now - $timestamp) > 60 || !preg_match('/^[0-9a-f]{32}$/', $nonce) || !preg_match('/^[0-9a-f]{128}$/', $signature)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(400, $lockFp, 'Bad Request: Invalid headers', $commonHeaders);
    }

    $isCurrentAdmin = hash_equals($state['admin_pubkey'], $adminPub);
    $isGraceAdmin = ($state['rotation'] !== null && $now < $state['rotation']['retire_at'] && hash_equals($state['rotation']['old_pubkey'], $adminPub));

    if (!$isCurrentAdmin && !$isGraceAdmin) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, 'Forbidden: Unauthorized admin pubkey', $commonHeaders);
    }

    $canonicalMsg = "HUMOCO-DROP-ADMIN-V1\nSTATUS\n\n{$timestamp}\n{$nonce}\n" . hash('sha256', '');
    if (!verify_ed25519_detached($signature, $canonicalMsg, $adminPub)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, 'Forbidden: Invalid admin signature', $commonHeaders);
    }

    if (isset($state['nonces'][$nonce])) {
        drop_respond(409, $lockFp, 'Conflict: Replay detected', $commonHeaders);
    }
    $state['nonces'][$nonce] = $now;

    lazy_prune_slots($dataDir, $channels, $state, $now);
    save_state($stateFile, $state);

    $slotCounts = [];
    $totalSlotCount = 0;
    foreach (array_keys($channels) as $ch) {
        $files = glob($dataDir . DIRECTORY_SEPARATOR . '*.' . $ch . '.bin');
        $c = ($files !== false) ? count($files) : 0;
        $slotCounts[$ch] = $c;
        $totalSlotCount += $c;
    }

    $statusData = [
        'used_bytes'        => $state['used_bytes'],
        'max_bytes'         => $maxTotalBytes,
        'soft_limit_bytes'  => $softLimitBytes,
        'soft_warning'      => ($state['used_bytes'] >= $softLimitBytes),
        'slot_count'        => $totalSlotCount,
        'channel_counts'    => $slotCounts,
        'admin_pubkey'      => $state['admin_pubkey'],
        'rotation'          => $state['rotation'],
    ];

    drop_respond(200, $lockFp, json_encode($statusData, JSON_UNESCAPED_SLASHES), array_merge($commonHeaders, [
        'Content-Type' => 'application/json; charset=utf-8',
    ]));
}

// =========================================================================
// ROUTE 6: PUT drop.php (Member Raw Binary Upload)
// =========================================================================
if ($method === 'PUT') {
    $channel = isset($_SERVER['HTTP_X_CHANNEL']) ? trim((string)$_SERVER['HTTP_X_CHANNEL']) : '';
    if (!preg_match('/^[a-z0-9_]{1,32}$/', $channel) || !isset($channels[$channel])) {
        drop_respond(404, $lockFp, 'Not Found', $commonHeaders);
    }

    $channelCfg = $channels[$channel];
    $maxBytes = (int)$channelCfg['max_bytes'];

    if (isset($_SERVER['CONTENT_LENGTH']) && (int)$_SERVER['CONTENT_LENGTH'] > $maxBytes) {
        drop_respond(413, $lockFp, 'Payload Too Large', $commonHeaders);
    }

    $pubkey = isset($_SERVER['HTTP_X_DROP_PUBKEY']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_PUBKEY'])) : '';
    $timestamp = isset($_SERVER['HTTP_X_DROP_TIMESTAMP']) ? (int)$_SERVER['HTTP_X_DROP_TIMESTAMP'] : 0;
    $nonce = isset($_SERVER['HTTP_X_DROP_NONCE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_NONCE'])) : '';
    $signature = isset($_SERVER['HTTP_X_DROP_SIGNATURE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_SIGNATURE'])) : '';

    if (abs($now - $timestamp) > 60 || !preg_match('/^[0-9a-f]{32}$/', $nonce) || !preg_match('/^[0-9a-f]{128}$/', $signature) || !preg_match('/^[0-9a-f]{64}$/', $pubkey)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(400, $lockFp, 'Bad Request: Invalid auth headers', $commonHeaders);
    }

    $channelMembers = load_channel_members($configFile, $channel);
    if (!in_array($pubkey, $channelMembers, true)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, 'Forbidden: Pubkey is not an authorized member', $commonHeaders);
    }

    $inputFp = fopen('php://input', 'rb');
    if ($inputFp === false) {
        drop_respond(500, $lockFp, 'Internal Server Error', $commonHeaders);
    }

    $rawBody = '';
    $bytesRead = 0;
    while (!feof($inputFp)) {
        $chunk = fread($inputFp, 8192);
        if ($chunk === false || $chunk === '') {
            break;
        }
        $bytesRead += strlen($chunk);
        if ($bytesRead > $maxBytes) {
            fclose($inputFp);
            drop_respond(413, $lockFp, 'Payload Too Large', $commonHeaders);
        }
        $rawBody .= $chunk;
    }
    fclose($inputFp);

    if (strlen($rawBody) === 0) {
        drop_respond(400, $lockFp, 'Bad Request: Empty payload', $commonHeaders);
    }

    $canonicalMsg = "HUMOCO-DROP-REQ-V1\nPUT\n{$channel}\n{$timestamp}\n{$nonce}\n" . hash('sha256', $rawBody);
    if (!verify_ed25519_detached($signature, $canonicalMsg, $pubkey)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, 'Forbidden: Invalid request signature', $commonHeaders);
    }

    // Authenticated request: does NOT increment IP failCount! Enforce per-member token bucket
    if (!isset($state['member_buckets'][$pubkey])) {
        $state['member_buckets'][$pubkey] = [$now, 15.0];
    }
    $mb = &$state['member_buckets'][$pubkey];
    $elapsedMb = max(0, $now - $mb[0]);
    $mb[1] = min(15.0, $mb[1] + $elapsedMb * 0.25);
    $mb[0] = $now;

    if ($mb[1] < 1.0) {
        save_state($stateFile, $state);
        drop_respond(429, $lockFp, 'Too Many Requests', array_merge($commonHeaders, ['Retry-After' => '4']));
    }
    $mb[1] -= 1.0;

    if (isset($state['nonces'][$nonce])) {
        drop_respond(409, $lockFp, 'Conflict: Replay detected', $commonHeaders);
    }
    $state['nonces'][$nonce] = $now;

    lazy_prune_slots($dataDir, $channels, $state, $now);

    $slotFile = $dataDir . DIRECTORY_SEPARATOR . $pubkey . '.' . $channel . '.bin';
    $currentEtag = null;
    $oldSize = 0;
    if (file_exists($slotFile)) {
        clearstatcache(true, $slotFile);
        $mtime = filemtime($slotFile);
        $ttlDays = (int)$channelCfg['ttl_days'];
        if ($mtime !== false && ($now - $mtime <= $ttlDays * 86400)) {
            $currentEtag = hash_file('sha256', $slotFile);
            $sz = filesize($slotFile);
            $oldSize = ($sz !== false) ? $sz : 0;
        }
    }

    // If-Match precondition check (§6.1)
    if (isset($_SERVER['HTTP_IF_MATCH'])) {
        $ifMatch = trim((string)$_SERVER['HTTP_IF_MATCH'], " \t\n\r\0\x0B\"");
        if ($ifMatch === 'none' || $ifMatch === '') {
            if ($currentEtag !== null) {
                drop_respond(412, $lockFp, 'Precondition Failed', array_merge($commonHeaders, [
                    'ETag' => '"' . $currentEtag . '"',
                ]));
            }
        } else {
            if ($currentEtag === null || !hash_equals($currentEtag, $ifMatch)) {
                $etagHeaderVal = ($currentEtag !== null) ? '"' . $currentEtag . '"' : '""';
                drop_respond(412, $lockFp, 'Precondition Failed', array_merge($commonHeaders, [
                    'ETag' => $etagHeaderVal,
                ]));
            }
        }
    }

    // Hard quota check
    $newSize = strlen($rawBody);
    $delta = $newSize - $oldSize;
    if ($state['used_bytes'] + $delta > $maxTotalBytes) {
        drop_respond(507, $lockFp, 'Insufficient Storage: Global drop quota exceeded', $commonHeaders);
    }

    $tmpSlot = $slotFile . '.tmp.' . getmypid() . '.' . mt_rand();
    if (file_put_contents($tmpSlot, $rawBody) === false || !rename($tmpSlot, $slotFile)) {
        @unlink($tmpSlot);
        drop_respond(500, $lockFp, 'Internal Server Error: Failed to write slot', $commonHeaders);
    }

    $state['used_bytes'] = max(0, $state['used_bytes'] + $delta);
    rebuild_index($dataDir, $channels);
    save_state($stateFile, $state);

    $newEtag = hash('sha256', $rawBody);
    drop_respond(204, $lockFp, null, array_merge($commonHeaders, [
        'ETag' => '"' . $newEtag . '"',
    ]));
}

// =========================================================================
// ROUTE 7: DELETE drop.php (Member Slot Delete)
// =========================================================================
if ($method === 'DELETE') {
    $channel = isset($_SERVER['HTTP_X_CHANNEL']) ? trim((string)$_SERVER['HTTP_X_CHANNEL']) : '';
    if (!preg_match('/^[a-z0-9_]{1,32}$/', $channel) || !isset($channels[$channel])) {
        drop_respond(404, $lockFp, 'Not Found', $commonHeaders);
    }

    $pubkey = isset($_SERVER['HTTP_X_DROP_PUBKEY']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_PUBKEY'])) : '';
    $timestamp = isset($_SERVER['HTTP_X_DROP_TIMESTAMP']) ? (int)$_SERVER['HTTP_X_DROP_TIMESTAMP'] : 0;
    $nonce = isset($_SERVER['HTTP_X_DROP_NONCE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_NONCE'])) : '';
    $signature = isset($_SERVER['HTTP_X_DROP_SIGNATURE']) ? strtolower(trim((string)$_SERVER['HTTP_X_DROP_SIGNATURE'])) : '';

    if (abs($now - $timestamp) > 60 || !preg_match('/^[0-9a-f]{32}$/', $nonce) || !preg_match('/^[0-9a-f]{128}$/', $signature) || !preg_match('/^[0-9a-f]{64}$/', $pubkey)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(400, $lockFp, 'Bad Request: Invalid auth headers', $commonHeaders);
    }

    $slotFile = $dataDir . DIRECTORY_SEPARATOR . $pubkey . '.' . $channel . '.bin';
    $slotExists = file_exists($slotFile);
    $channelMembers = load_channel_members($configFile, $channel);
    $isMember = in_array($pubkey, $channelMembers, true);

    if (!$isMember && !$slotExists) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(404, $lockFp, 'Not Found', $commonHeaders);
    }

    $canonicalMsg = "HUMOCO-DROP-REQ-V1\nDELETE\n{$channel}\n{$timestamp}\n{$nonce}\n" . hash('sha256', '');
    if (!verify_ed25519_detached($signature, $canonicalMsg, $pubkey)) {
        $bucket['failCount'] = min(100, $bucket['failCount'] + 1);
        write_ip_bucket($rlFp, $ipBucketIdx, $bucket);
        drop_respond(403, $lockFp, 'Forbidden: Invalid request signature', $commonHeaders);
    }

    if (!isset($state['member_buckets'][$pubkey])) {
        $state['member_buckets'][$pubkey] = [$now, 15.0];
    }
    $mb = &$state['member_buckets'][$pubkey];
    $elapsedMb = max(0, $now - $mb[0]);
    $mb[1] = min(15.0, $mb[1] + $elapsedMb * 0.25);
    $mb[0] = $now;
    if ($mb[1] < 1.0) {
        save_state($stateFile, $state);
        drop_respond(429, $lockFp, 'Too Many Requests', array_merge($commonHeaders, ['Retry-After' => '4']));
    }
    $mb[1] -= 1.0;

    if (isset($state['nonces'][$nonce])) {
        drop_respond(409, $lockFp, 'Conflict: Replay detected', $commonHeaders);
    }
    $state['nonces'][$nonce] = $now;

    if ($slotExists) {
        $sz = filesize($slotFile);
        @unlink($slotFile);
        if ($sz !== false) {
            $state['used_bytes'] = max(0, $state['used_bytes'] - $sz);
        }
        rebuild_index($dataDir, $channels);
        save_state($stateFile, $state);
    }

    drop_respond(204, $lockFp, null, $commonHeaders);
}

header('Allow: GET, HEAD, PUT, DELETE, POST');
drop_respond(405, $lockFp, 'Method Not Allowed', $commonHeaders);
