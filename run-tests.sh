#!/bin/bash

# Dieses Skript führt alle Tests der Human Money App aus (Frontend & Backend).

QUIET=false
FRONTEND_ONLY=false
BACKEND_ONLY=false

while [[ "$#" -gt 0 ]]; do
    case $1 in
        -q|--quiet) QUIET=true ;;
        -f|--frontend-only) FRONTEND_ONLY=true ;;
        -b|--backend-only) BACKEND_ONLY=true ;;
        -h|--help)
            echo "Nutzung: $0 [Optionen]"
            echo "Optionen:"
            echo "  -q, --quiet          Quiet-Modus (zeigt nur Fehler an, spart Token)"
            echo "  -f, --frontend-only  Führt nur Frontend-Tests/Checks aus"
            echo "  -b, --backend-only   Führt nur Backend-Tests/Checks aus"
            echo "  -h, --help           Zeigt diese Hilfe an"
            exit 0
            ;;
        *) echo "Unbekannte Option: $1"; exit 1 ;;
    esac
    shift
done

# Wenn weder frontend-only noch backend-only explizit gesetzt wurde, führen wir beides aus
if [ "$FRONTEND_ONLY" = false ] && [ "$BACKEND_ONLY" = false ]; then
    RUN_FRONTEND=true
    RUN_BACKEND=true
else
    RUN_FRONTEND=$FRONTEND_ONLY
    RUN_BACKEND=$BACKEND_ONLY
fi

if [ "$QUIET" = false ]; then
    clear
    echo "🧪 Starte alle Tests..."
fi

FAILED_TESTS=()
LOG_FILE=$(mktemp)

# Sicherstellen, dass das Temp-File beim Beenden gelöscht wird
trap 'rm -f "$LOG_FILE"' EXIT

run_step() {
    local step_name="$1"
    local cmd="$2"

    if [ "$QUIET" = true ]; then
        ( eval "$cmd" ) > "$LOG_FILE" 2>&1
        local status=$?
        if [ $status -ne 0 ]; then
            echo "❌ $step_name fehlgeschlagen! Details:"
            cat "$LOG_FILE"
            echo "--------------------------------------------------"
            exit 1
        fi

        # Suche nach Warnungen (z.B. ESLint warning, rustc warning, npm WARN)
        if grep -E -i "warning:|warning[[:space:]]+|npm WARN|warn:" "$LOG_FILE" >/dev/null 2>&1; then
            echo "⚠️  Warnungen in $step_name gefunden! Details:"
            cat "$LOG_FILE"
            echo "--------------------------------------------------"
        fi
    else
        echo "🔍 Führe aus: $step_name..."
        ( eval "$cmd" )
        local status=$?
        if [ $status -ne 0 ]; then
            echo "❌ $step_name fehlgeschlagen!"
            FAILED_TESTS+=("$step_name")
        else
            echo "✅ $step_name erfolgreich."
        fi
        echo ""
    fi
}

# 0. Tauri Version Check (immer wenn Frontend oder Backend ausgeführt wird, da es beide vergleicht)
run_step "Tauri Versionssynchronisation" "npm run check-versions"

if [ "$RUN_FRONTEND" = true ]; then
    # 1. TypeScript Check (Frontend)
    run_step "TypeScript Check (Frontend)" "npx tsc --noEmit"
    
    # 2. TypeScript Check (Node/Configs)
    run_step "TypeScript Check (Node/Configs)" "npx tsc -p tsconfig.node.json --noEmit"
    
    # 3. Lint Check (Frontend)
    run_step "Linting (ESLint)" "npm run lint"
    
    # 4. Frontend Tests (Vitest)
    run_step "Frontend Tests (Vitest)" "DEBUG_PRINT_LIMIT=0 npm test -- --run --reporter=verbose"
fi

if [ "$RUN_BACKEND" = true ]; then
    # 5. Backend Lint Check (Clippy)
    run_step "Backend Linting (Clippy)" "cd src-tauri && cargo clippy -- -D warnings"
    
    # 6. Backend Tests (Cargo)
    run_step "Backend Integrationstests (Rust)" "cd src-tauri && cargo test"
fi

if [ "$QUIET" = false ]; then
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
else
    # Quiet mode success
    echo "🎉 Alle Tests erfolgreich bestanden! 🎉"
    exit 0
fi
