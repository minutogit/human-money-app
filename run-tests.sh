#!/bin/bash

# Dieses Skript führt alle Tests der Human Money App aus (Frontend & Backend).
clear
echo "🧪 Starte alle Tests..."

# 0. Tauri Version Check
echo "🔍 Prüfe Tauri Versionssynchronisation..."
npm run check-versions
if [ $? -ne 0 ]; then
    echo "❌ Tauri Versionscheck fehlgeschlagen!"
    exit 1
fi
echo "✅ Tauri Versionscheck erfolgreich."

# 1. TypeScript Check
echo "🔍 Prüfe TypeScript Typen (Frontend)..."
npx tsc --noEmit
if [ $? -ne 0 ]; then
    echo "❌ TypeScript Check fehlgeschlagen!"
    exit 1
fi
echo "✅ TypeScript Check erfolgreich."

# 2. Frontend Tests (Vitest)
echo "⚛️ Starte Frontend Komponententests (Vitest)..."
# DEBUG_PRINT_LIMIT=0 unterdrückt den riesigen DOM-Dump bei Fehlern
DEBUG_PRINT_LIMIT=0 npm test -- --run --reporter=verbose
if [ $? -ne 0 ]; then
    echo "❌ Frontend Tests fehlgeschlagen!"
    exit 1
fi
echo "✅ Frontend Tests erfolgreich."

# 3. Backend Tests (Cargo)
echo "🦀 Starte Backend Integrationstests (Rust)..."
cd src-tauri
cargo test
if [ $? -ne 0 ]; then
    echo "❌ Backend Tests fehlgeschlagen!"
    exit 1
fi
echo "✅ Backend Tests erfolgreich."

echo ""
echo "🎉 ALLE TESTS BESTANDEN! 🎉"
