# HuMoCo App — UI Tour & Audit Benutzerhandbuch

Dieses Handbuch beschreibt das UI-Tour- und Prüfsystem der HuMoCo App (`human-money-app`). Es ermöglicht dir, die Benutzeroberfläche visuell zu testen, Schritt für Schritt durchzugehen, automatisierte Screenshot-Audits durchzuführen und Fehler direkt mit dem KI-Assistenten zu beheben.

---

## Überblick: Die 3 Werkzeuge

| Werkzeug | Befehl / Methode | Zweck | Typischer Einsatz |
| :--- | :--- | :--- | :--- |
| **1. Interaktive Guided Tour** | `npm run tour` | Manuelles Durchklicken aller Screens mit Overlay-Steuerung | Visueller Check, Look & Feel, Feedback per Schrittnummer |
| **2. Vollautomatischer Check** | `npm run tour:auto` | Headless-Durchlauf, Screenshots & automatischer Fehlerbericht | Schneller Regressionstest, Broken Layouts & Console Errors |
| **3. KI-Logik- & IPC-Audit** | Code-Audit / `/ui-audit` | Statische Tiefenprüfung auf Tauri IPC, Locks, Catches & i18n | Nach Refactorings & Backend-Änderungen |

---

## 1. Interaktive Guided Tour (`npm run tour`)

Die Guided Tour startet die App in einem interaktiven Browser-Modus mit einem schwebenden Steuerungs-Badge am oberen Bildschirmrand. Alle 26 Kern-Screens sind mit festen Nummern (`1.1` bis `5.6`) versehen.

### Starten

Wechsle in das App-Verzeichnis und starte die Tour:

```bash
cd human-money-app
npm run tour
```

Du kannst auch direkt zu einem bestimmten Schritt springen, indem du die Schritt-ID anhängst:

```bash
npm run tour 4.2
# oder
npm run tour 2.5
```

### Steuerung im Browser

Das am oberen Rand sichtbare Tour-Badge bietet intuitive Kontrollmöglichkeiten:

- **Tastatursteuerung:**
  - `←` (Pfeil links): Vorheriger Screen
  - `→` (Pfeil rechts): Nächster Screen
- **Maus / Touch im Badge:**
  - `◀` / `▶` Buttons zum Vor- und Zurückblättern
  - Dropdown-Menü zur direkten Auswahl eines beliebigen Screens (`1.1` bis `5.6`)
- **Modus-Umschalter:**
  - `🌙` / `☀️`: Umschalten zwischen **Dark Mode** und **Light Mode**
  - `📱` / `🖥️`: Umschalten zwischen **Mobile-Ansicht** (375px Breite) und **Desktop-Ansicht**
  - `📦` Szenario-Umschalter: Wechselt zwischen Mock-Daten:
    - **Normal:** Typischer Wallet-Zustand mit Gutscheinen, Transaktionen und Kontakten
    - **Leer (Empty State):** Frisches Profil ohne Gutscheine, Kontakte oder Historie
    - **Fehler (Error State):** Simuliert Netzwerk-, Validierungs- oder Synchronisationsprobleme

---

### Vollständige Tabelle aller Tour-Schritte (`1.1` bis `5.6`)

| ID | Bereich / Ansicht | Quellcode-Komponente | Typischer Prüfschwerpunkt |
| :---: | :--- | :--- | :--- |
| **1.1** | **Login-Ansicht** | [`src/components/Login.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/Login.tsx) | PIN-/Passworteingabe, Fehlermeldung bei falscher PIN, Profilwechsel |
| **1.2** | **Neues Profil erstellen** | [`src/components/CreateNewProfile.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/CreateNewProfile.tsx) | Name, Passwort-Vergabe, Seed-Generierung & Sicherheitswarnung |
| **1.3** | **Profil aus Seed wiederherstellen** | [`src/components/RecreateProfile.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/RecreateProfile.tsx) | Mnemonic-Wort-Eingabe, Validierung, Fehler-Feedback |
| **1.4** | **Backup-Datei wiederherstellen** | [`src/components/RestoreBackupView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/RestoreBackupView.tsx) | File-Upload / DropZone, Passphrase, Integritäts-Check |
| **1.5** | **Passwort vergessen / Wallet-Recovery** | [`src/components/WalletRecovery.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/WalletRecovery.tsx) | Notfall-Wiederherstellung, Seed-Abfrage, neues Passwort |
| **2.1** | **Dashboard / Startübersicht** | [`src/components/Dashboard.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/Dashboard.tsx) | Saldenübersicht, Schnellaktionen (Senden/Empfangen), Sync-Status |
| **2.2** | **Wallet / Gutscheinliste** | [`src/components/WalletView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/WalletView.tsx) | Gutscheinkarten, Filter (aktiv/abgelaufen), Sortierung, Empty State |
| **2.3** | **Gutschein-Details** | [`src/components/VoucherDetailsView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/VoucherDetailsView.tsx) | ProofChain-Verlauf, Gültigkeit, Bürgen, Split- & Transfer-Aktionen |
| **2.4** | **Neuen Gutschein erstellen** | [`src/components/CreateVoucher.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/CreateVoucher.tsx) | Betrag, Einheit (Minuto/FreeTaler), Gültigkeitsdauer, Bürgenwahl |
| **2.5** | **Gutschein senden** | [`src/components/SendView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/SendView.tsx) | Empfängerauswahl, QR-Code, Offline-BLE/Datei-Export, Betrags-Split |
| **2.6** | **Gutschein-Versand erfolgreich** | [`src/components/TransferSuccessView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/TransferSuccessView.tsx) | Erfolgs-Quittung, neuer Saldo, Teilen-/Drucken-Button |
| **2.7** | **Gutschein / Bundle empfangen** | [`src/components/ReceiveView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/ReceiveView.tsx) | QR-Scanner, Kamera, Datei-Import, Paste von Payload |
| **2.8** | **Empfang erfolgreich** | [`src/components/ReceiveSuccessView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/ReceiveSuccessView.tsx) | Gutschein-Validierung, Signaturprüfung, Hinzugefügt zum Saldo |
| **2.9** | **Bürgschafts- / Signaturanfrage** | [`src/components/SignRequestView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/SignRequestView.tsx) | Aussteller-Profil, Betrag, Bürgschaftsbedingungen, Bestätigen/Ablehnen |
| **2.10** | **Aktivitäten & Verlauf** | [`src/components/Activities.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/Activities.tsx) | Chronologische Liste aller Transaktionen, Sync-Events & Bürgschaften |
| **2.11** | **Transaktionshistorie** | [`src/components/TransactionHistoryView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/TransactionHistoryView.tsx) | Detaillierte Transaktionsfilter, Datumsbereich, Export (CSV/JSON) |
| **3.1** | **Adressbuch & Kontakte** | [`src/components/AddressBook.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/AddressBook.tsx) | Kontaktkarten, Vertrauensstufen (L0-L3), Notizen, Kontakt hinzufügen |
| **3.2** | **Marktplatz & Kontaktkarten** | [`src/components/marketplace/MarketplaceView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/marketplace/MarketplaceView.tsx) | Angebote & Gesuche, Eigene Angebote, Kontaktkarten-Austausch |
| **4.1** | **Einstellungen: Profil & Allgemein** | [`src/components/SettingsView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/SettingsView.tsx) | Anzeigename, Sprache (DE/EN), Farbschema, Währungseinheiten |
| **4.2** | **Einstellungen: Sync Service & Kanäle** | [`src/components/settings/SyncServiceSettingsCard.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/settings/SyncServiceSettingsCard.tsx) | Sync-Server URL, Intervall, Quiet Mode, Shard-Verbindungsstatus |
| **4.3** | **Einstellungen: Backup & Export** | [`src/components/BackupSettingsCard.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/BackupSettingsCard.tsx) | Automatisches Backup, Manuelles Backup, Seed-Phrase anzeigen |
| **4.4** | **Einstellungen: Netzwerk & Standards** | [`src/components/SettingsView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/SettingsView.tsx) | Standard-Definitionen, Quorum-Einstellungen, Node-Verbindungen |
| **5.1** | **Konflikt-Übersicht** | [`src/components/ConflictListView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/ConflictListView.tsx) | Erkannte Double-Spends, SST-Trap-Funde, betroffene Gutscheine |
| **5.2** | **Konflikt-Details** | [`src/components/ConflictDetailsView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/ConflictDetailsView.tsx) | Beweiskette, aufgedeckte Identität (DID), Quorum-Beweis |
| **5.3** | **Fork-Lock Sicherheits-Sperre** | [`src/components/ForkLockOverlay.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/ForkLockOverlay.tsx) | Notfall-Sperre bei Fork-Erkennung, Entsperr- und Sync-Anweisungen |
| **5.4** | **Konzept & Hilfe** | [`src/components/ConceptView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/ConceptView.tsx) | Erklärungen zu Minuto, Web of Trust, Offline-Sicherheit, FAQ |
| **5.5** | **Support & Spenden** | [`src/components/SupportView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/SupportView.tsx) | Entwickler unterstützen, Links zu GitHub, Community-Kanäle |
| **5.6** | **Bug-Report** | [`src/components/BugReportView.tsx`](file:///home/basti/Coding/humoco/human-money-app/src/components/BugReportView.tsx) | Diagnose-Daten exportieren, Fehlerbericht formulieren |

---

### Copy-Paste Prompt für Feedback nach der Tour

Kopiere diese Vorlage in den Chat mit der KI, um Feedback nach dem Durchklicken schnell und strukturiert zu übergeben:

```markdown
Ich habe die UI-Tour durchgeklickt. Bitte behebe folgende Punkte:
- Bei 4.2: Das Sync-Status-Badge überlappt auf mobiler Breite mit dem Speicher-Button.
- Bei 2.5: Im Send-Screen fehlt eine klare Abbruch-Möglichkeit (Cancel-Button), wenn man keinen Empfänger gewählt hat.
- Bei 2.2: Wenn keine Gutscheine vorhanden sind (Empty State), ist der Text zu nah am oberen Rand abgeschnitten.
- Bei 1.1: Im Dark Mode ist der Kontrast des Platzhalter-Texts im Passwort-Feld zu gering.

Bitte prüfe die entsprechenden Komponenten, passe sie an und stelle sicher, dass alle Vitest-Tests und der Build weiterhin sauber durchlaufen.
```

---

## 2. Vollautomatischer Durchlauf & KI-Design-Prüfung (`npm run tour:auto`)

Der automatische Modus fährt die App ohne Benutzerinteraktion im Headless-Modus ab:
1. Er besucht jeden Screen von `1.1` bis `5.6`.
2. Er speichert Screenshots unter `human-money-app/test-results/tour/<id>.png`.
3. Er fängt JavaScript-Konsolenfehler (`console.error`, unhandled rejections) und ungemockte Tauri-IPC-Aufrufe ab.
4. Er misst Layout-Überläufe (horizontales Scrollen des Bodys, abgeschnittene Boxen).
5. Er generiert den zusammenfassenden Bericht `human-money-app/test-results/tour/audit-report.md`.

### Ausführung

```bash
cd human-money-app
npm run tour:auto
```

### Copy-Paste Prompt für die KI

Nutze diesen Prompt im Chat, damit der Assistent die Tour selbstständig ausführt und analysiert:

```markdown
Bitte führe im Verzeichnis `human-money-app` den automatischen UI-Audit via `npm run tour:auto` aus.
Lies anschließend den erzeugten `test-results/tour/audit-report.md` und prüfe die generierten Screenshots:
1. Gibt es Layout-Fehler, Clipping oder überlappende Elemente?
2. Stimmt die visuelle Hierarchie (klare primäre Buttons, verständliche Struktur)?
3. Gibt es ungemockte IPC-Aufrufe oder Fehler in der Konsole?
Behebe alle gefundenen Mängel direkt im Code und verifiziere mit `npm run test` und `npm run build`.
```

---

## 3. KI-Logik- & Schnittstellen-Test (Code-Audit)

Der Code-Audit ist eine statische Tiefenprüfung von Frontend- und Tauri-Schnittstellen. Er sollte nach jedem größeren Feature, vor Releases oder nach Änderungen an `src-tauri` ausgeführt werden.

### Die 6 Prüfkriterien

1. **Tauri IPC-Vertrag:** Stimmen alle `invoke('command', { camelCaseArgs })` mit den Rust `#[tauri::command]` Parametern überein?
   - Testbefehl: `npx vitest run src/services/__tests__/tauriCommands.test.ts`
2. **Lade- & Lock-Sicherheit (Async Finally):** Wird jedes `setIsLoading(true)` / `setIsSaving(true)` ausnahmslos im `finally`-Block zurückgesetzt, damit die UI bei Backend-Fehlern nicht einfriert?
3. **Keine stillen Catches:** Fängt kein `catch {}` Fehler lautlos ab? Werden Fehler dem Nutzer über `translateError` oder Banner angezeigt?
4. **Modal- & Tab-Reset:** Werden Passwörter, Fehlermeldungen und Formulare beim Schließen eines Modals oder Tab-Wechsels sauber zurückgesetzt?
5. **Race Conditions & Cleanup:** Haben asynchrone `useEffect`-Aufrufe ein Abbruch-Flag (`let active = true; ... return () => { active = false; };`)?
6. **i18n-Vollständigkeit:** Sind alle Texte über `t('key')` übersetzt und keine Hardcoded-Strings vorhanden?
   - Testbefehl: `npm run i18n:check`

### Copy-Paste Prompt für die KI

```markdown
Bitte führe einen tiefen UI- und IPC-Code-Audit für die kürzlich geänderten Komponenten in `human-money-app` durch (gemäß Skill `ui-audit`):
1. Prüfe die Tauri-IPC-Verträge und führe `npx vitest run src/services/__tests__/tauriCommands.test.ts` aus.
2. Stelle sicher, dass alle Lade- und Speicherschalter in `finally`-Blöcken zurückgesetzt werden.
3. Eliminiere alle stillen `catch {}`-Blöcke und stelle sicher, dass Fehler angezeigt werden.
4. Prüfe Modals und Tabs auf sauberen State-Reset beim Schließen.
5. Führe `npm run i18n:check` aus und behebe fehlende Übersetzungen.
```
