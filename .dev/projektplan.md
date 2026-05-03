# Human Money App MVP - Projektplan

Dieser Plan strukturiert die Entwicklung der Human Money Desktop-App. Er priorisiert den Nutzwert und baut inkrementell auf der `human_money_core` auf.

## ✅ Phase 1: Die Kern-Transaktion (Abgeschlossen)
**Ziel:** Grundlegende Funktionsfähigkeit: Erstellen, Senden und Empfangen von Gutscheinen.

- [x] **UI-Grundgerüst:** Responsive Layout mit React 19, Tailwind CSS v4 und Vite.
- [x] **Gutschein-Erstellung:** `CreateVoucher.tsx` integriert mit `create_new_voucher`.
- [x] **Transaktions-Kern:** `SendView.tsx` und `ReceiveView.tsx` für Transfer-Bundles.
- [x] **Multi-Profil & Auth:** Login-System mit Mnemonic-Wiederherstellung und Passwort-Schutz.

## ✅ Phase 2: Verwaltung & Sicherheit (Abgeschlossen)
**Ziel:** Die App nutzbar und sicher machen.

- [x] **Dashboard & Details:** Übersichtliche Liste (`WalletView`) und detaillierte Gutscheinansicht (`VoucherDetailsView`).
- [x] **Kontaktverwaltung:** `AddressBook.tsx` zur Speicherung von Empfängern (DID -> Name).
- [x] **Sicherheitshärtung:**
    - [x] **Wallet Seal & Device Binding:** Schutz vor Klonen und Rollbacks.
    - [x] **Integrity Reports:** Automatischer Check auf Double-Spending/Forks beim Login.
    - [x] **Session Management:** "Passwort merken" mit Sliding-Expiration.
- [x] **Aktivitäten-Historie:** `Activities.tsx` zur Anzeige vergangener Transaktionen (Event-Sourcing).

## 🏃 Phase 3: Vertrauensnetz & Bürgschaften
**Ziel:** Implementierung des Guarantor-Flows und sozialer Validierung.

- [ ] **Bürge-Funktionalität (Guarantor-Flow):**
    - [ ] Signaturanfragen empfangen und anzeigen.
    - [ ] Detached Signatures erstellen (`create_detached_signature_response_bundle`).
    - [ ] Signaturen an bestehende Gutscheine anhängen (`process_and_attach_signature`).
- [ ] **Erweiterte Profil-Metadaten:** Integration von `PublicProfile` (Name, Organisation, Community) zur besseren Identifizierung von Bürgen.
- [ ] **Standard-Explorer:** Ansicht der verfügbaren Gutschein-Standards (Minuto, FreeTaler) und deren Regeln.

## 📅 Phase 4: Politur & Skalierbarkeit
**Ziel:** Vorbereitung auf den produktiven Einsatz.

- [ ] **Internationalisierung (i18n):** Integration von `i18next` für Multi-Language Support.
- [ ] **Dynamisches UI für Standards:** Dynamische Formular-Generierung basierend auf TOML-Standarddefinitionen.
- [ ] **Performance-Optimierung:** 
    - [ ] Lazy Loading für große Historien.
    - [ ] Optimierung der IPC-Payloads (DTO-Pattern Verfeinerung).
- [ ] **Backup-Strategien:** Geführte Export-Workflows für Wallet-Daten und Mnemonic.