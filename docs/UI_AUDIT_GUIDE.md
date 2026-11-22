# HuMoCo App — UI Tour & Audit Benutzerhandbuch

Dieses Handbuch beschreibt das UI-Tour- und Prüfsystem der HuMoCo App (`human-money-app`). Es ermöglicht dir, alle **31 Tour-Screens** (`1.1` bis `5.6`, verwaltet in [`scripts/tour-catalog.mjs`](../scripts/tour-catalog.mjs)) visuell zu testen, automatisierte Screenshot-Audits durchzuführen und IPC-/i18n-Fehler direkt mit dem KI-Assistenten zu beheben.

---

## Überblick: Die 3 Werkzeuge

| Werkzeug | Befehl / Methode | Zweck | Typischer Einsatz |
| :--- | :--- | :--- | :--- |
| **1. Interaktive Guided Tour** | `npm run tour` | Manuelles Durchklicken aller Screens mit Overlay-Steuerung | Visueller Check, Look & Feel, Mobile/Dark-Check |
| **2. Vollautomatischer Check** | `npm run tour:auto` | Headless-Durchlauf, Screenshots & automatischer Fehlerbericht | Schneller Regressionstest, Layout-Overflow & Console Errors |
| **3. KI-Logik- & IPC-Audit** | Code-Audit / `/ui-audit` | Statische Tiefenprüfung auf Tauri IPC, Locks, Catches & i18n | Nach Refactorings & Backend-Änderungen |

---

## 1. Interaktive Guided Tour (`npm run tour`)

Die Guided Tour startet die App im Browser-Mock-Modus mit einem schwebenden Steuerungs-Badge. Die maßgebliche Liste aller 31 Screens (`1.1`–`5.6` ohne `2.10`) liegt direkt in [`scripts/tour-catalog.mjs`](../scripts/tour-catalog.mjs).

```bash
cd human-money-app
npm run tour        # Alle Screens
npm run tour 4.2    # Direktsprung zu Schritt 4.2
```

- **Steuerung:** Pfeiltasten `←`/`→`, Buttons `◀`/`▶`, Screen-Dropdown, `🌙`/`☀️` (Dark/Light), `📱`/`🖥️` (Mobile 375px / Desktop), `📦` (Normal / Leer / Fehler).
- **Bereiche:**
  - `1.1–1.5`: Auth, Profil anlegen, Wiederherstellung & Recovery ([`src/components/auth/`](../src/components/auth/))
  - `2.1–2.9, 2.11`: Dashboard, Meine Gutscheine, Details, Erstellen, Senden/Empfangen, Bürgschaft & Historie ([`src/components/wallet/`](../src/components/wallet/), [`src/components/transfer/`](../src/components/transfer/))
  - `3.1–3.6`: Kontakte & Vertrauen + 3-Tab-Marktplatz & Visitenkarten ([`src/components/contacts/`](../src/components/contacts/), [`src/components/marketplace/`](../src/components/marketplace/))
  - `4.1–4.4`: Einstellungen (Profil, Sicherheit & Backup, Sync-Dienst, Erweitert) ([`src/components/settings/`](../src/components/settings/))
  - `5.1–5.6`: Konflikte, Fork-Lock, Hilfe-Center, Support & Bug-Report ([`src/components/conflicts/`](../src/components/conflicts/), [`src/components/support/`](../src/components/support/))

---

## 2. Vollautomatischer Durchlauf (`npm run tour:auto`)

```bash
cd human-money-app
npm run tour:auto
```

Besucht alle 31 Screens, speichert Screenshots unter `test-results/tour/<id>.png`, fängt Konsolenfehler & ungemockte Tauri-Calls ab, misst Mobile-/Desktop-Overflows und schreibt `test-results/tour/audit-report.md`.

---

## 3. KI-Logik-, i18n- & Schnittstellen-Audit (`/ui-audit`)

Prüfkommandos im App-Verzeichnis:

```bash
./run-tests.sh --quiet
npm run i18n:check
npm run lint
npx vitest run src/services/__tests__/tauriCommands.test.ts src/services/__tests__/commandAccessParity.test.ts src/services/__tests__/eventParity.test.ts src/services/__tests__/serviceReachability.test.ts
npx vitest run src/__tests__/glossaryLint.test.ts src/__tests__/uiModeLint.test.ts
```

Schwerpunkte bei manuellen Reviews:
1. **Tauri IPC & `AccessMode`:** `src/services/*.ts` ↔ `commandPolicy.ts` ↔ `src-tauri/src/commands/` (`camelCase` DTOs, 1:1 Argumente).
2. **Async-Sicherheit:** Lade-/Lock-Flags konsequent im `finally`-Block zurücksetzen; `SessionToken`- / Generation-Guards gegen Race Conditions.
3. **Fehler & Modal-Reset:** Keine stillen `catch {}`-Blöcke (`translateError(err, t)` nutzen); Passwörter und Formulardaten beim Schließen bereinigen.
4. **Terminologie & 5-Sprachen-i18n:** Alle 5 Locales (`en`, `de`, `es`, `fr`, `it`) synchron halten und [`docs/UX_GLOSSARY.md`](UX_GLOSSARY.md) einhalten.
