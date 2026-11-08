#!/usr/bin/env bash
#
# Integration and unit tests for HuMoCo Drop Server (drop.php)
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

# Check if php binary is available
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
echo " HuMoCo Drop Server Test Suite (PHP $(php -r 'echo PHP_VERSION;'))"
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
TEST_DIR="$(mktemp -d /tmp/humoco-drop-test.XXXXXX)"
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

# Copy drop.php and support files into isolated test environment
cp "$TOOL_DIR/drop.php" "$TEST_DIR/drop.php"
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

# Write test config.php: max_members = 2, max_bytes = 128
cat <<EOF > "$TEST_DIR/config.php"
<?php
return [
    'admin_pubkey'         => '$ADMIN_PK',
    'data_dir'             => 'data',
    'max_members'          => 2,
    'max_total_storage_mb' => 1,
    'channels' => [
        'card' => ['max_bytes' => 128, 'ttl_days' => 30],
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

# 1.1 GET drop.php without ?info -> 405 Method Not Allowed
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X GET "$BASE_URL/drop.php")
assert_status "GET drop.php is rejected with 405" "405" "$CODE"

# 1.2 POST drop.php without ?rotate -> 405 Method Not Allowed
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/drop.php" -d "foo=bar")
assert_status "POST drop.php without rotate is rejected with 405" "405" "$CODE"

# 1.3 GET drop.php?info -> 200 OK
INFO_JSON=$(curl -s "$BASE_URL/drop.php?info")
if echo "$INFO_JSON" | grep -q "$ADMIN_PK" && echo "$INFO_JSON" | grep -q '"channels":\["card"\]'; then
    echo "  [PASS] GET drop.php?info returned correct admin_pubkey and channel list"
else
    echo "  [FAIL] GET drop.php?info response invalid: $INFO_JSON" >&2
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
HEAD_CODE=$(curl -s -o /dev/null -w "%{http_code}" -I "$BASE_URL/drop.php" \
    -H "X-Drop-Pubkey: $MEMBER1_PK" \
    -H "X-Channel: card")
assert_status "HEAD with unlisted pubkey returns 403" "403" "$HEAD_CODE"

echo ""
echo "=== Test Group 3: Admin Member List Update (PUT drop.php?members) ==="

TS=$(date +%s)
NONCE1="0123456789abcdef0123456789abcdef"
BODY="{\"pubkeys\":[\"$MEMBER1_PK\",\"$MEMBER2_PK\"]}"
BODY_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$BODY")
CANON_MSG=$(printf "HUMOCO-DROP-ADMIN-V1\nPUT_MEMBERS\ncard\n%s\n%s\n%s" "$TS" "$NONCE1" "$BODY_HASH")
SIG=$(php_sign "$CANON_MSG" "$ADMIN_SK")

# 3.1 PUT with wrong signature -> 403
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php?members&channel=card" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE1" \
    -H "X-Drop-Signature: 00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000" \
    -d "$BODY")
assert_status "Admin PUT with forged signature returns 403" "403" "$CODE"

# 3.2 Valid Admin PUT -> 204
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php?members&channel=card" \
    -H "X-Drop-Pubkey: $ADMIN_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE1" \
    -H "X-Drop-Signature: $SIG" \
    -d "$BODY")
assert_status "Valid Admin PUT returns 204" "204" "$CODE"

# 3.3 HEAD now succeeds for MEMBER1 -> 200
HEAD_RESP=$(curl -s -I "$BASE_URL/drop.php" \
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
PUT_RESP=$(curl -s -i -X PUT "$BASE_URL/drop.php" \
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

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php" \
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
echo "=== Test Group 5: 180-Day Key Rotation (POST drop.php?rotate) ==="

RETIRE_AT=$((TS + 180 * 86400))
CERT_CANON=$(printf "HUMOCO-DROP-ROTATE-V1\n%s\n%s\n%d" "$ADMIN_PK" "$ADMIN2_PK" "$RETIRE_AT")
CERT_SIG=$(php_sign "$CERT_CANON" "$ADMIN_SK")

ROT_BODY=$(printf '{"old_pubkey":"%s","new_pubkey":"%s","retire_at":%d,"cert_sig":"%s"}' \
    "$ADMIN_PK" "$ADMIN2_PK" "$RETIRE_AT" "$CERT_SIG")
ROT_HASH=$(php -r 'echo hash("sha256", $argv[1]);' "$ROT_BODY")
NONCE_ROT="55555555666666667777777788888888"
CANON_ROT=$(printf "HUMOCO-DROP-ADMIN-V1\nROTATE\n\n%s\n%s\n%s" "$TS" "$NONCE_ROT" "$ROT_HASH")
SIG_ROT=$(php_sign "$CANON_ROT" "$ADMIN2_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/drop.php?rotate" \
    -H "X-Drop-Pubkey: $ADMIN2_PK" \
    -H "X-Drop-Timestamp: $TS" \
    -H "X-Drop-Nonce: $NONCE_ROT" \
    -H "X-Drop-Signature: $SIG_ROT" \
    -d "$ROT_BODY")
assert_status "Admin key rotation returns 200" "200" "$CODE"

# Verify GET ?info now returns ADMIN2_PK and rotation header
INFO_AFTER=$(curl -s -i "$BASE_URL/drop.php?info")
if echo "$INFO_AFTER" | grep -q "$ADMIN2_PK" && echo "$INFO_AFTER" | grep -qi "X-Humoco-Drop-Rotation:"; then
    echo "  [PASS] Rotation header present and new admin_pubkey active"
else
    echo "  [FAIL] Rotation header or admin_pubkey invalid: $INFO_AFTER" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 6: Member Slot Delete (DELETE drop.php) ==="

TS=$(date +%s)
NONCE_DEL="99999999aaaaaaaa99999999aaaaaaaa"
EMPTY_HASH="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
CANON_DEL=$(printf "HUMOCO-DROP-REQ-V1\nDELETE\ncard\n%s\n%s\n%s" "$TS" "$NONCE_DEL" "$EMPTY_HASH")
SIG_DEL=$(php_sign "$CANON_DEL" "$MEMBER1_SK")

CODE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/drop.php" \
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
