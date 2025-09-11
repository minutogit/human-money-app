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

**5. Ausgangs-Dateistruktur**
Dies ist der initiale Zustand des Projekts, bevor Code-Änderungen vorgenommen werden.

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
│   │   └── ui
│   │       ├── Button.tsx
│   │       ├── Input.tsx
│   │       └── Textarea.tsx
│   ├── main.tsx
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
├── tsconfig.json
├── tsconfig.node.json
└── vite.config.ts
```