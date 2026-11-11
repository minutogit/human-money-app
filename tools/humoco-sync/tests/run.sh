#!/usr/bin/env bash
#
# Integration and unit tests for HuMoCo Sync Server (sync.php)
# Usage: ./tests/run.sh [--allow-missing-php]
#

set -euo pipefail

ALLOW_MISSING_PHP=0
for arg in "$@"; do
    case "$arg" in
        --allow-missing-php)
            ALLOW_MISSING_PHP=1
            ;;
        -h|--help)
            echo "Usage: $0 [--allow-missing-php]"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--allow-missing-php]" >&2
            exit 1
            ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOL_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Check if php binary is available (support isolated test environment in /tmp/php-env)
if ! command -v php >/dev/null 2>&1; then
    if [ -x "/tmp/php-env/bin/php" ]; then
        export PATH="/tmp/php-env/bin:$PATH"
    fi
fi
if ! command -v php >/dev/null 2>&1; then
    if [ "$ALLOW_MISSING_PHP" -eq 1 ]; then
        echo "SKIP: php binary not installed"
        exit 0
    else
        echo "ERROR: php binary not installed (pass --allow-missing-php to skip cleanly in environments without PHP)" >&2
        exit 1
    fi
fi

# Check if curl binary is available
if ! command -v curl >/dev/null 2>&1; then
    echo "ERROR: curl binary not installed" >&2
    exit 1
fi

echo "=========================================================="
echo " HuMoCo Sync Server Test Suite (PHP $(php -r 'echo PHP_VERSION;'))"
echo "=========================================================="

# Check minimum PHP version (>= 7.4) and sodium extension
php -r '
if (version_compare(PHP_VERSION, "7.4.0", "<")) {
    fwrite(STDERR, "ERROR: PHP version >= 7.4 required, found " . PHP_VERSION . "\n");
    exit(1);
}
if (!function_exists("sodium_crypto_sign_detached")) {
    fwrite(STDERR, "ERROR: ext-sodium is required for Ed25519 verification\n");
    exit(1);
}
'

# Create isolated scratch workspace for tests
TEST_DIR="$(mktemp -d /tmp/humoco-sync-test.XXXXXX)"
DATA_DIR="$TEST_DIR/data"

PHP_PID=""
cleanup() {
    if [ -n "$PHP_PID" ] && kill -0 "$PHP_PID" 2>/dev/null; then
        kill "$PHP_PID" 2>/dev/null || true
        wait "$PHP_PID" 2>/dev/null || true
    fi
    if [ -d "$TEST_DIR" ]; then
        rm -rf "$TEST_DIR"
    fi
}
trap cleanup EXIT INT TERM

# Copy sync.php and support files into isolated test environment
cp "$TOOL_DIR/sync.php" "$TEST_DIR/sync.php"
if [ -f "$TOOL_DIR/index.html" ]; then
    cp "$TOOL_DIR/index.html" "$TEST_DIR/index.html"
else
    touch "$TEST_DIR/index.html"
fi

# Generate cryptographically secure test Ed25519 keypairs via PHP CLI
ADMIN_KP="$(php -r '
    $kp = sodium_crypto_sign_keypair();
    $sk = sodium_crypto_sign_secretkey($kp);
    $pk = sodium_crypto_sign_publickey($kp);
    echo bin2hex($sk) . ":" . bin2hex($pk);
')"
ADMIN_SK="$(echo "$ADMIN_KP" | cut -d: -f1)"
ADMIN_PK="$(echo "$ADMIN_KP" | cut -d: -f2)"

ADMIN2_KP="$(php -r '
    $kp = sodium_crypto_sign_keypair();
    $sk = sodium_crypto_sign_secretkey($kp);
    $pk = sodium_crypto_sign_publickey($kp);
    echo bin2hex($sk) . ":" . bin2hex($pk);
')"
ADMIN2_SK="$(echo "$ADMIN2_KP" | cut -d: -f1)"
ADMIN2_PK="$(echo "$ADMIN2_KP" | cut -d: -f2)"

MEMBER1_KP="$(php -r '
    $kp = sodium_crypto_sign_keypair();
    $sk = sodium_crypto_sign_secretkey($kp);
    $pk = sodium_crypto_sign_publickey($kp);
    echo bin2hex($sk) . ":" . bin2hex($pk);
')"
MEMBER1_SK="$(echo "$MEMBER1_KP" | cut -d: -f1)"
MEMBER1_PK="$(echo "$MEMBER1_KP" | cut -d: -f2)"

MEMBER2_KP="$(php -r '
    $kp = sodium_crypto_sign_keypair();
    $sk = sodium_crypto_sign_secretkey($kp);
    $pk = sodium_crypto_sign_publickey($kp);
    echo bin2hex($sk) . ":" . bin2hex($pk);
')"
MEMBER2_SK="$(echo "$MEMBER2_KP" | cut -d: -f1)"
MEMBER2_PK="$(echo "$MEMBER2_KP" | cut -d: -f2)"

MEMBER3_KP="$(php -r '
    $kp = sodium_crypto_sign_keypair();
    $sk = sodium_crypto_sign_secretkey($kp);
    $pk = sodium_crypto_sign_publickey($kp);
    echo bin2hex($sk) . ":" . bin2hex($pk);
')"
MEMBER3_SK="$(echo "$MEMBER3_KP" | cut -d: -f1)"
MEMBER3_PK="$(echo "$MEMBER3_KP" | cut -d: -f2)"

# Write test config.php: max_members = 2, max_bytes = 128, max_total_bytes = 120
cat <<EOF > "$TEST_DIR/config.php"
<?php
return [
    'admin_pubkey'         => '$ADMIN_PK',
    'data_dir'             => 'data',
    'max_members'          => 2,
    'max_total_storage_mb' => 1,
    'max_total_bytes'      => 120,
    'channels' => [
        'card'  => ['max_bytes' => 128, 'ttl_days' => 30],
        'wot'   => ['max_bytes' => 128, 'ttl_days' => 30],
        'media' => [
            'max_bytes'      => 256,
            'max_user_count' => 2,
            'max_user_bytes' => 512,
            'max_channel_mb' => 1,
            'ttl_days'       => 180,
        ],
    ],
];
EOF

# Find a free local TCP port
find_free_port() {
    local port
    for port in $(seq 18200 19200); do
        if ! (echo >/dev/tcp/127.0.0.1/"$port") 2>/dev/null; then
            echo "$port"
            return 0
        fi
    done
    echo "18080"
}

PORT="$(find_free_port)"
BASE_URL="http://127.0.0.1:$PORT"

# Launch internal PHP development web server
php -S "127.0.0.1:$PORT" -t "$TEST_DIR" >"$TEST_DIR/php-server.log" 2>&1 &
PHP_PID=$!

# Wait for server readiness
SERVER_READY=0
for _ in $(seq 1 40); do
    if curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/index.html" 2>/dev/null | grep -q '200'; then
        SERVER_READY=1
        break
    fi
    sleep 0.1
done

if [ "$SERVER_READY" -ne 1 ]; then
    echo "ERROR: PHP server failed to start on port $PORT" >&2
    cat "$TEST_DIR/php-server.log" >&2
    exit 1
fi

echo "[✓] PHP development server running on $BASE_URL (PID $PHP_PID)"

FAIL_COUNT=0
assert_status() {
    local desc="$1"
    local expected="$2"
    local actual="$3"
    if [ "$expected" = "$actual" ]; then
        echo "  [PASS] $desc (HTTP $actual)"
    else
        echo "  [FAIL] $desc (expected HTTP $expected, got HTTP $actual)" >&2
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
}

# Sign helper function for tests using PHP CLI
php_sign() {
    local msg="$1"
    local skHex="$2"
    php -r '
        $msg = $argv[1];
        $sk = hex2bin($argv[2]);
        echo bin2hex(sodium_crypto_sign_detached($msg, $sk));
    ' "$msg" "$skHex"
}

echo ""
echo "=== Test Group 1: Routing & Method Invariants ==="

# 1.1 GET sync.php without ?info -> 405 Method Not Allowed
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X GET "$BASE_URL/sync.php")
assert_status "GET sync.php is rejected with 405" "405" "$CODE"

# 1.2 POST sync.php without ?rotate -> 405 Method Not Allowed
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/sync.php" -d "foo=bar")
assert_status "POST sync.php without rotate is rejected with 405" "405" "$CODE"

# 1.3 GET sync.php?info -> 200 OK
INFO_JSON=$(curl -s "$BASE_URL/sync.php?info")
if echo "$INFO_JSON" | grep -q "$ADMIN_PK" && echo "$INFO_JSON" | grep -q '"card"' && echo "$INFO_JSON" | grep -q '"media"'; then
    echo "  [PASS] GET sync.php?info returned correct admin_pubkey and channel list"
else
    echo "  [FAIL] GET sync.php?info response invalid: $INFO_JSON" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 1.4 Rate limit store fixed 1-inode size check
RL_FILE="$TEST_DIR/ratelimit.bin.php"
if [ -f "$RL_FILE" ]; then
    RL_SZ=$(stat -c%s "$RL_FILE" 2>/dev/null || stat -f%z "$RL_FILE")
    if [ "$RL_SZ" -eq 65564 ]; then
        echo "  [PASS] ratelimit.bin.php has exact 65564 bytes (1-inode invariant)"
    else
        echo "  [FAIL] ratelimit.bin.php size mismatch: expected 65564, got $RL_SZ" >&2
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
    # Check __halt_compiler(); header
    if head -c 28 "$RL_FILE" | grep -q '__halt_compiler'; then
        echo "  [PASS] ratelimit.bin.php is protected by __halt_compiler()"
    else
        echo "  [FAIL] ratelimit.bin.php missing __halt_compiler() prefix" >&2
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
fi

echo ""
echo "=== Test Group 2: HEAD Capability Check ==="

# 2.1 HEAD for unlisted member -> 403 Forbidden
HEAD_CODE=$(curl -s -o /dev/null -w "%{http_code}" -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card")
assert_status "HEAD with unlisted pubkey returns 403" "403" "$HEAD_CODE"

echo ""
echo "=== Test Group 3: Admin Member List Update (PUT sync.php?members) ==="

TS=$(date +%s)
NONCE1="0123456789abcdef0123456789abcdef"
BODY="{\"pubkeys\":[\"$MEMBER1_PK\",\"$MEMBER2_PK\"]}"
BODY_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$BODY")
CANON_MSG=$(printf "HUMOCO-DROP-ADMIN-V1\nPUT_MEMBERS\ncard\n%s\n%s\n%s" "$TS" "$NONCE1" "$BODY_HASH")
SIG=$(php_sign "$CANON_MSG" "$ADMIN_SK")

# 3.1 PUT with wrong signature -> 403
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php?members&channel=card" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE1" \
    -H "X-Drop-Signature: 00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000" \
    -d "$BODY")
assert_status "Admin PUT with forged signature returns 403" "403" "$CODE"

# 3.2 Valid Admin PUT -> 204
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php?members&channel=card" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE1" \
    -H "X-Drop-Signature: $SIG" \
    -d "$BODY")
assert_status "Valid Admin PUT returns 204" "204" "$CODE"

# 3.3 HEAD now succeeds for MEMBER1 -> 200
HEAD_RESP=$(curl -s -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card")
if echo "$HEAD_RESP" | grep -q "200 OK" && echo "$HEAD_RESP" | grep -q "X-Drop-Max-Bytes: 128"; then
    echo "  [PASS] HEAD for authorized member returns 200 OK with capabilities"
else
    echo "  [FAIL] HEAD response mismatch: $HEAD_RESP" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 4: Member Slot Raw PUT & If-Match ==="

SLOT_PAYLOAD="Human Money Voucher Card: Alice - Minuto 50"
SLOT_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$SLOT_PAYLOAD")
TS=$(date +%s)
NONCE2="abcdef0123456789abcdef0123456789"
CANON_PUT=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\n%s\n%s\n%s" "$TS" "$NONCE2" "$SLOT_HASH")
SIG_PUT=$(php_sign "$CANON_PUT" "$MEMBER1_SK")

# 4.1 Valid Member Slot Upload -> 204
PUT_RESP=$(curl -s -i -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE2" \
    -H "X-Drop-Signature: $SIG_PUT" \
    --data-binary "$SLOT_PAYLOAD")

PUT_CODE=$(echo "$PUT_RESP" | grep -m1 "HTTP/" | awk '{print $2}')
assert_status "Valid member slot upload returns 204" "204" "$PUT_CODE"

# Verify ETag in response
if echo "$PUT_RESP" | grep -qi "ETag: \"$SLOT_HASH\""; then
    echo "  [PASS] PUT response contains matching ETag header"
else
    echo "  [FAIL] PUT response missing matching ETag: $PUT_RESP" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 4.2 If-Match "none" when slot already exists -> 412 Precondition Failed
TS=$(date +%s)
NONCE3="11111111222222223333333344444444"
CANON_IFM=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\n%s\n%s\n%s" "$TS" "$NONCE3" "$SLOT_HASH")
SIG_IFM=$(php_sign "$CANON_IFM" "$MEMBER1_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE3" \
    -H "X-Drop-Signature: $SIG_IFM" \
    -H 'If-Match: "none"' \
    --data-binary "$SLOT_PAYLOAD")
assert_status "PUT with If-Match: none on existing slot returns 412" "412" "$CODE"

# 4.3 Static read of slot file
FETCHED=$(curl -s "$BASE_URL/data/$MEMBER1_PK.card.bin")
if [ "$FETCHED" = "$SLOT_PAYLOAD" ]; then
    echo "  [PASS] Static read matches uploaded payload byte-for-byte"
else
    echo "  [FAIL] Static read content mismatch" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 5: 180-Day Key Rotation (POST sync.php?rotate) ==="

RETIRE_AT=$((TS + 180 * 86400))
CERT_CANON=$(printf "HUMOCO-DROP-ROTATE-V1\n%s\n%s\n%d" "$ADMIN_PK" "$ADMIN2_PK" "$RETIRE_AT")
CERT_SIG=$(php_sign "$CERT_CANON" "$ADMIN_SK")

ROT_BODY=$(printf '{"old_pubkey":"%s","new_pubkey":"%s","retire_at":%d,"cert_sig":"%s"}' \
    "$ADMIN_PK" "$ADMIN2_PK" "$RETIRE_AT" "$CERT_SIG")
ROT_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$ROT_BODY")
NONCE_ROT="55555555666666667777777788888888"
CANON_ROT=$(printf "HUMOCO-DROP-ADMIN-V1\nROTATE\n\n%s\n%s\n%s" "$TS" "$NONCE_ROT" "$ROT_HASH")
SIG_ROT=$(php_sign "$CANON_ROT" "$ADMIN2_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/sync.php?rotate" \
    -H "X-Drop-Pubkey: $ADMIN2_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_ROT" \
    -H "X-Drop-Signature: $SIG_ROT" \
    -d "$ROT_BODY")
assert_status "Admin key rotation returns 200" "200" "$CODE"

# Verify GET ?info now returns ADMIN2_PK and rotation header
INFO_AFTER=$(curl -s -i "$BASE_URL/sync.php?info")
if echo "$INFO_AFTER" | grep -q "$ADMIN2_PK" && echo "$INFO_AFTER" | grep -qi "X-Humoco-Sync-Rotation:"; then
    echo "  [PASS] Rotation header present and new admin_pubkey active"
else
    echo "  [FAIL] Rotation header or admin_pubkey invalid: $INFO_AFTER" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 6: Member Slot Delete (DELETE sync.php) ==="

TS=$(date +%s)
NONCE_DEL="99999999aaaaaaaa99999999aaaaaaaa"
EMPTY_HASH="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
CANON_DEL=$(printf "HUMOCO-DROP-REQ-V1\nDELETE\ncard\n%s\n%s\n%s" "$TS" "$NONCE_DEL" "$EMPTY_HASH")
SIG_DEL=$(php_sign "$CANON_DEL" "$MEMBER1_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_DEL" \
    -H "X-Drop-Signature: $SIG_DEL")
assert_status "Member slot deletion returns 204" "204" "$CODE"

# Verify slot is gone
if [ ! -f "$DATA_DIR/$MEMBER1_PK.card.bin" ]; then
    echo "  [PASS] Slot file was unlinked from disk"
else
    echo "  [FAIL] Slot file still exists after DELETE" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 7: Media Blobs (Channel media, Quotas, Ownership & Priority Eviction) ==="

# 7.1 Upload media blob for MEMBER1
MEDIA_ID1=$(php -r 'echo bin2hex(random_bytes(32));')
MEDIA_PAYLOAD1="ENCRYPTED_IMAGE_BLOB_MEMBER1_ALPHA"
MEDIA_HASH1=$(php -r 'echo hash("sha256", $argv[1]);' "$MEDIA_PAYLOAD1")
TS=$(date +%s)
NONCE_M1="ba010101010101010101010101010101"
CANON_PUT_M1=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nmedia:%s\n%s\n%s\n%s" "$MEDIA_ID1" "$TS" "$NONCE_M1" "$MEDIA_HASH1")
SIG_PUT_M1=$(php_sign "$CANON_PUT_M1" "$MEMBER1_SK")

PUT_M1_RESP=$(curl -s -i -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: media" \
    -H "X-Media-Id: $MEDIA_ID1" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_M1" \
    -H "X-Drop-Signature: $SIG_PUT_M1" \
    --data-binary "$MEDIA_PAYLOAD1")

PUT_M1_CODE=$(echo "$PUT_M1_RESP" | grep -m1 "HTTP/" | awk '{print $2}')
assert_status "Upload media blob returns 204" "204" "$PUT_M1_CODE"

if echo "$PUT_M1_RESP" | grep -qi "ETag: \"$MEDIA_HASH1\""; then
    echo "  [PASS] Media PUT response contains matching ETag header"
else
    echo "  [FAIL] Media PUT response missing matching ETag: $PUT_M1_RESP" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 7.2 Static read of media blob and verify index.json exclusion
FETCHED_MEDIA=$(curl -s "$BASE_URL/data/$MEDIA_ID1.media.bin")
if [ "$FETCHED_MEDIA" = "$MEDIA_PAYLOAD1" ]; then
    echo "  [PASS] Static read of .media.bin matches uploaded blob byte-for-byte"
else
    echo "  [FAIL] Static read of media blob mismatch" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

INDEX_JSON=$(curl -s "$BASE_URL/data/index.json")
if echo "$INDEX_JSON" | grep -q "$MEDIA_ID1"; then
    echo "  [FAIL] index.json must NOT contain media blob entries: $INDEX_JSON" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
else
    echo "  [PASS] index.json correctly excludes media blobs"
fi

# 7.3 HEAD on channel media returns quota and usage headers
HEAD_MEDIA_RESP=$(curl -s -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: media")

PAYLOAD1_LEN=$(php -r 'echo strlen($argv[1]);' "$MEDIA_PAYLOAD1")
if echo "$HEAD_MEDIA_RESP" | grep -q "200 OK" \
    && echo "$HEAD_MEDIA_RESP" | grep -qi "X-Drop-Media-Max-Count: 2" \
    && echo "$HEAD_MEDIA_RESP" | grep -qi "X-Drop-Media-Used-Count: 1" \
    && echo "$HEAD_MEDIA_RESP" | grep -qi "X-Drop-Media-Used-Bytes: $PAYLOAD1_LEN"; then
    echo "  [PASS] HEAD on media returns correct X-Drop-Media capability and usage headers"
else
    echo "  [FAIL] HEAD on media response invalid: $HEAD_MEDIA_RESP" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 7.4 Another member cannot overwrite or delete MEMBER1's media blob (403)
TS=$(date +%s)
NONCE_M2_OW="ba020202020202020202020202020202"
FORGED_BODY="MALICIOUS_OVERWRITE_ATTEMPT"
FORGED_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$FORGED_BODY")
CANON_M2_OW=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nmedia:%s\n%s\n%s\n%s" "$MEDIA_ID1" "$TS" "$NONCE_M2_OW" "$FORGED_HASH")
SIG_M2_OW=$(php_sign "$CANON_M2_OW" "$MEMBER2_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER2_PK" \
    -H "X-Channel: media" \
    -H "X-Media-Id: $MEDIA_ID1" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_M2_OW" \
    -H "X-Drop-Signature: $SIG_M2_OW" \
    --data-binary "$FORGED_BODY")
assert_status "Another member cannot overwrite mediaId (returns 403)" "403" "$CODE"

TS=$(date +%s)
NONCE_M2_DEL="ba030303030303030303030303030303"
CANON_M2_DEL=$(printf "HUMOCO-DROP-REQ-V1\nDELETE\nmedia:%s\n%s\n%s\n%s" "$MEDIA_ID1" "$TS" "$NONCE_M2_DEL" "$EMPTY_HASH")
SIG_M2_DEL=$(php_sign "$CANON_M2_DEL" "$MEMBER2_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER2_PK" \
    -H "X-Channel: media" \
    -H "X-Media-Id: $MEDIA_ID1" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_M2_DEL" \
    -H "X-Drop-Signature: $SIG_M2_DEL")
assert_status "Another member cannot delete mediaId (returns 403)" "403" "$CODE"

# 7.5 Exceeding max_user_count returns 507
sleep 1
MEDIA_ID2=$(php -r 'echo bin2hex(random_bytes(32));')
MEDIA_PAYLOAD2="ENCRYPTED_IMAGE_BLOB_MEMBER1_BETA"
MEDIA_HASH2=$(php -r 'echo hash("sha256", $argv[1]);' "$MEDIA_PAYLOAD2")
TS=$(date +%s)
NONCE_M1_2="ba040404040404040404040404040404"
CANON_PUT_M2=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nmedia:%s\n%s\n%s\n%s" "$MEDIA_ID2" "$TS" "$NONCE_M1_2" "$MEDIA_HASH2")
SIG_PUT_M2=$(php_sign "$CANON_PUT_M2" "$MEMBER1_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: media" \
    -H "X-Media-Id: $MEDIA_ID2" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_M1_2" \
    -H "X-Drop-Signature: $SIG_PUT_M2" \
    --data-binary "$MEDIA_PAYLOAD2")
assert_status "Upload 2nd media blob within max_user_count returns 204" "204" "$CODE"

MEDIA_ID3=$(php -r 'echo bin2hex(random_bytes(32));')
MEDIA_PAYLOAD3="ENCRYPTED_IMAGE_BLOB_MEMBER1_GAMMA"
MEDIA_HASH3=$(php -r 'echo hash("sha256", $argv[1]);' "$MEDIA_PAYLOAD3")
TS=$(date +%s)
NONCE_M1_3="ba050505050505050505050505050505"
CANON_PUT_M3=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nmedia:%s\n%s\n%s\n%s" "$MEDIA_ID3" "$TS" "$NONCE_M1_3" "$MEDIA_HASH3")
SIG_PUT_M3=$(php_sign "$CANON_PUT_M3" "$MEMBER1_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: media" \
    -H "X-Media-Id: $MEDIA_ID3" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_M1_3" \
    -H "X-Drop-Signature: $SIG_PUT_M3" \
    --data-binary "$MEDIA_PAYLOAD3")
assert_status "Upload 3rd media blob exceeding max_user_count (2) returns 507" "507" "$CODE"

# 7.6 Priority eviction: uploading Prio-1 wot slot evicts oldest media blob when storage is full
# Authorize MEMBER1 for wot channel
TS=$(date +%s)
NONCE_WOT_MEM="ba060606060606060606060606060606"
WOT_MEM_BODY="{\"pubkeys\":[\"$MEMBER1_PK\"]}"
WOT_MEM_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$WOT_MEM_BODY")
CANON_WOT_MEM=$(printf "HUMOCO-DROP-ADMIN-V1\nPUT_MEMBERS\nwot\n%s\n%s\n%s" "$TS" "$NONCE_WOT_MEM" "$WOT_MEM_HASH")
SIG_WOT_MEM=$(php_sign "$CANON_WOT_MEM" "$ADMIN2_SK")

curl -s -o /dev/null -X PUT "$BASE_URL/sync.php?members&channel=wot" \
    -H "X-Drop-Pubkey: $ADMIN2_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_WOT_MEM" \
    -H "X-Drop-Signature: $SIG_WOT_MEM" \
    -d "$WOT_MEM_BODY"

# Current used storage: MEDIA_ID1 (35 B) + MEDIA_ID2 (34 B) = 69 B. Max total is 120 B.
# Uploading a 73 B wot slot needs 69 + 73 = 142 B > 120 B.
# Priority eviction must automatically evict the oldest media blob (MEDIA_ID1) -> 69 - 35 + 73 = 107 B <= 120 B!
WOT_PAYLOAD="WOT_TRUST_GRAPH_PAYLOAD_FOR_MEMBER1_PRIO1_EVIC_TEST_EXACTLY_70_BYTES_NOW!"
WOT_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$WOT_PAYLOAD")
TS=$(date +%s)
NONCE_WOT="ba070707070707070707070707070707"
CANON_WOT=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nwot\n%s\n%s\n%s" "$TS" "$NONCE_WOT" "$WOT_HASH")
SIG_WOT=$(php_sign "$CANON_WOT" "$MEMBER1_SK")

WOT_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: wot" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_WOT" \
    -H "X-Drop-Signature: $SIG_WOT" \
    --data-binary "$WOT_PAYLOAD")
assert_status "Priority-1 wot slot upload succeeds via eviction (returns 204)" "204" "$WOT_CODE"

# Verify oldest media blob MEDIA_ID1 was evicted from disk
if [ ! -f "$DATA_DIR/$MEDIA_ID1.media.bin" ]; then
    echo "  [PASS] Oldest media blob (MEDIA_ID1) was automatically evicted"
else
    echo "  [FAIL] Oldest media blob (MEDIA_ID1) was NOT evicted" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# Verify newer media blob MEDIA_ID2 is still preserved on disk
if [ -f "$DATA_DIR/$MEDIA_ID2.media.bin" ]; then
    echo "  [PASS] Newer media blob (MEDIA_ID2) is preserved"
else
    echo "  [FAIL] Newer media blob (MEDIA_ID2) was prematurely evicted" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 7.7 Member DELETE of own media blob
TS=$(date +%s)
NONCE_M1_DEL="ba080808080808080808080808080808"
CANON_M1_DEL=$(printf "HUMOCO-DROP-REQ-V1\nDELETE\nmedia:%s\n%s\n%s\n%s" "$MEDIA_ID2" "$TS" "$NONCE_M1_DEL" "$EMPTY_HASH")
SIG_M1_DEL=$(php_sign "$CANON_M1_DEL" "$MEMBER1_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: media" \
    -H "X-Media-Id: $MEDIA_ID2" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_M1_DEL" \
    -H "X-Drop-Signature: $SIG_M1_DEL")
assert_status "Member DELETE of own mediaId returns 204" "204" "$CODE"

if [ ! -f "$DATA_DIR/$MEDIA_ID2.media.bin" ]; then
    echo "  [PASS] Deleted media blob was unlinked from disk"
else
    echo "  [FAIL] Deleted media blob still exists on disk" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=========================================================="
if [ "$FAIL_COUNT" -eq 0 ]; then
    echo " ALL TESTS PASSED SUCCESSFULLY! "
    echo "=========================================================="
    exit 0
else
    echo " $FAIL_COUNT TEST(S) FAILED! "
    echo "=========================================================="
    exit 1
fi
