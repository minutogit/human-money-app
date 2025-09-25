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
  * `commands/actions.rs`: Aktionen wie Gutscheinerstellung
  * `commands/utils.rs`: Hilfsfunktionen und Logging
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
* **Dashboard-Anzeige:**
  * `get_user_id`
  * `get_total_balance_by_currency`
  * `get_voucher_summaries`
  * `get_voucher_details`
* **Transaktionen:**
  * `create_transfer_bundle`
  * `receive_bundle`
* **Gutschein-Management:**
  * `create_new_voucher`
  * `get_voucher_standards`

**4. Datentypen & Fehlerbehandlung**

* **Typsicherheit:** Für alle Datenstrukturen, die zwischen Rust und dem Frontend ausgetauscht werden (z. B. `VoucherSummary`), müssen entsprechende TypeScript-Interfaces definiert werden.
* **Fehlerbehandlung:** Jeder `invoke`-Aufruf im Frontend muss robust in einem `try/catch`-Block gekapselt werden, um Fehler aus dem Rust-Backend abzufangen und dem Nutzer verständliches Feedback zu geben.

**5. Aktuelle Dateistruktur**
Dies ist der aktuelle Zustand des Projekts mit den implementierten Komponenten.

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
│   │   ├── CreateProfile.tsx
│   │   ├── CreateVoucher.tsx
│   │   ├── Dashboard.tsx
│   │   ├── Login.tsx
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
│   │   └── models.rs
│   ├── tauri.conf.json
│   └── wallet_data
│       ├── bundles.meta.enc
│       ├── fingerprints.enc
│       ├── profile.enc
│       └── vouchers.enc
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
  * `App.tsx`: Hauptkomponente mit State-Management für die Anwendungsphasen (Profil erstellen, Login, Dashboard, Passwort zurücksetzen, Gutschein erstellen, Gutschein Details anzeigen)
  * `components/Dashboard.tsx`: Zeigt Benutzer-ID, Guthaben und Gutscheinübersicht an
  * `components/CreateProfile.tsx`: Formular zur Profilerstellung mit Mnemonic-Generierung
  * `components/Login.tsx`: Login-Formular mit Passwort-Eingabe
  * `components/VoucherDetailsView.tsx`: Detaillierte Ansicht für einen einzelnen Gutschein mit allen Eigenschaften, Transaktionshistorie und Signaturinformationen
  * `components/CreateVoucher.tsx`: Formular zur Erstellung neuer Gutscheine mit umfassenden Details zum Ersteller und zur Besicherung
  * `components/WalletRecovery.tsx`: Formular zur Wiederherstellung des Wallets mit Mnemonic
  * `types.ts`: TypeScript-Interfaces für `VoucherSummary`, `VoucherDetails`, `NewVoucherData` und andere komplexe Datentypen
  * `utils/log.ts`: Frontend-Logging-Utility für konsistentes Logging ins Backend

* **Backend (src-tauri/)**
  * `src/lib.rs`: Hauptdatei mit allen Tauri-Befehlen, die die `voucher_lib::AppService`-Fassade nutzen
  * `src/main.rs`: Einstiegspunkt, der die `run()`-Funktion aus `lib.rs` aufruft
  * `src/commands/actions.rs`: Implementierung von Voucher-Aktionen wie `create_new_voucher`
  * `src/commands/auth.rs`: Authentifizierungsbezogene Befehle wie `create_profile`, `login`, `logout`
  * `src/commands/queries.rs`: Abfragebezogene Befehle wie `get_voucher_summaries`, `get_voucher_details`
  * `src/commands/utils.rs`: Hilfsfunktionen wie `generate_mnemonic`, `get_voucher_standards`, `frontend_log`, `log_to_backend`
  * `src/models.rs`: Datenstrukturdefinitionen für den Austausch zwischen Frontend und Backend

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