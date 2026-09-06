---
name: core-api-reference
description: Reference documentation for the human_money_core AppService API as used from Tauri commands. Includes all public functions, data structures, and error handling. Note - always verify against actual code for latest API.
---

# AppService: Public API Facade (v0.3+)

Dies ist die Kontextdatei für die `human_money_core`-Bibliothek (ab v0.3.0), die für die Entwicklung von Client-Anwendungen (Tauri, Mobile) aufbereitet wurde. Sie bietet eine präzise Referenz für die `AppService`-Fassade.

### 1. Architektur & Konzepte

*   **AppService-Fassade:** Einzige Schnittstelle zur Kernlogik. Verwaltet Zustand (`Locked`/`Unlocked`), Persistenz und Kryptografie.
*   **Zustandsverwaltung:** Nach `login` oder `create_profile` ist der Service `Unlocked`. `logout` sperrt ihn wieder.
*   **Read-Only-Zugriff:** Lese-Abfragen auf den Wallet-Zustand werden über `service.with_wallet(|wallet| ...)` bzw. `service.with_wallet_and_identity(|wallet, identity| ...)` ausgeführt.
*   **Pessimistic Locking:** Verhindert gleichzeitigen Zugriff mehrerer Prozesse auf dasselbe Wallet (`.wallet.lock`).
*   **Wallet Seal & Integrity:** Schützt vor Rollback-Angriffen und Manipulationen an den verschlüsselten Dateien. Bindet das Wallet an eine `local_instance_id` (Device-Binding).
*   **Argon2id:** Wird für das Key-Stretching des anonymen Ordnernamens und der Verschlüsselungsschlüssel verwendet.

### 2. Lebenszyklus & Authentifizierung

#### `pub fn new(base_storage_path: &Path) -> Result<Self, Error>`
Initialisiert den Service im `Locked`-Zustand.

#### `pub fn list_profiles(&self) -> Result<Vec<ProfileInfo>, Error>`
Listet alle Profile im Basisverzeichnis auf (liest `profiles.json`).
`ProfileInfo` enthält `profile_name` und `folder_name`.

#### `pub fn create_profile(...) -> Result<(), Error>`
Erstellt ein Profil.
*   **Parameter:** `profile_name`, `mnemonic`, `passphrase`, `user_prefix`, `password`, `language: MnemonicLanguage`, `local_instance_id`.

#### `pub fn login(...) -> Result<(), Error>`
Entsperrt ein Wallet.
*   **Parameter:** `folder_name`, `password`, `cleanup_on_login`, `local_instance_id`.

#### `pub fn logout(&mut self)`
Sperrt das Wallet und löscht alle Keys aus dem RAM.

#### `pub fn is_session_active(&self) -> bool`
Prüft, ob eine "Passwort merken"-Sitzung (Session) aktuell aktiv ist.

#### `pub fn unlock_session(&mut self, password: &str, duration_seconds: u64) -> Result<(), Error>`
Aktiviert den Session-Cache für schreibende Operationen (Modus B).

#### `pub fn lock_session(&mut self)`
Löscht den Session-Cache sofort.

#### `pub fn refresh_session_activity(&mut self) -> Result<(), Error>`
Verlängert die aktive Session bei Benutzerinteraktion.

### 3. Kern-Operationen (Commands / Mutationen)

#### `pub fn create_new_voucher(standard_toml_content: &str, data: NewVoucherData, password: Option<&str>) -> Result<Voucher, Error>`
Erstellt einen neuen Gutschein. Benötigt Auth (Passwort oder Session).
*Hinweis:* `lang_preference` wurde in v0.3 entfernt; `NewVoucherData` wird direkt aus dem Crate-Root importiert.

#### `pub fn create_transfer_bundle(request: MultiTransferRequest, standards: &HashMap<String, String>, archive: Option<&FileVoucherArchive>, password: Option<&str>) -> Result<CreateBundleResult, Error>`
Kernfunktion zum Senden von Werten. Erstellt verschlüsselten `SecureContainer`. Verwendet konkreten `FileVoucherArchive`.

#### `pub fn receive_bundle(bundle_data: &[u8], standards: &HashMap<String, String>, archive: Option<&FileVoucherArchive>, password: Option<&str>, force_accept_tolerance_bundle: bool) -> Result<ProcessBundleResult, Error>`
Kernfunktion zum Empfangen. Validiert Transaktion, Double-Spending und Recovery-Toleranzzonen.

#### `pub fn remove_voucher_signature(local_id: &str, sig_id: &str, password: Option<&str>) -> Result<(), Error>`
Entfernt Bürgen-Signaturen (nur vor Umlauf im Incomplete-Status möglich).

#### `pub fn run_storage_cleanup(&mut self) -> Result<CleanupReport, Error>`
Bereinigung abgelaufener oder alter Transaktionsfingerprints und archivierter Einträge.

### 4. Datenabfragen (Queries) - Read-Only via `with_wallet`

Lese-Operationen werden über die Scoped-Access-Methoden der Fassade ausgeführt:

```rust
// Zugriff nur auf Wallet:
service.with_wallet(|wallet| {
    wallet.get_user_id()
})?;

// Zugriff auf Wallet und UserIdentity:
service.with_wallet_and_identity(|wallet, identity| {
    wallet.get_total_balance_by_currency(Some(identity))
})?;
```

#### `wallet.get_user_id(&self) -> &str`
Gibt die `did:key`-ID des Nutzers zurück.

#### `wallet.list_vouchers(identity: Option<&UserIdentity>, voucher_standard_uuid_filter: Option<&[String]>, status_filter: Option<&[VoucherStatus]>, test_filter: Option<bool>) -> Vec<VoucherSummary>`
Gibt eine Liste von Gutscheinen zurück. `VoucherSummary` enthält:
*   `local_instance_id`, `status`, `valid_until`, `creator_id`, `current_amount`, `unit`, `raw_standard_name`, `voucher_standard_uuid`, `transaction_count`, `signatures_count`, `has_collateral`, `creator_first_name`, `creator_last_name`, `creator_coordinates`, `is_test_voucher`, `display_currency`, `display_standard_name`.
*   *Hinweis v0.3:* `description` und `allow_partial_transfers` sind nicht mehr Teil von `VoucherSummary`, sondern werden auf App-Ebene aus dem Standard bezogen bzw. mit Defaults versehen.

#### `wallet.get_total_balance_by_currency(identity: Option<&UserIdentity>) -> Vec<AggregatedBalance>`
Aggregiertes Guthaben pro Standard, Währung und Test-Status.

#### `wallet.get_voucher_details(&self, local_instance_id: &str) -> Result<VoucherDetails, VoucherCoreError>`
Vollständige Details inkl. Transaktionshistorie und BFF-Anzeigenamen.

#### `wallet.profile.to_public_profile(&self) -> PublicProfile`
Gibt das Metadaten-Profil des Nutzers zurück.

#### `wallet.get_active_asset_classes(&self) -> Vec<AssetClassSummary>`
Ermittelt alle im Wallet aktiven Asset-Klassen (Standard-UUID, Test-Status, BFF-Namen).

#### `wallet.check_reputation(&self, offender_id: &str) -> TrustStatus`
Prüft den Ruf einer User-ID basierend auf lokalen Beweisen (`ProofStore`).

#### `wallet.get_voucher_source_sender(&self, local_instance_id: &str, identity: &UserIdentity) -> Result<Option<String>, VoucherCoreError>`
Ermittelt die Identität des Absenders durch Entschlüsselung des Privacy Guards mit dem eigenen Schlüssel.

### 5. Standards & Kryptografie

#### `VoucherStandardDefinition::from_toml(toml_str: &str) -> Result<(Self, String), VoucherCoreError>`
Parst und verifiziert einen Gutschein-Standard (TOML). Berechnet deterministischen `logic_hash` und prüft die kryptografische Signatur.

#### `standard.immutable.issuance.allowed_signature_roles`
Enthält die Liste der erlaubten Signatur-Rollen (z.B. `["guarantor", "auditor"]`).

#### `WalletSeal::compute_seal_hash(seal: &WalletSeal) -> Result<String, VoucherCoreError>` / `seal.compute_hash()`
Berechnet den SHA-256-Hash des Siegels aus kanonischem JSON.

#### `human_money_core::services::voucher_validation::SignatureImpact`
Struktur zur Evaluierung der Eignung einer Rolle für eine Zusatzsignatur.

### 6. Fehlertaxonomie (`AppFacadeError` / `Error`)

Zentraler Fehler-Enum ab v0.3:
*   `WalletLocked`: Unit-Variante (Wallet ist gesperrt).
*   `RequiresSealRecovery`, `StateRollbackDetected`, `WalletLockedDueToFork`: Unit-Varianten für Sicherheitszustände.
*   `DoubleSpendAttemptBlocked { local_instance_id }`: Struct-Variante für blockierte Double-Spends.
*   `VoucherNotActive(VoucherStatus)`: Prüfung auf Quarantäne via `matches!(status, VoucherStatus::Quarantined { .. })`.
*   `Validation(ValidationError)`: Strukturelle und Business-Rule-Validierungsfehler.
*   `Json(String)`: JSON-Serialisierungsfehler.
*   `Storage(StorageError)`: Typisierte Speicherfehler (z.B. `StorageError::NotFound`).

### 7. V3 Protocol: Shared-Signature Trap (SST)

*   `TrapData` enthält `ds_tag: String`, `trap_r: String`, `trap_s: String`.
*   Die alten Felder `u`, `blinded_id` und `proof` wurden durch spend-spezifische Schnorr-Commitment- und Response-Shards abgelöst.