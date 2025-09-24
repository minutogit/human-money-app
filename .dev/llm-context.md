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

**Frontend (React - src)**

* **Framework:** React mit TypeScript
* **Kommunikation:** Die Interaktion mit dem Backend erfolgt ausschließlich über die `invoke`-Funktion aus der `@tauri-apps/api/core`-Bibliothek.
* **State Management:** Für den Prototyp wird auf Einfachheit gesetzt und Reacts nativer `useState`-Hook verwendet.

**Styling-Konfiguration (Tailwind CSS)**

* **Technologie:** Das Styling wird vollständig mit Tailwind CSS umgesetzt.
* **Integration:** Die Einbindung in das Vite-Build-System erfolgt **ausschließlich** über das offizielle Plugin `@tailwindcss/vite`. Eine `postcss.config.js` darf **nicht** verwendet werden, um Konflikte zu vermeiden.
* **Setup in CSS:** Die zentrale CSS-Datei (`src/App.css`) muss Tailwind über die Direktive `@import "tailwindcss";` importieren.
* **Laden der Konfiguration:** Die Projektkonfiguration `tailwind.config.js` (im Projekt-Stammverzeichnis) muss explizit über `@config "../tailwind.config.js";` in `src/App.css` geladen werden. Der relative Pfad (`../`) ist hierbei entscheidend.

**3. Kernfunktionalität (MVP)**
Die folgenden Funktionen der `voucher_lib` sollen implementiert werden:

* **Profil & Login:**
  * `create_profile`
  * `login`
* **Dashboard-Anzeige:**
  * `get_user_id`
  * `get_total_balance_by_currency`
  * `get_voucher_summaries`
* **Transaktionen:**
  * `create_transfer_bundle`
  * `receive_bundle`

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
│   │   └── WalletRecovery.tsx
│   ├── main.tsx
│   ├── types.ts
│   └── vite-env.d.ts
├── src-tauri
│   ├── build.rs
│   ├── Cargo.lock
│   ├── Cargo.toml
│   ├── src
│   │   ├── lib.rs
│   │   └── main.rs
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
  * `App.tsx`: Hauptkomponente mit State-Management für die Anwendungsphasen (Profil erstellen, Login, Dashboard, Passwort zurücksetzen)
  * `components/Dashboard.tsx`: Zeigt Benutzer-ID, Guthaben und Gutscheinübersicht an
  * `components/CreateProfile.tsx`: Formular zur Profilerstellung mit Mnemonic-Generierung
  * `components/Login.tsx`: Login-Formular mit Passwort-Eingabe
  * `components/PasswordReset.tsx`: Formular zur Wiederherstellung des Wallets mit Mnemonic
  * `types.ts`: TypeScript-Interface für `VoucherSummary`

* **Backend (src-tauri/)**
  * `src/lib.rs`: Hauptdatei mit allen Tauri-Befehlen, die die `voucher_lib::AppService`-Fassade nutzen
  * `src/main.rs`: Einstiegspunkt, der die `run()`-Funktion aus `lib.rs` aufruft

**7. Logging**

* **Frontend Logging:** Zum Debuggen von Frontend-Komponenten kann ein spezieller Logging-Befehl verwendet werden, der Logs direkt in das Rust-Terminal sendet:
  * In der Komponente: `invoke("frontend_log", { message: "Deine Log-Nachricht" }).catch(console.error);`
  * Dieser Befehl ruft eine spezielle Funktion im Rust-Backend auf, die die Nachricht mit dem Rust-Logger ausgibt und somit im Terminal sichtbar macht.
  * Beispiel:
    ```typescript
    useEffect(() => {
        invoke("frontend_log", { message: "Komponente wurde geladen" }).catch(console.error);
        // Deine Logik hier
    }, []);
    ```