#!/bin/bash

# Generiert einen Verzeichnisbaum, der für die KI-Analyse optimiert ist.
# Ignoriert temporäre Build-Verzeichnisse, IDE-Konfigurationen,
# Paketmanager-Caches und andere irrelevante Dateien/Ordner.

# Die Muster für auszuschließende Verzeichnisse und Dateien.
# WICHTIG: Hier werden nur NAMEN, keine Pfade angegeben.
EXCLUDES=(
    ".dev"
    ".idea"
    "node_modules"
    "target" # KORREKTUR: Nur der Name, nicht der Pfad "src-tauri/target"
    "gen"
    "icons"
    "capabilities"
    "dist"
    "pkg"
    ".git"
    ".vscode"
    "package-lock.json"
    "generate_tree.sh"
    ".gitignore"
    "wallet_data"
)

# Erstelle den endgültigen String für die -I Option mit printf (robuster als echo).
# 1. Erstellt einen String mit '|' am Ende jedes Elements.
# 2. Entfernt das letzte überflüssige '|' am Ende des Strings.
EXCLUDE_STRING=$(printf "%s|" "${EXCLUDES[@]}")
EXCLUDE_STRING=${EXCLUDE_STRING%|}

echo "Generating file tree, excluding: ${EXCLUDE_STRING}"
echo "---"

# Führe den tree-Befehl mit den Ausschlussmustern aus.
# -a: Zeigt alle Dateien, einschließlich versteckter.
# -I: Schließt Verzeichnisse aus, die dem Muster entsprechen.
# --prune: Schneidet die Ausgabe für ausgeschlossene Verzeichnisse ab,
#          damit deren Inhalt nicht mehr durchsucht wird.
tree -a --prune -I "${EXCLUDE_STRING}"