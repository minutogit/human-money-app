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
CANON_PUT=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\n\n%s\n%s\n%s" "$TS" "$NONCE2" "$SLOT_HASH")
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
CANON_IFM=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\n\n%s\n%s\n%s" "$TS" "$NONCE3" "$SLOT_HASH")
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
echo "=== Test Group 5: POST Rejection (POST sync.php) ==="

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/sync.php?rotate" -d "{}")
assert_status "POST sync.php is rejected with 405" "405" "$CODE"

echo ""
echo "=== Test Group 6: Member Slot Delete (DELETE sync.php) ==="

TS=$(date +%s)
NONCE_DEL="99999999aaaaaaaa99999999aaaaaaaa"
EMPTY_HASH="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
CANON_DEL=$(printf "HUMOCO-DROP-REQ-V1\nDELETE\ncard\n\n%s\n%s\n%s" "$TS" "$NONCE_DEL" "$EMPTY_HASH")
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
CANON_PUT_M1=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nmedia:%s\n\n%s\n%s\n%s" "$MEDIA_ID1" "$TS" "$NONCE_M1" "$MEDIA_HASH1")
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
CANON_M2_OW=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nmedia:%s\n\n%s\n%s\n%s" "$MEDIA_ID1" "$TS" "$NONCE_M2_OW" "$FORGED_HASH")
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
CANON_M2_DEL=$(printf "HUMOCO-DROP-REQ-V1\nDELETE\nmedia:%s\n\n%s\n%s\n%s" "$MEDIA_ID1" "$TS" "$NONCE_M2_DEL" "$EMPTY_HASH")
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
CANON_PUT_M2=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nmedia:%s\n\n%s\n%s\n%s" "$MEDIA_ID2" "$TS" "$NONCE_M1_2" "$MEDIA_HASH2")
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
CANON_PUT_M3=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nmedia:%s\n\n%s\n%s\n%s" "$MEDIA_ID3" "$TS" "$NONCE_M1_3" "$MEDIA_HASH3")
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
SIG_WOT_MEM=$(php_sign "$CANON_WOT_MEM" "$ADMIN_SK")

curl -s -o /dev/null -X PUT "$BASE_URL/sync.php?members&channel=wot" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
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
CANON_WOT=$(printf "HUMOCO-DROP-REQ-V1\nPUT\nwot\n\n%s\n%s\n%s" "$TS" "$NONCE_WOT" "$WOT_HASH")
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
CANON_M1_DEL=$(printf "HUMOCO-DROP-REQ-V1\nDELETE\nmedia:%s\n\n%s\n%s\n%s" "$MEDIA_ID2" "$TS" "$NONCE_M1_DEL" "$EMPTY_HASH")
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
echo "=== Test Group 8: Sub-Mailbox Prefix Isolation & Management (X-Drop-Prefix) ==="

# Clean up wot slot from Test Group 7 to free storage budget (120 bytes max)
TS=$(date +%s)
NONCE_WOT_DEL="ca000000000000000000000000000000"
CANON_WOT_DEL=$(printf "HUMOCO-DROP-REQ-V1\nDELETE\nwot\n\n%s\n%s\n%s" "$TS" "$NONCE_WOT_DEL" "$EMPTY_HASH")
SIG_WOT_DEL=$(php_sign "$CANON_WOT_DEL" "$MEMBER1_SK")
curl -s -o /dev/null -X DELETE "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: wot" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_WOT_DEL" \
    -H "X-Drop-Signature: $SIG_WOT_DEL"

# 8.1 Upload prefixed slots: privat, firma, and root slot
PAYLOAD_PRIVAT="CARD_ALICE_PRIVAT_100"
HASH_PRIVAT=$(php -r 'echo hash("sha256", $argv[1]);' "$PAYLOAD_PRIVAT")
TS=$(date +%s)
NONCE_P="ca010101010101010101010101010101"
CANON_P=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\nprivat\n%s\n%s\n%s" "$TS" "$NONCE_P" "$HASH_PRIVAT")
SIG_P=$(php_sign "$CANON_P" "$MEMBER1_SK")

PUT_P_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: privat" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_P" \
    -H "X-Drop-Signature: $SIG_P" \
    --data-binary "$PAYLOAD_PRIVAT")
assert_status "Upload slot with X-Drop-Prefix: privat returns 204" "204" "$PUT_P_CODE"

if [ -f "$DATA_DIR/${MEMBER1_PK}_privat.card.bin" ]; then
    echo "  [PASS] File ${MEMBER1_PK}_privat.card.bin was created on disk"
else
    echo "  [FAIL] File ${MEMBER1_PK}_privat.card.bin was NOT created" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

PAYLOAD_FIRMA="CARD_ALICE_FIRMA_500"
HASH_FIRMA=$(php -r 'echo hash("sha256", $argv[1]);' "$PAYLOAD_FIRMA")
TS=$(date +%s)
NONCE_F="ca020202020202020202020202020202"
CANON_F=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\nfirma\n%s\n%s\n%s" "$TS" "$NONCE_F" "$HASH_FIRMA")
SIG_F=$(php_sign "$CANON_F" "$MEMBER1_SK")

PUT_F_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: firma" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_F" \
    -H "X-Drop-Signature: $SIG_F" \
    --data-binary "$PAYLOAD_FIRMA")
assert_status "Upload slot with X-Drop-Prefix: firma returns 204" "204" "$PUT_F_CODE"

if [ -f "$DATA_DIR/${MEMBER1_PK}_firma.card.bin" ]; then
    echo "  [PASS] File ${MEMBER1_PK}_firma.card.bin was created on disk"
else
    echo "  [FAIL] File ${MEMBER1_PK}_firma.card.bin was NOT created" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

PAYLOAD_ROOT="CARD_ALICE_ROOT_50"
HASH_ROOT=$(php -r 'echo hash("sha256", $argv[1]);' "$PAYLOAD_ROOT")
TS=$(date +%s)
NONCE_R="ca030303030303030303030303030303"
CANON_R=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\n\n%s\n%s\n%s" "$TS" "$NONCE_R" "$HASH_ROOT")
SIG_R=$(php_sign "$CANON_R" "$MEMBER1_SK")

PUT_R_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_R" \
    -H "X-Drop-Signature: $SIG_R" \
    --data-binary "$PAYLOAD_ROOT")
assert_status "Upload root slot (no prefix) returns 204" "204" "$PUT_R_CODE"

if [ -f "$DATA_DIR/${MEMBER1_PK}.card.bin" ]; then
    echo "  [PASS] File ${MEMBER1_PK}.card.bin was created on disk"
else
    echo "  [FAIL] File ${MEMBER1_PK}.card.bin was NOT created" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 8.2 Verify index.json entries and prefix metadata
INDEX_CHECK=$(php -r '
    $content = file_get_contents($argv[1]);
    $idx = json_decode($content, true);
    if (!is_array($idx)) { echo "INVALID_JSON"; exit; }
    $m1 = $argv[2];
    $hasPrivat = false;
    $hasFirma = false;
    $hasRoot = false;
    foreach ($idx as $e) {
        if ($e["id"] === $m1 && $e["channel"] === "card") {
            if (isset($e["prefix"]) && $e["prefix"] === "privat") $hasPrivat = true;
            if (isset($e["prefix"]) && $e["prefix"] === "firma") $hasFirma = true;
            if (!isset($e["prefix"])) $hasRoot = true;
        }
    }
    if ($hasPrivat && $hasFirma && $hasRoot) {
        echo "OK";
    } else {
        echo "MISSING_ENTRIES: privat=" . ($hasPrivat ? 1 : 0) . " firma=" . ($hasFirma ? 1 : 0) . " root=" . ($hasRoot ? 1 : 0);
    }
' "$BASE_URL/data/index.json" "$MEMBER1_PK")

if [ "$INDEX_CHECK" = "OK" ]; then
    echo "  [PASS] index.json correctly indexes privat, firma, and root slots with prefix metadata"
else
    echo "  [FAIL] index.json prefix verification failed: $INDEX_CHECK" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 8.3 HEAD capability check with independent X-Drop-Slot-Etag
HEAD_PRIV_RESP=$(curl -s -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: privat")
ETAG_PRIV=$(echo "$HEAD_PRIV_RESP" | grep -i "X-Drop-Slot-Etag:" | awk '{print $2}' | tr -d '\r\n')

HEAD_FIRM_RESP=$(curl -s -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: firma")
ETAG_FIRM=$(echo "$HEAD_FIRM_RESP" | grep -i "X-Drop-Slot-Etag:" | awk '{print $2}' | tr -d '\r\n')

HEAD_ROOT_RESP=$(curl -s -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card")
ETAG_ROOT=$(echo "$HEAD_ROOT_RESP" | grep -i "X-Drop-Slot-Etag:" | awk '{print $2}' | tr -d '\r\n')

if [ "$ETAG_PRIV" = "$HASH_PRIVAT" ] && [ "$ETAG_FIRM" = "$HASH_FIRMA" ] && [ "$ETAG_ROOT" = "$HASH_ROOT" ]; then
    echo "  [PASS] HEAD returns independent X-Drop-Slot-Etag for privat, firma, and root mailboxes"
else
    echo "  [FAIL] HEAD ETag mismatch: priv=$ETAG_PRIV (exp $HASH_PRIVAT), firm=$ETAG_FIRM (exp $HASH_FIRMA), root=$ETAG_ROOT (exp $HASH_ROOT)" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 8.4 Forging / mismatching X-Drop-Prefix header -> 403 Forbidden
TS=$(date +%s)
NONCE_FORGE1="ca040404040404040404040404040404"
CANON_FORGE1=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\nprivat\n%s\n%s\n%s" "$TS" "$NONCE_FORGE1" "$HASH_PRIVAT")
SIG_FORGE1=$(php_sign "$CANON_FORGE1" "$MEMBER1_SK")

CODE_FORGE1=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: firma" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_FORGE1" \
    -H "X-Drop-Signature: $SIG_FORGE1" \
    --data-binary "$PAYLOAD_PRIVAT")
assert_status "Signature for privat with X-Drop-Prefix: firma is rejected with 403" "403" "$CODE_FORGE1"

TS=$(date +%s)
NONCE_FORGE2="ca050505050505050505050505050505"
CANON_FORGE2=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\n\n%s\n%s\n%s" "$TS" "$NONCE_FORGE2" "$HASH_ROOT")
SIG_FORGE2=$(php_sign "$CANON_FORGE2" "$MEMBER1_SK")

CODE_FORGE2=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: privat" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_FORGE2" \
    -H "X-Drop-Signature: $SIG_FORGE2" \
    --data-binary "$PAYLOAD_ROOT")
assert_status "Signature for root with X-Drop-Prefix: privat is rejected with 403" "403" "$CODE_FORGE2"

# 8.5 Invalid X-Drop-Prefix syntax rejected with 400 (PUT/DELETE) and 403 (HEAD)
TS=$(date +%s)
INVALID_PREFIXES=("UPPERCASE" "bad_underscore" "-leading" "double--dash")
for bad_p in "${INVALID_PREFIXES[@]}"; do
    CODE_BAD=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
        -H "X-Drop-Pubkey: $MEMBER1_PK" \
        -H "X-Channel: card" \
        -H "X-Drop-Prefix: $bad_p" \
        -H "X-Drop-Timestamp: $TS" \
        -H "X-Drop-Nonce: ca060606060606060606060606060606" \
        -H "X-Drop-Signature: $SIG_P" \
        --data-binary "$PAYLOAD_PRIVAT")
    assert_status "PUT with invalid prefix '$bad_p' is rejected with 400" "400" "$CODE_BAD"
done

HEAD_BAD_CODE=$(curl -s -o /dev/null -w "%{http_code}" -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: INVALID_UPPER")
assert_status "HEAD with invalid prefix is rejected with 403" "403" "$HEAD_BAD_CODE"

PUT_MEDIA_PREFIX_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: media" \
    -H "X-Drop-Prefix: privat" \
    -H "X-Media-Id: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: ca070707070707070707070707070707" \
    -H "X-Drop-Signature: $SIG_P" \
    --data-binary "BLOB")
assert_status "PUT on media with X-Drop-Prefix is rejected with 400" "400" "$PUT_MEDIA_PREFIX_CODE"

HEAD_MEDIA_PREFIX_CODE=$(curl -s -o /dev/null -w "%{http_code}" -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: media" \
    -H "X-Drop-Prefix: privat")
assert_status "HEAD on media with X-Drop-Prefix is rejected with 403" "403" "$HEAD_MEDIA_PREFIX_CODE"

# 8.6 If-Match update on privat does not touch firma or root slot
PAYLOAD_PRIVAT2="CARD_ALICE_PRIVAT_200"
HASH_PRIVAT2=$(php -r 'echo hash("sha256", $argv[1]);' "$PAYLOAD_PRIVAT2")
TS=$(date +%s)
NONCE_P2="ca080808080808080808080808080808"
CANON_P2=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\nprivat\n%s\n%s\n%s" "$TS" "$NONCE_P2" "$HASH_PRIVAT2")
SIG_P2=$(php_sign "$CANON_P2" "$MEMBER1_SK")

CODE_IFM_STALE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: privat" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_P2" \
    -H "X-Drop-Signature: $SIG_P2" \
    -H 'If-Match: "0000000000000000000000000000000000000000000000000000000000000000"' \
    --data-binary "$PAYLOAD_PRIVAT2")
assert_status "PUT with stale If-Match on privat returns 412" "412" "$CODE_IFM_STALE"

TS=$(date +%s)
NONCE_P3="ca090909090909090909090909090909"
CANON_P3=$(printf "HUMOCO-DROP-REQ-V1\nPUT\ncard\nprivat\n%s\n%s\n%s" "$TS" "$NONCE_P3" "$HASH_PRIVAT2")
SIG_P3=$(php_sign "$CANON_P3" "$MEMBER1_SK")

CODE_IFM_OK=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: privat" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_P3" \
    -H "X-Drop-Signature: $SIG_P3" \
    -H "If-Match: \"$HASH_PRIVAT\"" \
    --data-binary "$PAYLOAD_PRIVAT2")
assert_status "PUT with valid If-Match on privat returns 204" "204" "$CODE_IFM_OK"

FETCHED_PRIV2=$(curl -s "$BASE_URL/data/${MEMBER1_PK}_privat.card.bin")
FETCHED_FIRM=$(curl -s "$BASE_URL/data/${MEMBER1_PK}_firma.card.bin")
FETCHED_ROOT=$(curl -s "$BASE_URL/data/${MEMBER1_PK}.card.bin")

if [ "$FETCHED_PRIV2" = "$PAYLOAD_PRIVAT2" ] && [ "$FETCHED_FIRM" = "$PAYLOAD_FIRMA" ] && [ "$FETCHED_ROOT" = "$PAYLOAD_ROOT" ]; then
    echo "  [PASS] privat update preserved firma and root slots intact"
else
    echo "  [FAIL] Slot data isolation failure during If-Match update" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 8.7 DELETE on privat does not touch firma or root slot
TS=$(date +%s)
NONCE_P_DEL="ca0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a"
CANON_P_DEL=$(printf "HUMOCO-DROP-REQ-V1\nDELETE\ncard\nprivat\n%s\n%s\n%s" "$TS" "$NONCE_P_DEL" "$EMPTY_HASH")
SIG_P_DEL=$(php_sign "$CANON_P_DEL" "$MEMBER1_SK")

CODE_P_DEL=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Prefix: privat" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_P_DEL" \
    -H "X-Drop-Signature: $SIG_P_DEL")
assert_status "DELETE on privat sub-mailbox returns 204" "204" "$CODE_P_DEL"

if [ ! -f "$DATA_DIR/${MEMBER1_PK}_privat.card.bin" ] && [ -f "$DATA_DIR/${MEMBER1_PK}_firma.card.bin" ] && [ -f "$DATA_DIR/${MEMBER1_PK}.card.bin" ]; then
    echo "  [PASS] privat slot was deleted while firma and root slots remain intact"
else
    echo "  [FAIL] DELETE isolation failure: privat or other slots in unexpected state" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 8.8 Removing member from members.{channel}.php prunes all remaining prefixed slots
TS=$(date +%s)
NONCE_ADMIN_DEL="ca0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"
BODY_REM="{\"pubkeys\":[\"$MEMBER2_PK\"]}"
BODY_REM_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$BODY_REM")
CANON_REM=$(printf "HUMOCO-DROP-ADMIN-V1\nPUT_MEMBERS\ncard\n%s\n%s\n%s" "$TS" "$NONCE_ADMIN_DEL" "$BODY_REM_HASH")
SIG_REM=$(php_sign "$CANON_REM" "$ADMIN_SK")

CODE_REM=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php?members&channel=card" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_ADMIN_DEL" \
    -H "X-Drop-Signature: $SIG_REM" \
    -d "$BODY_REM")
assert_status "Admin PUT_MEMBERS removing MEMBER1 returns 204" "204" "$CODE_REM"

if [ ! -f "$DATA_DIR/${MEMBER1_PK}_firma.card.bin" ] && [ ! -f "$DATA_DIR/${MEMBER1_PK}.card.bin" ]; then
    echo "  [PASS] Removing member pruned both root and all prefixed slots (_firma) from disk"
else
    echo "  [FAIL] Prefixed or root slot still on disk after member removal" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

INDEX_AFTER_REM=$(curl -s "$BASE_URL/data/index.json")
if echo "$INDEX_AFTER_REM" | grep -q "$MEMBER1_PK"; then
    echo "  [FAIL] index.json still contains entries for removed MEMBER1: $INDEX_AFTER_REM" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
else
    echo "  [PASS] index.json contains zero entries for removed MEMBER1"
fi

echo ""
echo "=== Test Group 9: Server Revocation & Migration (PUT sync.php?revoke) ==="

# 9.1 Publish future revocation notice (grace period: deprecating)
TS=$(date +%s)
ISSUED_AT_1=$TS
EFFECTIVE_FUTURE=$((TS + 86400))
NEW_WRITE="https://new-sync.example.org/sync.php"
NEW_READ="https://new-sync.example.org/d-1111222233334444"
NEW_PUB="$MEMBER2_PK"
REASON_TXT="Umzug auf neuen Server"
NOTICE_CANON_1=$(printf "HUMOCO-DROP-REVOKE-V1\n%s\n%s\n%s\n%s\n%s\n%s\n%s" \
    "$ADMIN_PK" "$ISSUED_AT_1" "$EFFECTIVE_FUTURE" "$NEW_WRITE" "$NEW_READ" "$NEW_PUB" "$REASON_TXT")
NOTICE_SIG_1=$(php_sign "$NOTICE_CANON_1" "$ADMIN_SK")
BODY_REV_1=$(php -r '
echo json_encode([
    "drop_pub" => $argv[1],
    "issued_at" => (int)$argv[2],
    "effective_at" => (int)$argv[3],
    "reason" => $argv[4],
    "replacement" => [
        "write_url" => $argv[5],
        "read_url" => $argv[6],
        "drop_pub" => $argv[7],
    ],
    "signature" => $argv[8],
], JSON_UNESCAPED_SLASHES);
' "$ADMIN_PK" "$ISSUED_AT_1" "$EFFECTIVE_FUTURE" "$REASON_TXT" "$NEW_WRITE" "$NEW_READ" "$NEW_PUB" "$NOTICE_SIG_1")

NONCE_REV_1="da020202020202020202020202020202"
BODY_REV_1_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$BODY_REV_1")
CANON_ADM_1=$(printf "HUMOCO-DROP-ADMIN-V1\nPUT_REVOKE\n%s\n%s\n%s" "$TS" "$NONCE_REV_1" "$BODY_REV_1_HASH")
SIG_ADM_1=$(php_sign "$CANON_ADM_1" "$ADMIN_SK")

CODE_REV_1=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php?revoke" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_REV_1" \
    -H "X-Drop-Signature: $SIG_ADM_1" \
    -d "$BODY_REV_1")
assert_status "Valid future PUT ?revoke returns 200" "200" "$CODE_REV_1"

REV_JSON=$(curl -s "$BASE_URL/data/revocation.json")
if echo "$REV_JSON" | grep -q "$NEW_WRITE" && echo "$REV_JSON" | grep -q "$NOTICE_SIG_1"; then
    echo "  [PASS] Static revocation.json contains signed notice and replacement URL"
else
    echo "  [FAIL] Static revocation.json missing expected fields: $REV_JSON" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

INFO_REV=$(curl -s "$BASE_URL/sync.php?info")
if echo "$INFO_REV" | grep -q "$NEW_WRITE"; then
    echo "  [PASS] GET sync.php?info includes revocation notice"
else
    echo "  [FAIL] GET sync.php?info missing revocation notice: $INFO_REV" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# Server remains usable during grace period
CODE_HEAD_GRACE=$(curl -s -o /dev/null -w "%{http_code}" -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER2_PK" \
    -H "X-Channel: card")
assert_status "HEAD still returns 200 during grace period" "200" "$CODE_HEAD_GRACE"

# 9.3 Clear revocation with higher issued_at and effective_at: null
TS=$(date +%s)
ISSUED_AT_2=$((ISSUED_AT_1 + 1))
BODY_REV_CLR="{\"issued_at\":$ISSUED_AT_2,\"effective_at\":null}"
NONCE_REV_2="da030303030303030303030303030303"
BODY_REV_2_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$BODY_REV_CLR")
CANON_ADM_2=$(printf "HUMOCO-DROP-ADMIN-V1\nPUT_REVOKE\n%s\n%s\n%s" "$TS" "$NONCE_REV_2" "$BODY_REV_2_HASH")
SIG_ADM_2=$(php_sign "$CANON_ADM_2" "$ADMIN_SK")

CODE_REV_CLR=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php?revoke" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_REV_2" \
    -H "X-Drop-Signature: $SIG_ADM_2" \
    -d "$BODY_REV_CLR")
assert_status "Clearing revocation via effective_at: null returns 200" "200" "$CODE_REV_CLR"

if [ ! -f "$DATA_DIR/revocation.json" ]; then
    echo "  [PASS] revocation.json was removed after clearing"
else
    echo "  [FAIL] revocation.json still exists after clearing" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 9.4 Immediate revocation (effective_at <= now) enforces 410 Gone on HEAD/PUT/DELETE
TS=$(date +%s)
ISSUED_AT_3=$((ISSUED_AT_2 + 1))
EFFECTIVE_NOW=$((TS - 5))
NOTICE_CANON_3=$(printf "HUMOCO-DROP-REVOKE-V1\n%s\n%s\n%s\n%s\n%s\n%s\n%s" \
    "$ADMIN_PK" "$ISSUED_AT_3" "$EFFECTIVE_NOW" "$NEW_WRITE" "$NEW_READ" "$NEW_PUB" "Abgeschaltet")
NOTICE_SIG_3=$(php_sign "$NOTICE_CANON_3" "$ADMIN_SK")
BODY_REV_3=$(php -r '
echo json_encode([
    "drop_pub" => $argv[1],
    "issued_at" => (int)$argv[2],
    "effective_at" => (int)$argv[3],
    "reason" => "Abgeschaltet",
    "replacement" => [
        "write_url" => $argv[4],
        "read_url" => $argv[5],
        "drop_pub" => $argv[6],
    ],
    "signature" => $argv[7],
], JSON_UNESCAPED_SLASHES);
' "$ADMIN_PK" "$ISSUED_AT_3" "$EFFECTIVE_NOW" "$NEW_WRITE" "$NEW_READ" "$NEW_PUB" "$NOTICE_SIG_3")

NONCE_REV_3="da040404040404040404040404040404"
BODY_REV_3_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$BODY_REV_3")
CANON_ADM_3=$(printf "HUMOCO-DROP-ADMIN-V1\nPUT_REVOKE\n%s\n%s\n%s" "$TS" "$NONCE_REV_3" "$BODY_REV_3_HASH")
SIG_ADM_3=$(php_sign "$CANON_ADM_3" "$ADMIN_SK")

CODE_REV_3=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php?revoke" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_REV_3" \
    -H "X-Drop-Signature: $SIG_ADM_3" \
    -d "$BODY_REV_3")
assert_status "Immediate revocation PUT ?revoke returns 200" "200" "$CODE_REV_3"

CODE_HEAD_GONE=$(curl -s -o /dev/null -w "%{http_code}" -I "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER2_PK" \
    -H "X-Channel: card")
assert_status "HEAD returns 410 Gone after effective_at" "410" "$CODE_HEAD_GONE"

CODE_PUT_GONE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER2_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: da050505050505050505050505050505" \
    -H "X-Drop-Signature: $(printf '0%.0s' {1..128})" \
    -d "hello")
assert_status "PUT returns 410 Gone after effective_at" "410" "$CODE_PUT_GONE"

CODE_DEL_GONE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/sync.php" \
    -H "X-Drop-Pubkey: $MEMBER2_PK" \
    -H "X-Channel: card" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: da060606060606060606060606060606" \
    -H "X-Drop-Signature: $(printf '0%.0s' {1..128})")
assert_status "DELETE returns 410 Gone after effective_at" "410" "$CODE_DEL_GONE"

# 9.5 Self-redirect replacement in PUT ?revoke is rejected with 400
TS=$(date +%s)
ISSUED_AT_4=$((ISSUED_AT_3 + 1))
EFFECTIVE_FUTURE_4=$((TS + 3600))
SELF_WRITE="$BASE_URL/sync.php"
SELF_READ="$BASE_URL/data"
NOTICE_CANON_SELF=$(printf "HUMOCO-DROP-REVOKE-V1\n%s\n%s\n%s\n%s\n%s\n%s\n%s" \
    "$ADMIN_PK" "$ISSUED_AT_4" "$EFFECTIVE_FUTURE_4" "$SELF_WRITE" "$SELF_READ" "$ADMIN_PK" "Loop")
NOTICE_SIG_SELF=$(php_sign "$NOTICE_CANON_SELF" "$ADMIN_SK")
BODY_REV_SELF=$(php -r '
echo json_encode([
    "drop_pub" => $argv[1],
    "issued_at" => (int)$argv[2],
    "effective_at" => (int)$argv[3],
    "reason" => "Loop",
    "replacement" => [
        "write_url" => $argv[4],
        "read_url" => $argv[5],
        "drop_pub" => $argv[1],
    ],
    "signature" => $argv[6],
], JSON_UNESCAPED_SLASHES);
' "$ADMIN_PK" "$ISSUED_AT_4" "$EFFECTIVE_FUTURE_4" "$SELF_WRITE" "$SELF_READ" "$NOTICE_SIG_SELF")

NONCE_REV_SELF="da070707070707070707070707070707"
BODY_REV_SELF_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$BODY_REV_SELF")
CANON_ADM_SELF=$(printf "HUMOCO-DROP-ADMIN-V1\nPUT_REVOKE\n%s\n%s\n%s" "$TS" "$NONCE_REV_SELF" "$BODY_REV_SELF_HASH")
SIG_ADM_SELF=$(php_sign "$CANON_ADM_SELF" "$ADMIN_SK")

CODE_REV_SELF=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php?revoke" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_REV_SELF" \
    -H "X-Drop-Signature: $SIG_ADM_SELF" \
    -d "$BODY_REV_SELF")
assert_status "Self-redirect replacement in PUT ?revoke is rejected with 400" "400" "$CODE_REV_SELF"

# 9.6 Non-admin / invalid signature on PUT ?revoke is rejected with 403
TS=$(date +%s)
NONCE_REV_BAD="da010101010101010101010101010101"
BODY_REV_BAD='{"issued_at":100,"effective_at":null}'
BODY_REV_BAD_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$BODY_REV_BAD")
CANON_REV_BAD=$(printf "HUMOCO-DROP-ADMIN-V1\nPUT_REVOKE\n%s\n%s\n%s" "$TS" "$NONCE_REV_BAD" "$BODY_REV_BAD_HASH")
SIG_REV_BAD=$(php_sign "$CANON_REV_BAD" "$MEMBER2_SK")

CODE_REV_BAD=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/sync.php?revoke" \
    -H "X-Drop-Pubkey: $MEMBER2_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_REV_BAD" \
    -H "X-Drop-Signature: $SIG_REV_BAD" \
    -d "$BODY_REV_BAD")
assert_status "Non-admin PUT ?revoke is rejected with 403" "403" "$CODE_REV_BAD"

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
