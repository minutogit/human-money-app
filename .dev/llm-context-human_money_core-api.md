.dev/llm-context-human_money_core-api.md
### Kontext für Tauri-App-Entwicklung mit human_money_core

Dies ist die Kontextdatei für die `human_money_core`-Bibliothek, die für die Entwicklung von Client-Anwendungen (Tauri, Mobile) aufbereitet wurde. Sie bietet eine präzise Referenz für die `AppService`-Fassade.

### 1. Architektur & Konzepte

*   **AppService-Fassade:** Einzige Schnittstelle zur Kernlogik. Verwaltet Zustand (`Locked`/`Unlocked`), Persistenz und Kryptografie.
*   **Zustandsverwaltung:** Nach `login` oder `create_profile` ist der Service `Unlocked`. `logout` sperrt ihn wieder.
*   **Pessimistic Locking:** Verhindert gleichzeitigen Zugriff mehrerer Prozesse auf dasselbe Wallet (`.wallet.lock`).
*   **Wallet Seal & Integrity:** Schützt vor Rollback-Angriffen und Manipulationen an den verschlüsselten Dateien. Bindet das Wallet an eine `local_instance_id` (Device-Binding).
*   **Argon2id:** Wird für das Key-Stretching des anonymen Ordnernamens und der Verschlüsselungsschlüssel verwendet.

### 2. Lebenszyklus & Authentifizierung

#### `pub fn new(base_storage_path: &Path) -> Result<Self, String>`
Initialisiert den Service im `Locked`-Zustand.

#### `pub fn list_profiles(&self) -> Result<Vec<ProfileInfo>, String>`
Listet alle Profile im Basisverzeichnis auf (liest `profiles.json`).
`ProfileInfo` enthält `profile_name` und `folder_name`.

#### `pub fn create_profile(...) -> Result<(), String>`
Erstellt ein Profil.
*   **Parameter:** `profile_name`, `mnemonic`, `passphrase`, `user_prefix`, `password`, `language: MnemonicLanguage`, `local_instance_id`.

#### `pub fn login(...) -> Result<(), String>`
Entsperrt ein Wallet.
*   **Parameter:** `folder_name`, `password`, `cleanup_on_login`, `local_instance_id`.

#### `pub fn logout(&mut self)`
Sperrt das Wallet und löscht alle Keys aus dem RAM.

#### `pub fn is_session_active(&self) -> bool`
Prüft, ob eine "Passwort merken"-Sitzung (Session) aktuell aktiv ist.

#### `pub fn unlock_session(&mut self, password: &str, duration_seconds: u64) -> Result<(), String>`
Aktiviert den Session-Cache für schreibende Operationen (Modus B).

#### `pub fn lock_session(&mut self)`
Löscht den Session-Cache sofort.

#### `pub fn refresh_session_activity(&mut self) -> Result<(), String>`
Verlängert die aktive Session bei Benutzerinteraktion.

### 3. Kern-Operationen (Commands)

#### `pub fn create_new_voucher(standard_toml_content: &str, lang_preference: &str, data: NewVoucherData, password: Option<&str>) -> Result<Voucher, String>`
Erstellt einen neuen Gutschein. Benötigt Auth (Passwort oder Session).

#### `pub fn create_transfer_bundle(request: MultiTransferRequest, standards: &HashMap<String, String>, archive: Option<&dyn VoucherArchive>, password: Option<&str>) -> Result<CreateBundleResult, String>`
Kernfunktion zum Senden von Werten. Erstellt verschlüsselten `SecureContainer`.

#### `pub fn receive_bundle(bundle_data: &[u8], standards: &HashMap<String, String>, archive: Option<&dyn VoucherArchive>, password: Option<&str>) -> Result<ProcessBundleResult, String>`
Kernfunktion zum Empfangen. Validiert Transaktion und Double-Spending.

### 4. Datenabfragen (Queries) - Read-Only

#### `pub fn get_user_id(&self) -> Result<String, String>`
Gibt die `did:key`-ID des Nutzers zurück.

#### `pub fn get_voucher_summaries(standard_filter: Option<&[String]>, status_filter: Option<&[VoucherStatus]>, test_filter: Option<bool>) -> Result<Vec<VoucherSummary>, String>`
Gibt eine Liste von Gutscheinen zurück. `VoucherSummary` enthält:
*   `local_instance_id`, `status`, `valid_until`, `creator_id`, `description`, `current_amount`, `unit`, `voucher_standard_name`, `voucher_standard_uuid`, `transaction_count`, `has_collateral`, `is_test_voucher`, etc.

#### `pub fn get_total_balance_by_currency(&self) -> Result<Vec<AggregatedBalance>, String>`
Aggregiertes Guthaben pro Währung.

#### `pub fn get_voucher_details(&self, local_id: &str) -> Result<VoucherDetails, String>`
Vollständige Details inkl. Transaktionshistorie.

#### `pub fn get_public_profile(&self) -> Result<PublicProfile, String>`
Gibt das Metadaten-Profil des Nutzers zurück.

#### `pub fn get_event_history(offset: usize, limit: usize, password: Option<&str>) -> Result<Vec<WalletEvent>, String>`
Abfrage der Event-Sourcing-Historie.

#### `pub fn get_active_asset_classes(&self) -> Result<Vec<AssetClassSummary>, String>`
Ermittelt alle im Wallet aktiven Asset-Klassen (Gutschein-Standard + Test-Status).

#### `pub fn parse_voucher_standard(&self, standard_toml_content: &str) -> Result<VoucherStandardDefinition, String>`
Parst und verifiziert einen Gutschein-Standard (TOML). Verifiziert auch die kryptografische Signatur.

#### `pub fn get_allowed_signature_roles_from_standard(&self, standard_toml_content: &str) -> Result<Vec<String>, String>`
Extrahiert die Liste der erlaubten Signatur-Rollen (z.B. "guarantor") aus einem Standard.

### 5. Management & Sicherheit

#### `pub fn update_public_profile(profile: PublicProfile, password: Option<&str>) -> Result<(), String>`
Aktualisiert den Namen, Adresse, etc.

#### `pub fn remove_voucher_signature(local_id: &str, sig_id: &str, password: Option<&str>) -> Result<(), String>`
Entfernt Bürgen-Signaturen (nur vor Umlauf möglich).

#### `pub fn check_reputation(&self, offender_id: &str) -> Result<TrustStatus, String>`
Prüft den Ruf einer User-ID basierend auf lokalen Beweisen.

#### `pub fn get_voucher_source_sender(&self, local_instance_id: &str) -> Result<Option<String>, String>`
Ermittelt die Identität des Absenders eines Gutscheins (ggf. durch Entschlüsselung).

#### `pub fn run_storage_cleanup(&mut self) -> Result<CleanupReport, VoucherCoreError>`
Manuelle Bereinigung alter Transaktionsdaten.

#### `pub fn check_for_forks(&mut self) -> Result<IntegrityReport, String>`
Prüft auf Double-Spending-Versuche durch Abgleich mit dem Server-Seal.

### 6. Wichtige Rückgabestrukturen

*   **`ProcessBundleResult`**: Enthält `DoubleSpendCheckResult`, `TransferSummary` und IDs der neuen Gutscheine.
*   **`IntegrityReport`**: Meldet `Valid`, `ForkDetected`, `RollbackDetected` oder `MissingSeal`.

### 7. Sicherheitsregeln für Apps

1.  **`local_instance_id`**: Muss stabil für ein Gerät sein, darf aber **NIE** im Wallet-Ordner gespeichert werden (Gefahr der Klonung). Empfohlen: OS Keyring.
2.  **Passwort-Sicherheit**: Sensible Operationen erfordern Passwort oder eine aktive Session.
3.  **Fehlerbehandlung**: UI sollte Sperrfehler (`Wallet locked by PID 1234`) verständlich anzeigen.