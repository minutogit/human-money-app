<?php
/**
 * HuMoCo Drop Server – Zero-dependency privacy-preserving HTTP drop
 *
 * Requirements: PHP >= 7.4, standard hash and json functions, zero database, zero Composer.
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

// Ensure data directory exists (0755) and initialize .htaccess for Apache static safety
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
        . "    <IfModule !mod_authz_core.c>\n"
        . "        Order allow,deny\n"
        . "        Deny from all\n"
        . "    </IfModule>\n"
        . "</FilesMatch>\n";
    @file_put_contents($dataHtaccess, $htaccessContent);
}

// Method check: only PUT and DELETE are allowed on drop.php
$method = isset($_SERVER['REQUEST_METHOD']) ? strtoupper($_SERVER['REQUEST_METHOD']) : '';
if ($method !== 'PUT' && $method !== 'DELETE') {
    header('Allow: PUT, DELETE');
    http_response_code(405);
    header('Content-Type: text/plain; charset=utf-8');
    echo "Method Not Allowed\n";
    exit;
}

// Helper: send HTTP response and cleanly release lock
function drop_respond($statusCode, $lockFp, $message = null) {
    if ($lockFp !== null && is_resource($lockFp)) {
        flock($lockFp, LOCK_UN);
        fclose($lockFp);
    }
    http_response_code($statusCode);
    if ($message !== null && $message !== '') {
        header('Content-Type: text/plain; charset=utf-8');
        echo $message . "\n";
    }
    exit;
}

// Helper: run TTL purge across configured channels and rebuild index.json atomically
function purge_and_rebuild_index($dataDir, array $channels) {
    $now = time();
    $binFiles = glob($dataDir . DIRECTORY_SEPARATOR . '*.*.bin');
    $indexEntries = [];

    if ($binFiles !== false) {
        foreach ($binFiles as $file) {
            $base = basename($file);
            if (!preg_match('/^([0-9a-f]{32})\.([a-z0-9_]{1,32})\.bin$/', $base, $matches)) {
                continue;
            }
            $id = $matches[1];
            $ch = $matches[2];

            if (!isset($channels[$ch]) || !is_array($channels[$ch])) {
                continue;
            }

            clearstatcache(true, $file);
            $mtime = filemtime($file);
            if ($mtime === false) {
                continue;
            }

            $ttlDays = isset($channels[$ch]['ttl_days']) ? (int)$channels[$ch]['ttl_days'] : 0;
            if ($ttlDays > 0 && $mtime < ($now - ($ttlDays * 86400))) {
                @unlink($file);
                continue;
            }

            $size = filesize($file);
            if ($size === false) {
                continue;
            }

            $indexEntries[] = [
                'id' => $id,
                'channel' => $ch,
                'mtime' => $mtime,
                'size' => $size,
            ];
        }
    }

    // Sort deterministically: by channel ASC, then id ASC
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

// Acquire exclusive flock across all mutating operations
$lockFile = $dataDir . DIRECTORY_SEPARATOR . '.lock';
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

$channels = isset($config['channels']) && is_array($config['channels']) ? $config['channels'] : [];

// Route 1: Admin member list update (PUT drop.php?members&channel=<name> + X-Admin-Token)
if ($method === 'PUT' && isset($_GET['members'])) {
    $channel = isset($_GET['channel']) && $_GET['channel'] !== '' ? (string)$_GET['channel'] : (isset($_SERVER['HTTP_X_CHANNEL']) ? (string)$_SERVER['HTTP_X_CHANNEL'] : '');
    if (!preg_match('/^[a-z0-9_]{1,32}$/', $channel) || !isset($channels[$channel]) || !is_array($channels[$channel])) {
        drop_respond(404, $lockFp, 'Not Found');
    }

    $cfgAdminHash = isset($config['admin_token_hash']) ? strtolower(trim((string)$config['admin_token_hash'])) : '';
    // Reject unconfigured or default all-zeros hash
    if (preg_match('/^0{64}$/', $cfgAdminHash) || !preg_match('/^[0-9a-f]{64}$/', $cfgAdminHash)) {
        drop_respond(404, $lockFp, 'Not Found');
    }

    $adminToken = isset($_SERVER['HTTP_X_ADMIN_TOKEN']) ? trim((string)$_SERVER['HTTP_X_ADMIN_TOKEN']) : '';
    if (!preg_match('/^[0-9a-fA-F]{64}$/', $adminToken)) {
        drop_respond(404, $lockFp, 'Not Found');
    }
    $rawAdminBytes = hex2bin($adminToken);
    if ($rawAdminBytes === false) {
        drop_respond(404, $lockFp, 'Not Found');
    }
    $calcAdminHash = hash('sha256', $rawAdminBytes);
    if (!hash_equals($cfgAdminHash, $calcAdminHash)) {
        drop_respond(404, $lockFp, 'Not Found');
    }

    $rawInput = file_get_contents('php://input');
    $bodyData = json_decode($rawInput, true);
    if (!is_array($bodyData) || !isset($bodyData['ids']) || !is_array($bodyData['ids'])) {
        drop_respond(400, $lockFp, 'Bad Request');
    }

    $maxMembers = isset($config['max_members']) ? (int)$config['max_members'] : 300;
    if (count($bodyData['ids']) > $maxMembers) {
        drop_respond(507, $lockFp, 'Insufficient Storage');
    }

    $deduped = [];
    foreach ($bodyData['ids'] as $id) {
        if (!is_string($id) || !preg_match('/^[0-9a-f]{32}$/', $id)) {
            drop_respond(400, $lockFp, 'Bad Request');
        }
        $deduped[$id] = true;
    }
    $memberIds = array_keys($deduped);
    sort($memberIds);

    $membersFile = $dataDir . DIRECTORY_SEPARATOR . 'members.' . $channel . '.json';
    $tmpMembers = $membersFile . '.tmp.' . getmypid() . '.' . mt_rand();
    $membersPayload = json_encode(['ids' => $memberIds], JSON_UNESCAPED_SLASHES);
    if (file_put_contents($tmpMembers, $membersPayload) === false) {
        drop_respond(500, $lockFp, 'Internal Server Error');
    }
    if (!rename($tmpMembers, $membersFile)) {
        @unlink($tmpMembers);
        drop_respond(500, $lockFp, 'Internal Server Error');
    }

    // Delete existing slots for this channel if ID is not in the new member list
    $memberIdSet = array_flip($memberIds);
    $channelPattern = $dataDir . DIRECTORY_SEPARATOR . '*.' . $channel . '.bin';
    $slotFiles = glob($channelPattern);
    if ($slotFiles !== false) {
        $suffix = '.' . $channel . '.bin';
        $suffixLen = strlen($suffix);
        foreach ($slotFiles as $sFile) {
            $bName = basename($sFile);
            if (strlen($bName) > $suffixLen && substr($bName, -$suffixLen) === $suffix) {
                $slotId = substr($bName, 0, -$suffixLen);
                if (!isset($memberIdSet[$slotId])) {
                    @unlink($sFile);
                }
            }
        }
    }

    purge_and_rebuild_index($dataDir, $channels);
    drop_respond(204, $lockFp);
}

// Route 2: Member slot upload (PUT drop.php + X-Drop-Token + X-Channel)
if ($method === 'PUT') {
    $channel = isset($_SERVER['HTTP_X_CHANNEL']) ? trim((string)$_SERVER['HTTP_X_CHANNEL']) : '';
    if (!preg_match('/^[a-z0-9_]{1,32}$/', $channel) || !isset($channels[$channel]) || !is_array($channels[$channel])) {
        drop_respond(404, $lockFp, 'Not Found');
    }

    $dropToken = isset($_SERVER['HTTP_X_DROP_TOKEN']) ? trim((string)$_SERVER['HTTP_X_DROP_TOKEN']) : '';
    if (!preg_match('/^[0-9a-fA-F]{64}$/', $dropToken)) {
        drop_respond(404, $lockFp, 'Not Found');
    }
    $rawDropBytes = hex2bin($dropToken);
    if ($rawDropBytes === false) {
        drop_respond(404, $lockFp, 'Not Found');
    }
    $accessId = substr(hash('sha256', $rawDropBytes), 0, 32);

    $membersFile = $dataDir . DIRECTORY_SEPARATOR . 'members.' . $channel . '.json';
    if (!file_exists($membersFile)) {
        drop_respond(404, $lockFp, 'Not Found');
    }
    $membersContent = file_get_contents($membersFile);
    $membersData = json_decode($membersContent, true);
    if (!is_array($membersData) || !isset($membersData['ids']) || !is_array($membersData['ids']) || !in_array($accessId, $membersData['ids'], true)) {
        drop_respond(404, $lockFp, 'Not Found');
    }

    $channelCfg = $channels[$channel];
    $maxBytes = isset($channelCfg['max_bytes']) ? (int)$channelCfg['max_bytes'] : 65536;
    $maxB64Len = (int)(ceil($maxBytes / 3) * 4) + 4096;

    if (isset($_SERVER['CONTENT_LENGTH']) && (int)$_SERVER['CONTENT_LENGTH'] > $maxB64Len) {
        drop_respond(413, $lockFp, 'Payload Too Large');
    }

    $inputFp = fopen('php://input', 'rb');
    if ($inputFp === false) {
        drop_respond(500, $lockFp, 'Internal Server Error');
    }
    $body = '';
    $bytesRead = 0;
    while (!feof($inputFp)) {
        $chunk = fread($inputFp, 8192);
        if ($chunk === false || $chunk === '') {
            break;
        }
        $bytesRead += strlen($chunk);
        if ($bytesRead > $maxB64Len) {
            fclose($inputFp);
            drop_respond(413, $lockFp, 'Payload Too Large');
        }
        $body .= $chunk;
    }
    fclose($inputFp);

    $cleanBody = preg_replace('/\s+/', '', $body);
    $decoded = base64_decode($cleanBody, true);
    if ($decoded === false) {
        drop_respond(400, $lockFp, 'Bad Request');
    }
    if (strlen($decoded) > $maxBytes) {
        drop_respond(413, $lockFp, 'Payload Too Large');
    }

    $slotFile = $dataDir . DIRECTORY_SEPARATOR . $accessId . '.' . $channel . '.bin';
    $tmpSlot = $slotFile . '.tmp.' . getmypid() . '.' . mt_rand();
    if (file_put_contents($tmpSlot, $decoded) === false) {
        drop_respond(500, $lockFp, 'Internal Server Error');
    }
    if (!rename($tmpSlot, $slotFile)) {
        @unlink($tmpSlot);
        drop_respond(500, $lockFp, 'Internal Server Error');
    }

    purge_and_rebuild_index($dataDir, $channels);
    drop_respond(204, $lockFp);
}

// Route 3: Member slot delete (DELETE drop.php + X-Drop-Token + X-Channel)
if ($method === 'DELETE') {
    $channel = isset($_SERVER['HTTP_X_CHANNEL']) ? trim((string)$_SERVER['HTTP_X_CHANNEL']) : '';
    if (!preg_match('/^[a-z0-9_]{1,32}$/', $channel) || !isset($channels[$channel]) || !is_array($channels[$channel])) {
        drop_respond(404, $lockFp, 'Not Found');
    }

    $dropToken = isset($_SERVER['HTTP_X_DROP_TOKEN']) ? trim((string)$_SERVER['HTTP_X_DROP_TOKEN']) : '';
    if (!preg_match('/^[0-9a-fA-F]{64}$/', $dropToken)) {
        drop_respond(404, $lockFp, 'Not Found');
    }
    $rawDropBytes = hex2bin($dropToken);
    if ($rawDropBytes === false) {
        drop_respond(404, $lockFp, 'Not Found');
    }
    $accessId = substr(hash('sha256', $rawDropBytes), 0, 32);

    $membersFile = $dataDir . DIRECTORY_SEPARATOR . 'members.' . $channel . '.json';
    if (!file_exists($membersFile)) {
        drop_respond(404, $lockFp, 'Not Found');
    }
    $membersContent = file_get_contents($membersFile);
    $membersData = json_decode($membersContent, true);
    if (!is_array($membersData) || !isset($membersData['ids']) || !is_array($membersData['ids']) || !in_array($accessId, $membersData['ids'], true)) {
        drop_respond(404, $lockFp, 'Not Found');
    }

    $slotFile = $dataDir . DIRECTORY_SEPARATOR . $accessId . '.' . $channel . '.bin';
    if (file_exists($slotFile)) {
        @unlink($slotFile);
    }

    purge_and_rebuild_index($dataDir, $channels);
    drop_respond(204, $lockFp);
}

drop_respond(404, $lockFp, 'Not Found');
