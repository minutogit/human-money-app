#!/bin/bash

# Dieses Skript führt alle Tests der Human Money App aus (Frontend & Backend).
clear
echo "🧪 Starte alle Tests..."

FAILED_TESTS=()


# 0. Tauri Version Check
echo "🔍 Prüfe Tauri Versionssynchronisation..."
npm run check-versions
if [ $? -ne 0 ]; then
    echo "❌ Tauri Versionscheck fehlgeschlagen!"
    FAILED_TESTS+=("Tauri Versionssynchronisation")
else
    echo "✅ Tauri Versionscheck erfolgreich."
fi


# 1. TypeScript Check
echo "🔍 Prüfe TypeScript Typen (Frontend)..."
npx tsc --noEmit
TSC_EXIT=$?
echo "🔍 Prüfe TypeScript Typen (Node/Configs)..."
npx tsc -p tsconfig.node.json --noEmit
TSC_NODE_EXIT=$?

if [ $TSC_EXIT -ne 0 ] || [ $TSC_NODE_EXIT -ne 0 ]; then
    echo "❌ TypeScript Check fehlgeschlagen!"
    FAILED_TESTS+=("TypeScript Check (Frontend/Node)")
else
    echo "✅ TypeScript Check erfolgreich."
fi


# 2. Lint Check (Frontend)
echo "🔍 Prüfe Code-Qualität (ESLint)..."
npm run lint
if [ $? -ne 0 ]; then
    echo "❌ Linting fehlgeschlagen!"
    FAILED_TESTS+=("Linting (ESLint)")
else
    echo "✅ Linting erfolgreich."
fi


# 3. Frontend Tests (Vitest)
echo "⚛️ Starte Frontend Komponententests (Vitest)..."
# DEBUG_PRINT_LIMIT=0 unterdrückt den riesigen DOM-Dump bei Fehlern
DEBUG_PRINT_LIMIT=0 npm test -- --run --reporter=verbose
if [ $? -ne 0 ]; then
    echo "❌ Frontend Tests fehlgeschlagen!"
    FAILED_TESTS+=("Frontend Tests (Vitest)")
else
    echo "✅ Frontend Tests erfolgreich."
fi


# 4. Backend Lint Check (Clippy)
echo "🦀 Prüfe Rust Code-Qualität (Clippy)..."
# In den src-tauri Ordner wechseln und clippy ausführen
(cd src-tauri && cargo clippy -- -D warnings)
if [ $? -ne 0 ]; then
    echo "❌ Clippy Check fehlgeschlagen!"
    FAILED_TESTS+=("Backend Linting (Clippy)")
else
    echo "✅ Clippy Check erfolgreich."
fi


# 5. Backend Tests (Cargo)
echo "🦀 Starte Backend Integrationstests (Rust)..."
(cd src-tauri && cargo test)
if [ $? -ne 0 ]; then
    echo "❌ Backend Tests fehlgeschlagen!"
    FAILED_TESTS+=("Backend Integrationstests (Rust)")
else
    echo "✅ Backend Tests erfolgreich."
fi

echo ""
echo "--------------------------------------------------"
if [ ${#FAILED_TESTS[@]} -eq 0 ]; then
    echo "🎉 ALLE TESTS BESTANDEN! 🎉"
    exit 0
else
    echo "❌ FOLGENDE TESTS SIND FEHLGESCHLAGEN:"
    for test in "${FAILED_TESTS[@]}"; do
        echo "   - $test"
    done
    echo "--------------------------------------------------"
    exit 1
fi
