#!/usr/bin/env bash
# ==============================================================================
# HuMoCo Lokaler Sync-Server (Development & Testing)
# ==============================================================================
# Startet einen lokalen PHP-Webserver im Verzeichnis dieses Skripts.
# Hier können die Dateien aus "humoco-sync-setup.zip" oder config.php
# direkt abgelegt und mit der HuMoCo-App getestet werden.
# ==============================================================================

set -eo pipefail

SERVER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SERVER_DIR"

# 1. PHP und Sodium-Prüfung
if ! command -v php >/dev/null 2>&1; then
    echo ""
    echo "❌ FEHLER: PHP ist auf diesem System noch nicht installiert."
    echo ""
    echo "Installiere es einfach mit folgendem Befehl:"
    echo "   sudo apt install -y php-cli php-sodium"
    echo ""
    exit 1
fi

SODIUM_CHECK=$(php -r 'echo function_exists("sodium_crypto_sign_detached") ? "ok" : "missing";' 2>/dev/null || echo "error")
if [ "$SODIUM_CHECK" != "ok" ]; then
    echo ""
    echo "❌ FEHLER: Die PHP-Erweiterung 'ext-sodium' fehlt."
    echo ""
    echo "Installiere sie mit:"
    echo "   sudo apt install -y php-sodium"
    echo ""
    exit 1
fi

# 2. Prüfen, ob sync.php existiert
if [ ! -f "$SERVER_DIR/sync.php" ]; then
    echo ""
    echo "⚠️  WARNUNG: Keine 'sync.php' in diesem Verzeichnis gefunden!"
    echo "   Verzeichnis: $SERVER_DIR"
    echo "   Bitte kopiere die sync.php hier hinein oder entpacke das Sync-ZIP hier."
    echo ""
fi

# 3. Prüfen, ob config.php existiert
CONFIG_STATUS="Gefunden"
if [ ! -f "$SERVER_DIR/config.php" ]; then
    CONFIG_STATUS="Noch nicht vorhanden (bitte config.php hier ablegen oder aus der App reinkopieren)"
fi

# 4. Freien Port ermitteln (Standard: 8080)
PORT="${PORT:-8080}"
is_port_in_use() {
    (echo >/dev/tcp/127.0.0.1/"$1") >/dev/null 2>&1
}

if is_port_in_use "$PORT"; then
    echo "Port $PORT ist bereits belegt. Suche freien Port..."
    for p in $(seq 8081 8100); do
        if ! is_port_in_use "$p"; then
            PORT="$p"
            break
        fi
    done
fi

BASE_URL="http://127.0.0.1:$PORT"
SYNC_URL="$BASE_URL/sync.php"

# 5. Rate-Limiter für lokale Entwicklung deaktivieren
export HUMOCO_DISABLE_RATE_LIMIT=1

# 6. Übersicht ausgeben
echo ""
echo "========================================================================"
echo " 🚀 HuMoCo Lokaler Sync-Server"
echo "========================================================================"
echo " 📂 Server-Verzeichnis: $SERVER_DIR"
echo " ⚙️  config.php:         $CONFIG_STATUS"
echo " 🛡️  IP-Rate-Limiter:    Deaktiviert (HUMOCO_DISABLE_RATE_LIMIT=1)"
echo "------------------------------------------------------------------------"
echo " 🔗 URL für die App (in Einstellungen -> Sync-Dienst einfügen):"
echo ""
echo "    $SYNC_URL"
echo ""
echo "    (Alternativ reicht oft auch: $BASE_URL)"
echo "========================================================================"
echo " Server läuft auf $BASE_URL (Beenden mit Strg+C)"
echo "------------------------------------------------------------------------"

# 7. PHP Built-In Server starten
exec php -S "127.0.0.1:$PORT" -t "$SERVER_DIR"
