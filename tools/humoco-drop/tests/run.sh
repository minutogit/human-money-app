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

# Check minimum PHP version (>= 7.4)
php -r '
if (version_compare(PHP_VERSION, "7.4.0", "<")) {
    fwrite(STDERR, "ERROR: PHP version >= 7.4 required, found " . PHP_VERSION . "\n");
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

# Generate cryptographically secure test tokens via PHP CLI
ADMIN_TOKEN="$(php -r 'echo bin2hex(random_bytes(32));')"
ADMIN_TOKEN_HASH="$(php -r 'echo hash("sha256", hex2bin("'"$ADMIN_TOKEN"'"));')"

MEMBER1_TOKEN="$(php -r 'echo bin2hex(random_bytes(32));')"
MEMBER1_ID="$(php -r 'echo substr(hash("sha256", hex2bin("'"$MEMBER1_TOKEN"'")), 0, 32);')"

MEMBER2_TOKEN="$(php -r 'echo bin2hex(random_bytes(32));')"
MEMBER2_ID="$(php -r 'echo substr(hash("sha256", hex2bin("'"$MEMBER2_TOKEN"'")), 0, 32);')"

MEMBER3_TOKEN="$(php -r 'echo bin2hex(random_bytes(32));')"
MEMBER3_ID="$(php -r 'echo substr(hash("sha256", hex2bin("'"$MEMBER3_TOKEN"'")), 0, 32);')"

# Write test config.php: max_members = 2 (to test 507), max_bytes = 128 (to test 413)
cat <<EOF > "$TEST_DIR/config.php"
<?php
return [
    'admin_token_hash' => '$ADMIN_TOKEN_HASH',
    'data_dir'         => 'data',
    'max_members'      => 2,
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

echo ""
echo "=== Test Group 1: Method & Routing Invariants ==="

# 1.1 GET drop.php -> 405 Method Not Allowed
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X GET "$BASE_URL/drop.php")
assert_status "GET drop.php is rejected with 405" "405" "$CODE"

# 1.2 POST drop.php -> 405 Method Not Allowed
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/drop.php" -d "foo=bar")
assert_status "POST drop.php is rejected with 405" "405" "$CODE"

# 1.3 PUT drop.php without any headers -> 404
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php" -d "payload")
assert_status "PUT drop.php without drop/admin tokens returns 404" "404" "$CODE"

# 1.4 DELETE drop.php without tokens -> 404
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/drop.php")
assert_status "DELETE drop.php without drop token returns 404" "404" "$CODE"

echo ""
echo "=== Test Group 2: Admin Member List Update (PUT drop.php?members) ==="

# 2.1 PUT with wrong admin token -> 404
WRONG_ADMIN="0000000000000000000000000000000000000000000000000000000000000000"
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php?members&channel=card" \
    -H "X-Admin-Token: $WRONG_ADMIN" \
    -d "{\"ids\":[\"$MEMBER1_ID\"]}")
assert_status "Admin PUT with invalid token returns 404" "404" "$CODE"

# 2.2 PUT with unconfigured channel -> 404
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php?members&channel=nonexistent" \
    -H "X-Admin-Token: $ADMIN_TOKEN" \
    -d "{\"ids\":[\"$MEMBER1_ID\"]}")
assert_status "Admin PUT with unconfigured channel returns 404" "404" "$CODE"

# 2.3 PUT exceeding max_members (configured max = 2, sending 3 IDs) -> 507
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php?members&channel=card" \
    -H "X-Admin-Token: $ADMIN_TOKEN" \
    -d "{\"ids\":[\"$MEMBER1_ID\",\"$MEMBER2_ID\",\"$MEMBER3_ID\"]}")
assert_status "Admin PUT exceeding max_members returns 507" "507" "$CODE"

# 2.4 PUT with malformed ID (non-hex or wrong length) -> 400
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php?members&channel=card" \
    -H "X-Admin-Token: $ADMIN_TOKEN" \
    -d '{"ids":["not-a-valid-hex-32-chars-length"]}')
assert_status "Admin PUT with malformed ID returns 400" "400" "$CODE"

# 2.5 Valid Admin PUT with MEMBER1 and MEMBER2 -> 204
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php?members&channel=card" \
    -H "X-Admin-Token: $ADMIN_TOKEN" \
    -d "{\"ids\":[\"$MEMBER1_ID\",\"$MEMBER2_ID\"]}")
assert_status "Valid Admin PUT returns 204" "204" "$CODE"

# Verify static read of members.card.json
MEMBERS_CONTENT=$(curl -s "$BASE_URL/data/members.card.json")
if echo "$MEMBERS_CONTENT" | grep -q "$MEMBER1_ID" && echo "$MEMBERS_CONTENT" | grep -q "$MEMBER2_ID"; then
    echo "  [PASS] Static read of members.card.json contains both member IDs"
else
    echo "  [FAIL] Static read of members.card.json does not contain expected IDs: $MEMBERS_CONTENT" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 3: Member Slot Upload & Static Reads ==="

# 3.1 Unlisted member (MEMBER3) upload -> 404
SAMPLE_B64=$(php -r 'echo base64_encode("HuMoCo test slot content for Member 1");')
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php" \
    -H "X-Drop-Token: $MEMBER3_TOKEN" \
    -H "X-Channel: card" \
    -d "$SAMPLE_B64")
assert_status "Unlisted member upload is rejected with 404" "404" "$CODE"

# 3.2 Listed member upload to unconfigured channel -> 404
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php" \
    -H "X-Drop-Token: $MEMBER1_TOKEN" \
    -H "X-Channel: unknown_channel" \
    -d "$SAMPLE_B64")
assert_status "Upload to unconfigured channel returns 404" "404" "$CODE"

# 3.3 Listed member upload with invalid base64 -> 400
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php" \
    -H "X-Drop-Token: $MEMBER1_TOKEN" \
    -H "X-Channel: card" \
    -d "invalid-base64-payload!!!")
assert_status "Upload with invalid base64 body returns 400" "400" "$CODE"

# 3.4 Listed member upload exceeding max_bytes (limit is 128 bytes, payload is 129 bytes) -> 413
OVERSIZED_B64=$(php -r 'echo base64_encode(str_repeat("X", 129));')
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php" \
    -H "X-Drop-Token: $MEMBER1_TOKEN" \
    -H "X-Channel: card" \
    -d "$OVERSIZED_B64")
assert_status "Upload exceeding channel max_bytes returns 413" "413" "$CODE"

# 3.5 Valid upload for MEMBER1 -> 204
RAW_PAYLOAD="Human Money Voucher Card: Alice - Minuto 50"
MEMBER1_PAYLOAD_B64=$(php -r 'echo base64_encode("'"$RAW_PAYLOAD"'");')
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php" \
    -H "X-Drop-Token: $MEMBER1_TOKEN" \
    -H "X-Channel: card" \
    -d "$MEMBER1_PAYLOAD_B64")
assert_status "Valid member slot upload returns 204" "204" "$CODE"

# 3.6 Static read of uploaded slot: exact byte parity
FETCHED_PAYLOAD=$(curl -s "$BASE_URL/data/$MEMBER1_ID.card.bin")
if [ "$FETCHED_PAYLOAD" = "$RAW_PAYLOAD" ]; then
    echo "  [PASS] Static read of $MEMBER1_ID.card.bin matches uploaded bytes exactly"
else
    echo "  [FAIL] Static read content mismatch (expected '$RAW_PAYLOAD', got '$FETCHED_PAYLOAD')" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# 3.7 Static read of index.json contains MEMBER1 entry
INDEX_JSON=$(curl -s "$BASE_URL/data/index.json")
if echo "$INDEX_JSON" | grep -q "$MEMBER1_ID" && echo "$INDEX_JSON" | grep -q '"channel":"card"'; then
    echo "  [PASS] Static read of index.json contains slot metadata for $MEMBER1_ID"
else
    echo "  [FAIL] index.json missing expected metadata: $INDEX_JSON" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 4: Slot Isolation (Derivation Parity) ==="

# 4.1 MEMBER2 uploads their own slot
MEMBER2_PAYLOAD="Human Money Voucher Card: Bob - FreeTaler 100"
MEMBER2_PAYLOAD_B64=$(php -r 'echo base64_encode("'"$MEMBER2_PAYLOAD"'");')
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php" \
    -H "X-Drop-Token: $MEMBER2_TOKEN" \
    -H "X-Channel: card" \
    -d "$MEMBER2_PAYLOAD_B64")
assert_status "MEMBER2 valid slot upload returns 204" "204" "$CODE"

# Verify MEMBER1 slot was NOT modified or overwritten
VERIFY_M1=$(curl -s "$BASE_URL/data/$MEMBER1_ID.card.bin")
VERIFY_M2=$(curl -s "$BASE_URL/data/$MEMBER2_ID.card.bin")
if [ "$VERIFY_M1" = "$RAW_PAYLOAD" ] && [ "$VERIFY_M2" = "$MEMBER2_PAYLOAD" ]; then
    echo "  [PASS] Slot isolation verified: Member 1 and Member 2 slots exist independently"
else
    echo "  [FAIL] Slot isolation violated! Member 1 or Member 2 slot corrupted." >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 5: Immediate Slot Cleanup on Member Removal ==="

# 5.1 Admin updates member list keeping only MEMBER1 (MEMBER2 is removed)
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php?members&channel=card" \
    -H "X-Admin-Token: $ADMIN_TOKEN" \
    -d "{\"ids\":[\"$MEMBER1_ID\"]}")
assert_status "Admin removes MEMBER2 from member list (returns 204)" "204" "$CODE"

# Verify MEMBER2 slot file has been purged from disk immediately
if [ ! -f "$DATA_DIR/$MEMBER2_ID.card.bin" ]; then
    echo "  [PASS] Removed member's slot file ($MEMBER2_ID.card.bin) was deleted from disk"
else
    echo "  [FAIL] Removed member's slot file still exists on disk!" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# Verify index.json no longer lists MEMBER2
INDEX_AFTER_REMOVAL=$(curl -s "$BASE_URL/data/index.json")
if ! echo "$INDEX_AFTER_REMOVAL" | grep -q "$MEMBER2_ID"; then
    echo "  [PASS] index.json no longer lists removed member slot"
else
    echo "  [FAIL] index.json still lists removed member: $INDEX_AFTER_REMOVAL" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 6: TTL Purge Behavior ==="

# 6.1 Re-add MEMBER2 so both are allowed
curl -s -o /dev/null -X PUT "$BASE_URL/drop.php?members&channel=card" \
    -H "X-Admin-Token: $ADMIN_TOKEN" \
    -d "{\"ids\":[\"$MEMBER1_ID\",\"$MEMBER2_ID\"]}"

# Re-upload MEMBER2 slot
curl -s -o /dev/null -X PUT "$BASE_URL/drop.php" \
    -H "X-Drop-Token: $MEMBER2_TOKEN" \
    -H "X-Channel: card" \
    -d "$MEMBER2_PAYLOAD_B64"

# Artificially age MEMBER1's slot to 31 days ago (TTL is 30 days)
touch -d "31 days ago" "$DATA_DIR/$MEMBER1_ID.card.bin"
echo "[i] Set mtime of $MEMBER1_ID.card.bin to 31 days ago"

# Trigger a mutating write operation: re-upload MEMBER2 slot
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "$BASE_URL/drop.php" \
    -H "X-Drop-Token: $MEMBER2_TOKEN" \
    -H "X-Channel: card" \
    -d "$MEMBER2_PAYLOAD_B64")
assert_status "Mutating write triggers TTL purge (returns 204)" "204" "$CODE"

# Verify expired slot was purged by TTL
if [ ! -f "$DATA_DIR/$MEMBER1_ID.card.bin" ]; then
    echo "  [PASS] Expired slot was automatically purged by TTL"
else
    echo "  [FAIL] Expired slot ($MEMBER1_ID.card.bin) was not purged by TTL!" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# Verify index.json only lists MEMBER2
INDEX_AFTER_TTL=$(curl -s "$BASE_URL/data/index.json")
if ! echo "$INDEX_AFTER_TTL" | grep -q "$MEMBER1_ID" && echo "$INDEX_AFTER_TTL" | grep -q "$MEMBER2_ID"; then
    echo "  [PASS] index.json correctly purged expired entry and retained active entry"
else
    echo "  [FAIL] index.json does not reflect TTL purge: $INDEX_AFTER_TTL" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 7: Member Self-Deletion (DELETE drop.php) ==="

# 7.1 MEMBER2 deletes own slot
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/drop.php" \
    -H "X-Drop-Token: $MEMBER2_TOKEN" \
    -H "X-Channel: card")
assert_status "Member slot deletion returns 204" "204" "$CODE"

# Verify slot is gone from disk
if [ ! -f "$DATA_DIR/$MEMBER2_ID.card.bin" ]; then
    echo "  [PASS] Slot file removed from disk upon DELETE"
else
    echo "  [FAIL] Slot file still exists after DELETE!" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

# Verify index.json is empty array []
INDEX_FINAL=$(curl -s "$BASE_URL/data/index.json")
if [ "$INDEX_FINAL" = "[]" ]; then
    echo "  [PASS] index.json is empty ([]) after all slots deleted"
else
    echo "  [FAIL] index.json is not empty: $INDEX_FINAL" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== Test Group 8: Cross-Language Test Vectors Parity ==="

VECTOR_PATH="$TOOL_DIR/../../crates/humoco-market/tests/vectors/drop_access.json"
if [ -f "$VECTOR_PATH" ]; then
    echo "[i] Verifying drop_access test vectors from $VECTOR_PATH..."
    VECTOR_RESULT=$(php -r '
        $file = $argv[1];
        $data = json_decode(file_get_contents($file), true);
        if (!is_array($data)) {
            fwrite(STDERR, "Malformed vector file\n");
            exit(1);
        }
        $count = 0;
        foreach ($data as $vec) {
            $tokenHex = isset($vec["write_token_hex"]) ? $vec["write_token_hex"] : ($vec["token_hex"] ?? null);
            $expectedId = isset($vec["access_id"]) ? $vec["access_id"] : null;
            if ($tokenHex && $expectedId) {
                $calcId = substr(hash("sha256", hex2bin($tokenHex)), 0, 32);
                if ($calcId !== $expectedId) {
                    fwrite(STDERR, "Mismatch for token $tokenHex: expected $expectedId, got $calcId\n");
                    exit(1);
                }
                $count++;
            }
        }
        echo "OK:$count";
    ' "$VECTOR_PATH" 2>&1)
    if echo "$VECTOR_RESULT" | grep -q '^OK:'; then
        echo "  [PASS] Verified cross-language parity against drop_access.json ($VECTOR_RESULT)"
    else
        echo "  [FAIL] Cross-language vector check failed: $VECTOR_RESULT" >&2
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
else
    echo "  [INFO] Optional drop_access.json vectors file not present yet; skipped cross-language vector test."
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
