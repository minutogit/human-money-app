#!/bin/bash

# Dieses Skript räumt alte Prozesse auf und startet die Tauri-Entwicklungsumgebung.
clear
echo "🧹 Suche und beende alte Vite & Tauri Prozesse..."

# Überprüfe, welche Prozesse gefunden werden
echo "Aktive Vite-Prozesse vor dem Beenden:"
ps aux | grep vite | grep -v grep || echo "Keine Vite-Prozesse gefunden"

echo "Aktive Tauri-Prozesse vor dem Beenden:"
ps aux | grep tauri | grep -v grep || echo "Keine Tauri-Prozesse gefunden"

# Beendet laufende Vite- und Tauri-CLI-Prozesse, ohne bei Fehlschlag abzubrechen.
pkill -f 'vite' || true
pkill -f 'tauri-cli' || true
pkill -f 'cargo.*tauri' || true

# Kurze Pause, um das System Zeit zum Beenden zu geben
sleep 3

# Überprüfe erneut
echo "Aktive Vite-Prozesse nach dem Beenden:"
ps aux | grep vite | grep -v grep || echo "Keine Vite-Prozesse mehr aktiv"

echo "Aktive Tauri-Prozesse nach dem Beenden:"
ps aux | grep tauri | grep -v grep || echo "Keine Tauri-Prozesse mehr aktiv"

echo "✅ Aufräumen beendet."

echo "📦 Prüfe und installiere Frontend-Abhängigkeiten..."
npm install

echo "🚀 Starte jetzt die Tauri App..."

# Startet die Tauri-Entwicklungsumgebung
tauri dev