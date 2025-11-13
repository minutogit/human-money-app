Dieses Dokument dient als "Single Source of Truth" für alle Architekturentscheidungen und technologischen Vorgaben beim Bau des Tauri-basierten Wallet-Clients.

**1. Projektziel & Umfang**

* **Ziel:** Entwicklung eines plattformübergreifenden Desktop-Prototyps (Windows, macOS, Linux) für das dezentrale Gutschein-System.
* **Funktion:** Die Anwendung dient als Client-Frontend für die `voucher_lib`.
* **Anforderung:** Das UI muss responsiv sein, um eine spätere Portierung auf mobile Plattformen zu erleichtern.

**2. Architektur & Technologiestack**
Die Anwendung folgt einer strikten Trennung von Backend und Frontend.

**Backend (Rust - src-tauri)**

* **Framework:** Tauri
* **Kernlogik:** Nutzt ausschließlich die `AppService`-Fassade aus der `voucher_lib`.
* **Rolle:** Dient als dünne Brücke, die die `AppService`-Funktionen an das Frontend weiterleitet. Es wird keine neue Geschäftslogik implementiert.
* **State Management:** Eine Instanz des `AppService` wird im globalen Tauri-State gehalten.
* **Schnittstelle:** Funktionen werden über das `#[tauri::command]`-Makro für das Frontend zugänglich gemacht. Rust `Result`-Typen werden in für das Frontend verständliche Fehler umgewandelt.
* **Kommandomodulstruktur:** Die Tauri-Befehle sind in mehrere Module aufgeteilt:
  * `commands/auth.rs`: Authentifizierungsbezogene Befehle
  * `commands/queries.rs`: Abfragebezogene Befehle
  * `commands/actions.rs`: Aktionen wie Gutscheinerstellung und Transaktionen
  * `commands/utils.rs`: Hilfsfunktionen und Logging
  * `settings.rs`: Konfigurations- und Einstellungsverwaltung
* **Datenmodelle:** Zusätzliche Datenstrukturen in `models.rs` dienen zum Austausch zwischen Frontend und Backend.

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
Die folgenden Funktionen der `voucher_lib` sollen implementiert werden:

* **Profil & Login:**
  * `create_profile`
  * `login`
  * `recover_wallet_and_set_new_password`
  * `logout`
  * `generate_mnemonic`
  * `validate_mnemonic`
  * `list_profiles` (neu für Multi-Profile-Unterstützung)
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
├── .dev/
│   ├── llm-context-vouchercore-api.md
│   ├── llm-context.md
│   ├── projektplan.md
│   └── voucher-example.json
├── .gitignore
├── .taurignore
├── generate_tree.sh
├── index.html
├── package.json
├── public
│   ├── tauri.svg
│   └── vite.svg
├── README.md
├── src
│   ├── App.css
│   ├── App.tsx
│   ├── assets
│   │   └── react.svg
│   ├── components
│   │   ├── CreateNewProfile.tsx
│   │   ├── CreateVoucher.tsx
│   │   ├── Dashboard.tsx
│   │   ├── Login.tsx
│   │   ├── ReceiveSuccessView.tsx
│   │   ├── ReceiveView.tsx
│   │   ├── RecreateProfile.tsx
│   │   ├── SendView.tsx
│   │   ├── SettingsView.tsx
│   │   ├── TransactionHistoryView.tsx
│   │   ├── TransferSuccessView.tsx
│   │   │   ├── ui
│   │   │   │   ├── Button.tsx
│   │   │   │   ├── Input.tsx
│   │   │   │   └── Textarea.tsx
│   │   ├── VoucherDetailsView.tsx
│   │   └── WalletRecovery.tsx
│   ├── main.tsx
│   ├── types.ts
│   ├── utils
│   │   └── log.ts
│   └── vite-env.d.ts
├── src-tauri
│   ├── build.rs
│   ├── capabilities
│   │   └── default.json
│   ├── Cargo.lock
│   ├── Cargo.toml
│   ├── icons
│   │   ├── 128x128.png
│   │   ├── 128x128@2x.png
│   │   ├── 32x32.png
│   │   ├── icon.icns
│   │   ├── icon.ico
│   │   ├── icon.png
│   │   ├── Square107x107Logo.png
│   │   ├── Square142x142Logo.png
│   │   ├── Square150x150Logo.png
│   │   ├── Square284x284Logo.png
│   │   ├── Square30x30Logo.png
│   │   ├── Square310x310Logo.png
│   │   ├── Square44x44Logo.png
│   │   ├── Square71x71Logo.png
│   │   ├── Square89x89Logo.png
│   │   └── StoreLogo.png
│   ├── src
│   │   ├── commands
│   │   │   ├── actions.rs
│   │   │   ├── auth.rs
│   │   │   ├── mod.rs
│   │   │   ├── queries.rs
│   │   │   └── utils.rs
│   │   ├── lib.rs
│   │   ├── main.rs
│   │   ├── models.rs
│   │   └── settings.rs
│   ├── .gitignore
│   └── tauri.conf.json
├── start-dev.sh
├── tailwind.config.js
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
└── voucher_standards
    ├── minuto_v1
    │   └── standard.toml
    └── silver_v1
        └── standard.toml

```
├── index.html
├── package.json
├── public
│   ├── tauri.svg
│   └── vite.svg
├── README.md
├── src
│   ├── App.css
│   ├── App.tsx
│   ├── assets
│   │   └── react.svg
│   ├── components
│   │   ├── CreateNewProfile.tsx
│   │   ├── CreateVoucher.tsx
│   │   ├── Dashboard.tsx
│   │   ├── Login.tsx
│   │   ├── ReceiveSuccessView.tsx
│   │   ├── ReceiveView.tsx
│   │   ├── RecreateProfile.tsx
│   │   ├── SendView.tsx
│   │   ├── SettingsView.tsx
│   │   ├── TransactionHistoryView.tsx
│   │   ├── TransferSuccessView.tsx
│   │   ├── ui
│   │   │   ├── Button.tsx
│   │   │   ├── Input.tsx
│   │   │   └── Textarea.tsx
│   │   ├── VoucherDetailsView.tsx
│   │   └── WalletRecovery.tsx
│   ├── main.tsx
│   ├── types.ts
│   ├── utils
│   │   └── log.ts
│   └── vite-env.d.ts
├── src-tauri
│   ├── build.rs
│   ├── Cargo.lock
│   ├── Cargo.toml
│   ├── src
│   │   ├── commands
│   │   │   ├── actions.rs
│   │   │   ├── auth.rs
│   │   │   ├── mod.rs
│   │   │   ├── queries.rs
│   │   │   └── utils.rs
│   │   ├── lib.rs
│   │   ├── main.rs
│   │   ├── models.rs
│   │   └── settings.rs
│   └── tauri.conf.json
├── start-dev.sh
├── tailwind.config.js
├── .taurignore
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
└── voucher_standards
    ├── minuto_v1
    │   └── standard.toml
    └── silver_v1
        └── standard.toml

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
  * `src/lib.rs`: Hauptdatei mit allen Tauri-Befehlen, die die `voucher_lib::AppService`-Fassade nutzen; erweitert um neue Befehle wie `get_bip39_wordlist`, `save_transaction_record`, `get_app_settings`, `save_app_settings`
  * `src/main.rs`: Einstiegspunkt, der die `run()`-Funktion aus `lib.rs` aufruft
  * `src/commands/actions.rs`: Implementierung von Voucher-Aktionen wie `create_new_voucher`, `create_transfer_bundle`, `receive_bundle`, `save_transaction_record`
  * `src/commands/auth.rs`: Authentifizierungsbezogene Befehle wie `create_profile`, `login`, `logout`, `list_profiles`
  * `src/commands/queries.rs`: Abfragebezogene Befehle wie `get_voucher_summaries`, `get_voucher_details`, `get_transaction_history`
  * `src/commands/utils.rs`: Hilfsfunktionen wie `generate_mnemonic`, `get_voucher_standards`, `get_bip39_wordlist`, `frontend_log`, `log_to_backend`
  * `src/models.rs`: Datenstrukturdefinitionen für den Austausch zwischen Frontend und Backend; neue Strukturen wie `NominalValueData`, `FrontendAddressData`, `FrontendCollateralData`, `FrontendCreatorData`, `FrontendNewVoucherData`
  * `src/settings.rs`: Implementierung der Einstellungs- und Konfigurationsverwaltung mit Speicherung in verschlüsselter Datei; `AppSettings` mit `bundle_retention_days`

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

* **Multi-Profile-Unterstützung:** Das System unterstützt nun mehrere Benutzerprofile, die über ein Auswahlinterface verwaltet werden können. Beim Erstellen eines Profils kann ein menschenlesbarer Profilname vergeben werden, und bei der Anmeldung erfolgt eine Profilauswahl.
* **Bundle-Empfangsworkflow:** Neue Komponenten `ReceiveView` und `ReceiveSuccessView` ermöglichen den Empfang von Transfer-Bundles über Dateidialog oder Drag & Drop. Der `dragDropEnabled`-Parameter in `tauri.conf.json` wurde auf `false` gesetzt, um native Browser-Events zu ermöglichen.
* **Send-Workflow und Transaktionshistorie:** Neue Komponenten `SendView`, `TransactionHistoryView` und `TransferSuccessView` implementieren den kompletten Workflow zum Versenden von Gutscheinen und zur Anzeige der Transaktionshistorie.
* **In-Memory-Caching:** Die Transaktionshistorie und App-Einstellungen werden nach der Anmeldung einmal entschlüsselt und im Speicher gecached, um wiederholte Passwortabfragen zu vermeiden.
* **Automatisches Bereinigen:** Beim Login werden automatisch abgelaufene Transfer-Bundle-Daten aus dem Verlauf gelöscht, basierend auf dem konfigurierten Aufbewahrungszeitraum.
* **Log-Rotation:** Implementierung einer Log-Rotationsfunktion, die beim Start die Größe der Logdatei prüft und diese bei Überschreitung eines definierten Limits kürzt. Logging wird zusätzlich in eine Datei im Anwendungs-Log-Verzeichnis geschrieben.
* **Erweiterte Logging-Utility:** Neue Frontend-Logging-Funktionen (`logger.info`, `logger.warn`, `logger.error`) senden Logs direkt ins Rust-Terminal für besseres Debugging.