---
name: app-context
description: Full architecture overview, component details, implemented features, Tailwind setup, and Tauri command documentation for the Human Money App.
---

# Human Money App — Full Context

Dieses Dokument dient als "Single Source of Truth" für alle Architekturentscheidungen und technologischen Vorgaben beim Bau des Tauri-basierten Wallet-Clients.

**1. Projektziel & Umfang**

* **Ziel:** Entwicklung eines plattformübergreifenden Desktop-Prototyps (Windows, macOS, Linux) für das dezentrale Gutschein-System.
* **Funktion:** Die Anwendung dient als Client-Frontend für die `human_money_core`.
* **Anforderung:** Das UI muss responsiv sein, um eine spätere Portierung auf mobile Plattformen zu erleichtern.

**2. Architektur & Technologiestack**
Die Anwendung folgt einer strikten Trennung von Backend und Frontend.

**Backend (Rust - src-tauri)**

* **Framework:** Tauri
* **Kernlogik:** Nutzt ausschließlich die `AppService`-Fassade aus der `human_money_core`.
* **Rolle:** Dient als dünne Brücke, die die `AppService`-Funktionen an das Frontend weiterleitet. Es wird keine neue Geschäftslogik implementiert.
* **State Management:** Eine Instanz des `AppService` wird im globalen Tauri-State gehalten.
* **Schnittstelle:** Funktionen werden über das `#[tauri::command]`-Makro für das Frontend zugänglich gemacht. Rust `Result`-Typen werden in für das Frontend verständliche Fehler umgewandelt.
* **Kommandomodulstruktur:** Die Tauri-Befehle sind in mehrere Module aufgeteilt:
  * `commands/auth.rs`: Authentifizierungsbezogene Befehle
  * `commands/queries.rs`: Abfragebezogene Befehle
  * `commands/actions.rs`: Aktionen wie Gutscheinerstellung und Transaktionen
  * `commands/contacts.rs`: Adressbuch- und Kontaktverwaltung
  * `commands/integrity.rs`: Sicherheitsprüfungen und Integritätsberichte
  * `commands/utils.rs`: Hilfsfunktionen und Logging
  * `settings.rs`: Konfigurations- und Einstellungsverwaltung
* **Datenmodelle:** Die Datenstrukturen in `src-tauri/src/models/` sind modularisiert:
  * `models/voucher.rs`: Gutscheinbezogene DTOs
  * `models/wallet.rs`: Wallet- und Guthaben-DTOs
  * `models/profile.rs`: Profilbezogene DTOs
  * `models/events.rs`: Event-Sourcing-bezogene DTOs
  * `models/conflicts.rs`: Double-Spend-Konflikt-DTOs

**Frontend (React - src)**

* **Framework:** React mit TypeScript
* **Kommunikation:** Die Interaktion mit dem Backend erfolgt ausschließlich über die `invoke`-Funktion aus der `@tauri-apps/api/core`-Bibliothek.
* **State Management:** Für den Prototyp wird auf Einfachheit gesetzt und Reacts nativer `useState`-Hook verwendet.

**Styling-Konfiguration (Tailwind CSS)**

* **Technologie:** Das Styling wird vollständig mit Tailwind CSS umgesetzt.
* **Integration:** Die Einbindung in das Vite-Build-System erfolgt **ausschließlich** über das offizielle Plugin `@tailwindcss/vite`. Eine `postcss.config.js` darf **nicht** verwendet werden, um Konflikte zu vermeiden.
* **Setup in CSS:** Die zentrale CSS-Datei (`src/App.css`) muss Tailwind über die Direktive `@import "tailwindcss";` importieren.
* **Laden der Konfiguration:** Die Projektkonfiguration `tailwind.config.js` (im Projekt-Stammverzeichnis) muss explizit über `@config "../tailwind.config.js";` in `src/App.css` geladen werden. Der relative Pfad (`../`) ist hierbei entscheidend.
* **Plugins:** Das Projekt nutzt `@tailwindcss/postcss` und `autoprefixer` für erweiterte Funktionalitäten.

**3. Kernfunktionalität (MVP)**
Die folgenden Funktionen der `human_money_core` sollen implementiert werden:

* **Profil & Login:**
  * `create_profile`
  * `login`
  * `recover_wallet_and_set_new_password`
  * `logout`
  * `is_session_active` (neu für UI-Session-Status)
  * `generate_mnemonic`
  * `validate_mnemonic`
  * `list_profiles`
* **Dashboard-Anzeige:**
  * `get_user_id`
  * `get_total_balance_by_currency`
  * `get_voucher_summaries`
  * `get_voucher_details`
* **Transaktionen:**
  * `create_transfer_bundle`
  * `receive_bundle`
  * `get_transaction_history`
  * `save_transaction_record`
  * `save_transaction_bundle`
  * `cleanup_expired_bundles`
* **Gutschein-Management:**
  * `create_new_voucher`
  * `get_voucher_standards`
  * `remove_voucher_signature`
* **Kontaktverwaltung:**
  * `list_contacts`
  * `save_contact`
  * `delete_contact`
* **Sicherheit & Integrität:**
  * `get_integrity_report`
  * `check_for_forks`
* **Einstellungen:**
  * `get_app_settings`
  * `save_app_settings`
* **Hilfsfunktionen:**
  * `get_bip39_wordlist`
  * `frontend_log`
  * `log_to_backend`

**4. Datentypen & Fehlerbehandlung**

* **Typsicherheit:** Für alle Datenstrukturen, die zwischen Rust und dem Frontend ausgetauscht werden (z. B. `VoucherSummary`), müssen entsprechende TypeScript-Interfaces definiert werden. Neue Interfaces umfassen `Address`, `CreatorData`, `CollateralData`, `ValueDefinition`, `Collateral`, `PublicProfile`, `VoucherTemplateData`, `SourceTransfer`, `MultiTransferRequest`, `InvolvedVoucherInfo` und erweiterte Strukturen für Voucher-Erstellung und Transaktionen.
* **Fehlerbehandlung:** Jeder `invoke`-Aufruf im Frontend muss robust in einem `try/catch`-Block gekapselt werden, um Fehler aus dem Rust-Backend abzufangen und dem Nutzer verständliches Feedback zu geben.

**5. Aktuelle Dateistruktur**
Dies ist der aktuelle Zustand des Projekts mit den implementierten Komponenten.

```
.
├── .agents/
├── .cargo/
├── .github/
├── .dev/
├── public/
├── src/
│   ├── assets/
│   ├── components/
│   │   ├── ui/                 ← Basis-UI-Komponenten (Button, Input, Card, etc.)
│   │   ├── Activities.tsx      ← Transaktionsaktivitäten
│   │   ├── AddressBook.tsx     ← Kontaktverwaltung
│   │   ├── CreateNewProfile.tsx
│   │   ├── CreateVoucher.tsx
│   │   ├── Dashboard.tsx
│   │   ├── IntegrityReportModal.tsx
│   │   ├── Login.tsx
│   │   ├── ReceiveView.tsx
│   │   ├── SendView.tsx
│   │   ├── Sidebar.tsx
│   │   ├── VoucherDetailsView.tsx
│   │   └── WalletView.tsx
│   ├── context/               ← SessionContext & State
│   ├── utils/                 ← Helfer (format, log, userIdHelper)
│   ├── types.ts               ← Zentrale TypeScript-Interfaces
│   └── App.tsx                ← Routing & Main Layout
├── src-tauri/
│   ├── src/
│   │   ├── commands/          ← Tauri Command Handler
│   │   ├── models/            ← Frontend-Backend DTOs (modular)
│   │   ├── lib.rs             ← Tauri Setup
│   │   └── settings.rs        ← App-Konfiguration
│   └── Cargo.toml
├── voucher_standards/         ← Standard-Definitionen (Minuto, FreeTaler)
└── STATUS.md                  ← Aktueller Projektstatus
```

**6. Implementierte Kernfunktionen**

* **Frontend (src/)**
  * `App.tsx`: Hauptkomponente mit State-Management für die Anwendungsphasen (Profil erstellen, Login, Dashboard, Passwort zurücksetzen, Gutschein erstellen, Gutschein Details anzeigen, Senden, Empfangen, Einstellungen)
  * `components/Dashboard.tsx`: Zeigt Benutzer-ID, Guthaben und Gutscheinübersicht an
  * `components/CreateProfile.tsx`: Formular zur Profilerstellung mit Mnemonic-Generierung und Profilnameneingabe
  * `components/Login.tsx`: Login-Formular mit Profilauswahl und Passwort-Eingabe
  * `components/VoucherDetailsView.tsx`: Detaillierte Ansicht für einen einzelnen Gutschein mit allen Eigenschaften, Transaktionshistorie und Signaturinformationen
  * `components/CreateVoucher.tsx`: Formular zur Erstellung neuer Gutscheine mit umfassenden Details zum Ersteller und zur Besicherung
  * `components/WalletRecovery.tsx`: Formular zur Wiederherstellung des Wallets mit Profilauswahl und Mnemonic
  * `components/SendView.tsx`: Komponente zum Erstellen und Vorbereiten von Transfer-Bundles
  * `components/ReceiveView.tsx`: Komponente zum Empfangen von Transfer-Bundles mit Datei-Dialog und Drag & Drop Unterstützung
  * `components/TransactionHistoryView.tsx`: Zeigt vergangene Sende-Transaktionen an
  * `components/SettingsView.tsx`: Ermöglicht die Konfiguration von Anwendungseinstellungen wie Bundle-Retention-Period
  * `components/TransferSuccessView.tsx`: Zeigt Erfolgsmeldung nach erfolgreichem Versand und ermöglicht Bundle-Speicherung
  * `components/ReceiveSuccessView.tsx`: Zeigt Zusammenfassung nach erfolgreichem Empfang einer Transaktion
  * `types.ts`: TypeScript-Interfaces für `VoucherSummary`, `VoucherDetails`, `NewVoucherData`, `TransactionHistoryEntry`, `ProfileInfo` und andere komplexe Datentypen
  * `utils/log.ts`: Frontend-Logging-Utility für konsistentes Logging ins Backend

* **Backend (src-tauri/)**
  * `src/lib.rs`: Hauptdatei mit allen Tauri-Befehlen und Plugin-Registrierungen.
  * `src/commands/actions.rs`: Gutschein-Aktionen (Create, Transfer, Receive).
  * `src/commands/auth.rs`: Profil-Management und Authentifizierung.
  * `src/commands/contacts.rs`: Adressbuch-Logik.
  * `src/commands/integrity.rs`: Fork-Erkennung und Integritätsprüfung.
  * `src/commands/queries.rs`: Abfragen für Dashboards und Details.
  * `src/models/`: Modularisierte DTOs für die IPC-Kommunikation (camelCase serialisiert).
  * `src/settings.rs`: Anwendungsweite Einstellungen.

**7. Logging**

* **Frontend Logging:** Zum Debuggen von Frontend-Komponenten kann ein spezielles Logging-System verwendet werden, das Logs direkt in das Rust-Terminal sendet:
  * Verwendung der neuen Logging-Utility: `import { logger } from "../utils/log";` und dann z.B. `logger.info("Nachricht")`, `logger.warn("Warnung")`, `logger.error("Fehler")`
  * Diese Funktionen nutzen den `log_to_backend`-Befehl, der eine spezielle Funktion im Rust-Backend aufruft, die die Nachricht mit dem Rust-Logger ausgibt und somit im Terminal sichtbar macht.
  * Beispiel:
    ```typescript
    import { logger } from "../utils/log";
    
    useEffect(() => {
        logger.info("Komponente wurde geladen");
        // Deine Logik hier
    }, []);
    ```
  * Alternativ kann weiterhin der direkte Befehl verwendet werden: `invoke("frontend_log", { message: "Deine Log-Nachricht" }).catch(console.error);`

**8. Weitere Features**

* **Sicherheits-Banner & Integrität:** Die App prüft beim Login und bei Aktionen auf "Forks" (Double-Spending-Versuche). Bei Erkennung wird ein prominentes Banner angezeigt, das zu einem detaillierten Integritätsbericht führt.
* **In-Memory-Caching:** Transaktions-Aktivitäten und Profileinstellungen werden im Speicher gehalten, um die UI-Reaktivität zu erhöhen.
* **Address Book Integration:** Kontakte können gespeichert und direkt im Send-Flow ausgewählt werden.
* **Voucher Card UI:** Eine zentrale Komponente zur konsistenten Darstellung von Gutscheinen in allen Listenansichten.