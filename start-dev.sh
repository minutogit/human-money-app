#!/bin/bash

# Dieses Skript räumt alte Prozesse auf und startet die Tauri-Entwicklungsumgebung.

echo "🧹 Suche und beende alte Vite & Tauri Prozesse..."

# Beendet laufende Vite- und Tauri-CLI-Prozesse, ohne bei Fehlschlag abzubrechen.
pkill -f 'vite' || true
pkill -f 'tauri-cli' || true

echo "✅ Aufräumen beendet."
echo "🚀 Starte jetzt die Tauri App..."

# Startet die Tauri-Entwicklungsumgebung
tauri dev