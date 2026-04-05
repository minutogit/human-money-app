---
name: core-api-reference
description: Reference documentation for the human_money_core AppService API as used from Tauri commands. Includes all public functions, data structures, and error handling. Note - always verify against actual code for latest API.
---

# human_money_core API Reference for App Development
Dies ist die Kontextdatei für die human_money_core-Bibliothek, die für die Entwicklung von Client-Anwendungen, wie z. B. Tauri-Apps, aufbereitet wurde. Sie bietet eine präzise und vollständige Referenz für die öffentliche API, die zur Interaktion mit der Kernlogik notwendig ist.

### 1. Projekt & Zweck
**Projektname:** human_money_core

**Zweck:** Bereitstellung der Kernlogik für ein dezentrales, elektronisches Gutschein-System.

**Ziel:** Die Bibliothek stellt eine `AppService`-Fassade bereit, die alle komplexen Operationen kapselt und eine einfache, sichere Schnittstelle für die Anwendungsentwicklung bietet.

### 2. Architektur & Kernkonzepte
**Dezentraler Ansatz:** Das System basiert auf digitalen Gutscheinen (im Grunde signierte Textdateien), die ihre eigene Transaktionshistorie enthalten. Es gibt kein zentrales, globales Ledger.

**Offline-Fähigkeit:** Transaktionen können offline durchgeführt werden, indem die aktualisierte Gutschein-Datei direkt an den neuen Halter übergeben wird.

**Betrugserkennung:** Das System ist darauf ausgelegt, "Double Spending" (das mehrfache Ausgeben desselben Gutscheins) kryptografisch nachweisbar zu machen. Die Client-Anwendung muss sich nicht um die Details der Erkennung kümmern, sondern nur auf die Ergebnisse reagieren, die der `AppService` liefert.

**Entkoppelte Speicherung:** Die `human_money_core` nutzt ein `Storage`-Trait, um die Logik von der Speicherung zu trennen. Für Client-Anwendungen wird eine Standardimplementierung (`FileStorage`) bereitgestellt, die alle Daten sicher verschlüsselt im Dateisystem ablegt.

**Prozess-Sperrung (Pessimistic Locking):** Um Dateninkonsistenzen bei gleichzeitig laufenden Instanzen zu verhindern, implementiert die `FileStorage` ein dateibasiertes Locking mit PID-Check (`.wallet.lock`). Schreibende Operationen erwerben automatisch eine exklusive Sperre. Wenn ein anderer Prozess das Wallet bereits sperrt, wird die Operation mit einem Fehler abgelehnt.

### 3. Öffentliche API: Das AppService-Modul
Der `AppService` ist die einzige Schnittstelle, die für die Entwicklung der Client-Anwendung relevant ist. Er verwaltet den Zustand des Wallets (gesperrt/entsperrt) und stellt alle notwendigen Funktionen bereit.

**`pub struct AppService`**
**Zustandsverwaltung:** Hält den Zustand des Wallets (`Locked` oder `Unlocked`). Nach einem `login` ist der Service im `Unlocked`-Zustand und alle Operationen sind möglich. `logout` versetzt ihn zurück in den `Locked`-Zustand.

**Automatische Speicherung:** Alle Operationen, die den Wallet-Zustand verändern (z.B. ein Transfer), werden automatisch persistent gespeichert.

#### Hauptfunktionen (Befehle)
**`pub fn new(base_storage_path: &Path) -> Result<Self, String>`**

Initialisiert einen neuen `AppService` im `Locked`-Zustand. Erstellt eine `FileStorage`-Instanz für den angegebenen Pfad. Das Verzeichnis wird bei Bedarf erstellt.

**`pub fn list_profiles(&self) -> Result<Vec<ProfileInfo>, String>`**

Listet alle verfügbaren, im Basisverzeichnis konfigurierten Profile auf. Liest die zentrale `profiles.json`-Datei und gibt eine Liste von `ProfileInfo`-Objekten zurück, die für die Anzeige in einem Login-Screen verwendet werden kann.

Die `ProfileInfo`-Struktur enthält:
* `profile_name: String`: Der vom Benutzer gewählte, menschenlesbare Name.
* `folder_name: String`: Der anonyme Ordnername, der für `login` benötigt wird.

**`pub fn create_profile(&mut self, profile_name: &str, mnemonic: &str, passphrase: Option<&str>, user_prefix: Option<&str>, password: &str) -> Result<(), String>`**

Erstellt ein komplett neues Benutzerprofil. Diese Funktion leitet einen anonymen Ordnernamen aus den Secrets ab, speichert das Wallet in diesem Ordner und fügt einen Eintrag mit dem `profile_name` zur zentralen `profiles.json` hinzu. Bei Erfolg wird der Service in den `Unlocked`-Zustand versetzt.

**`pub fn login(&mut self, folder_name: &str, password: &str, cleanup_on_login: bool) -> Result<(), String>`**

Entsperrt ein existierendes Wallet. Der `folder_name` wird typischerweise über die `list_profiles`-Funktion bezogen. Bei Bedarf kann eine Speicherbereinigung direkt beim Login durchgeführt werden.

**`pub fn recover_wallet_and_set_new_password(&mut self, folder_name: &str, mnemonic: &str, passphrase: Option<&str>, new_password: &str) -> Result<(), String>`**

Stellt ein Wallet mithilfe der Mnemonic-Phrase wieder her und setzt ein neues Passwort. Der `folder_name` gibt an, welches Profil wiederhergestellt werden soll. Bei Erfolg wird der Service in den `Unlocked`-Zustand versetzt.

**`pub fn logout(&mut self)`**

Sperrt das Wallet und entfernt sensible Daten wie private Schlüssel aus dem Speicher. Diese Operation kann nicht fehlschlagen.

**`pub fn create_new_voucher(&mut self, standard_toml_content: &str, lang_preference: &str, data: NewVoucherData, password: &str) -> Result<Voucher, String>`**

Erstellt einen brandneuen Gutschein, fügt ihn zum Wallet hinzu und speichert den Zustand. Verifiziert zuerst die Standard-Definition, bevor der Gutschein erstellt wird.
*Hinweis: Diese Operation erwirbt eine exklusive Dateisperre. Schlägt fehl, wenn ein anderer Prozess das Wallet verwendet.*
**`pub fn create_transfer_bundle(&mut self, request: MultiTransferRequest, standard_definitions_toml: &HashMap<String, String>, archive: Option<&dyn VoucherArchive>, password: &str) -> Result<CreateBundleResult, String>`**

Erstellt ein verschlüsseltes `SecureContainer`-Bundle für einen Transfer an einen Empfänger. Dies ist der Kernprozess zum Senden von Werten. Die Funktion akzeptiert eine `MultiTransferRequest`, die es ermöglicht, Guthaben von einem oder mehreren Quell-Gutscheinen in einer einzigen Transaktion zu bündeln.

Die `MultiTransferRequest`-Struktur enthält:
* `recipient_id: String`: Die User-ID des Empfängers.
* `sources: Vec<SourceTransfer>`: Eine Liste von Quell-Gutscheinen, die für die Zahlung verwendet werden sollen. Jede `SourceTransfer` definiert `local_instance_id` und `amount_to_send`.
* `notes: Option<String>`: Optionale Notizen für den Empfänger.
* `sender_profile_name: Option<String>`: Optionaler Anzeigename des Senders.

Das Ergebnis (`CreateBundleResult`) ist eine Struktur, die die serialisierten Daten (`bundle_bytes: Vec<u8>`) für den Versand sowie detaillierte Informationen über die Transaktion (z.B. `involved_sources_details`) enthält.
Die Wallet wird automatisch gespeichert.
*Hinweis: Diese Operation erwirbt eine exklusive Dateisperre.*

**`pub fn receive_bundle(&mut self, bundle_data: &[u8], standard_definitions_toml: &HashMap<String, String>, archive: Option<&dyn VoucherArchive>, password: &str) -> Result<ProcessBundleResult, String>`**

Verarbeitet ein empfangenes Bundle. Die Funktion validiert die Transaktion, fügt die Gutscheine zum eigenen Wallet hinzu und gibt ein `ProcessBundleResult` zurück, das über den Erfolg und die Details der Transaktion informiert. Die Wallet wird automatisch gespeichert. Der Caller muss die benötigten Standard-Definitionen als TOML-Strings bereitstellen.

---

### 3.1 Wichtige Rückgabe-Strukturen

**`pub struct ProcessBundleResult`**
Dies ist die Rückgabestruktur von `receive_bundle` und enthält eine Zusammenfassung der Transaktion.

* `header: TransactionBundleHeader`: Metadaten des Bundles (Absender, Empfänger, Notizen, etc.).
* `check_result: DoubleSpendCheckResult`: Ergebnis der Double-Spend-Prüfung.
* `transfer_summary: TransferSummary`: Detaillierte Aufschlüsselung der empfangenen Werte.
* `involved_vouchers: Vec<String>`: Liste der lokalen IDs der Gutscheine, die im Wallet neu erstellt/aktualisiert wurden.

**`pub struct TransferSummary`**
Fasst die Ergebnisse eines Transfers pro Währungseinheit zusammen.

* `summable_amounts: HashMap<String, String>`:
  * Aufsummierte Beträge für teilbare/summierbare Gutscheine (z.B. "10.50 Minuto").
  * Key: Währungseinheit (z.B. "Minuto"), Value: Summe als String.
* `countable_items: HashMap<String, u32>`:
  * Gezählte Einheiten für nicht-teilbare/nicht-summierbare Gutscheine (z.B. "3 Brote").
  * Key: Währungseinheit (z.B. "Brot"), Value: Anzahl.

---

**`pub fn create_signing_request_bundle(&self, local_instance_id: &str, recipient_id: &str) -> Result<Vec<u8>, String>`**

Erstellt ein Bundle, um eine Signaturanfrage für einen Gutschein an einen Bürgen zu senden. Diese Operation verändert den Wallet-Zustand nicht.

**`pub fn create_detached_signature_response_bundle(&self, voucher_to_sign: &Voucher, role: &str, include_details: bool, original_sender_id: &str) -> Result<Vec<u8>, String>`**

Erstellt eine losgelöste Signatur als Antwort auf eine Signaturanfrage. Diese Operation wird vom Bürgen aufgerufen und verändert dessen Wallet-Zustand nicht.

**`pub fn process_and_attach_signature(&mut self, container_bytes: &[u8], standard_toml_content: &str, password: &str) -> Result<(), String>`**

Verarbeitet eine empfangene losgelöste Signatur, fügt sie dem lokalen Gutschein hinzu und speichert den Wallet-Zustand.

**`pub fn import_resolution_endorsement(&mut self, endorsement: ResolutionEndorsement, password: &str) -> Result<(), String>`**

Importiert eine Beilegungserklärung für einen Double-Spend-Konflikt, fügt sie dem entsprechenden Beweis im Wallet hinzu und speichert den Zustand.

**`pub fn save_encrypted_data(&mut self, name: &str, data: &[u8], password: &str) -> Result<(), String>`**

Speichert einen beliebigen Byte-Slice verschlüsselt auf der Festplatte. Diese Methode nutzt den gleichen sicheren Verschlüsselungsmechanismus wie das Wallet selbst. Ideal, um anwendungsspezifische Daten (z.B. Konfigurationen, Kontakte) sicher abzulegen.

**`pub fn load_encrypted_data(name: &str, password: &str) -> Result<Vec<u8>, String>`**

Lädt und entschlüsselt einen zuvor gespeicherten, beliebigen Datenblock. Aus Sicherheitsgründen wird das Passwort für jede Leseoperation benötigt, um den Entschlüsselungsschlüssel abzuleiten.

#### Hilfsfunktionen (Statische Methoden)
Diese Funktionen sind Teil des `AppService`, benötigen aber keinen initialisierten Zustand (weder `Locked` noch `Unlocked`) und können jederzeit aufgerufen werden.

**`pub fn generate_mnemonic(word_count: u32) -> Result<String, String>`**

Erzeugt eine neue, kryptografisch sichere BIP-39 Mnemonic-Phrase (Seed-Wörter). Ideal, um einem neuen Benutzer bei der Profilerstellung eine Phrase vorzuschlagen. `word_count` ist typischerweise 12 oder 24.

**`pub fn validate_mnemonic(mnemonic: &str) -> Result<(), String>`**

Überprüft eine gegebene Mnemonic-Phrase auf ihre Gültigkeit (korrekte Wörter, gültige Prüfsumme). Dies ist nützlich, um dem Benutzer bei der Eingabe zur Wiederherstellung eines Wallets sofortiges Feedback zu geben, bevor der eigentliche Login-Versuch unternommen wird.


#### Konflikt-Management
Diese Funktionen dienen der Verwaltung von Double-Spend-Konflikten.

**`pub fn list_conflicts(&self) -> Result<Vec<ProofOfDoubleSpendSummary>, String>`**

Gibt eine Liste von Zusammenfassungen aller bekannten Double-Spend-Konflikte im Wallet zurück.

**`pub fn get_proof_of_double_spend(&self, proof_id: &str) -> Result<ProofOfDoubleSpend, String>`**

Ruft einen vollständigen `ProofOfDoubleSpend` (den Beweis für einen Double-Spend-Versuch) anhand seiner ID ab. Ideal, um Details anzuzeigen oder den Beweis zu exportieren.

**`pub fn create_resolution_endorsement(&self, proof_id: &str, notes: Option<String>) -> Result<ResolutionEndorsement, String>`**

Erstellt eine signierte Beilegungserklärung für einen Konflikt. Dies ist eine vom Wallet-Inhaber (dem Opfer) signierte Nachricht, die bestätigt, dass der Konflikt aus seiner Sicht gelöst wurde. Diese Operation verändert den Wallet-Zustand nicht.

**`pub fn run_storage_cleanup(&mut self) -> Result<CleanupReport, VoucherCoreError>`**

Führt eine manuelle Bereinigung des Speichers für Transaktions-Fingerprints durch. Dies löscht abgelaufene und, falls nötig, die ältesten Fingerprints, um das Speicherlimit einzuhalten.

---

#### Abfragen (Queries)
Diese Funktionen dienen dem reinen Lesezugriff auf das entsperrte Wallet und sind ideal, um die Benutzeroberfläche mit Daten zu befüllen. Sie benötigen keine Passwörter, da sie den Zustand des Wallets nicht verändern.

**`pub fn get_user_id(&self) -> Result<String, String>`**

Gibt die eindeutige ID des aktuellen Benutzers zurück (z.B. `did:key:z...`).

`pub fn get_voucher_summaries(
    &self,
    voucher_standard_uuid_filter: Option<&[String]>,
    status_filter: Option<&[VoucherStatus]>,
) -> Result<Vec<VoucherSummary>, String>`

Die Funktion akzeptiert die folgenden optionalen Filter:

* `voucher_standard_uuid_filter`: Ein optionaler Slice (`&[String]`) von UUIDs. Wenn vorhanden, werden nur Gutscheine zurückgegeben, deren Standard-UUID in diesem Slice enthalten ist. Wenn `None` oder ein leerer Slice übergeben wird, werden alle Gutscheine, unabhängig vom Standard, berücksichtigt.

* `status_filter`: Ein optionaler Slice (`&[VoucherStatus]`) von Status-Enums. Wenn vorhanden, werden nur Gutscheine mit einem dieser Status-Werte zurückgegeben. Wenn `None` oder ein leerer Slice übergeben wird, werden alle Gutschein-Status berücksichtigt.

Die Funktion ist ideal, um eine Übersicht aller Guthaben in einer UI anzuzeigen und diese nach bestimmten Kriterien zu filtern.

---

Die zurückgegebene `VoucherSummary`-Struktur enthält die folgenden Felder:

* `local_instance_id`: Die eindeutige, lokale ID der Gutschein-Instanz im Wallet.
* `status`: Der aktuelle Status des Gutscheins (`Active`, `Archived`, `Quarantined`, etc.).
* `valid_until`: Das Gültigkeitsdatum des Gutscheins im ISO 8601-Format.
* `creator_id`: Die eindeutige ID des ursprünglichen Erstellers (oft ein Public Key).
* `description`: Eine menschenlesbare Beschreibung des Gutscheins.
* `current_amount`: Der aktuelle verfügbare Betrag des Gutscheins als String.
* `unit`: Die Abkürzung der Währungseinheit (z.B. "m" für Minuten).
* `voucher_standard_name`: Der Name des Gutschein-Standards (z.B. "Minuto-Gutschein").
* `voucher_standard_uuid`: Die eindeutige Kennung (UUID) des Standards.
* `transaction_count`: Die Anzahl der Transaktionen, exklusive der initialen `init`-Transaktion.
* `guarantor_signatures_count`: Die Anzahl der vorhandenen Bürgen-Signaturen.
* `additional_signatures_count`: Die Anzahl der vorhandenen zusätzlichen, optionalen Signaturen.
* `has_collateral`: Ein boolesches Flag, das anzeigt, ob der Gutschein besichert ist.
* `creator_first_name`: Der Vorname des ursprünglichen Erstellers.
* `creator_last_name`: Der Nachname des ursprünglichen Erstellers.
* `creator_coordinates`: Die geografischen Koordinaten des Erstellers.


**`pub fn get_total_balance_by_currency(&self) -> Result<Vec<AggregatedBalance>, String>`**

Gibt einen Vektor von `AggregatedBalance`-Strukturen zurück, die das aggregierte Gesamtguthaben für jede Währung (z.B. "Minuto", "EUR") enthalten. Perfekt für eine Dashboard-Anzeige.

Die `AggregatedBalance`-Struktur enthält:
* `standard_name: String`: Der Name des Gutschein-Standards (z.B. "Minuto-Gutschein").
* `standard_uuid: String`: Die eindeutige UUID des Gutschein-Standards.
* `unit: String`: Die Währungseinheit (z.B. `Minuten`, `EUR`).
* `total_amount: String`: Der aufsummierte Gesamtbetrag als kanonischer String.




**`pub fn get_voucher_details(&self, local_id: &str) -> Result<VoucherDetails, String>`**

Ruft detaillierte Informationen zu einem einzelnen Gutschein ab, inklusive seiner Transaktionshistorie. `VoucherDetails` ist eine umfassende Struktur für eine Detailansicht.


## Gutschein-Struktur (Voucher)

Ein Gutschein ist ein JSON-Objekt mit folgenden Hauptfeldern:

- **`voucher_standard`**: Name, UUID, Hash der Definition.
- **`voucher_id`**: Eindeutige ID (Hash der Stammdaten).
- **`voucher_nonce`**: Zufälliger Nonce für Anonymität.
- **`creation_date`**: Erstellungsdatum (ISO 8601).
- **`valid_until`**: Gültigkeitsdatum (ISO 8601).
- **`nominal_value`**: Wert mit `unit`, `amount`, `abbreviation`, `description`.
- **`collateral`** (optional): Besicherung mit `unit`, `amount`, `type`, `redeem_condition`.
- **`creator_profile`**: Öffentliches Profil des Erstellers.
- **`signatures`**: Array von Signaturen (inkl. Bürgen) mit `signature_id`, `signer_id`, `signature`, `role`, `details`.
- **`transactions`**: Verkettete Liste von Transaktionen mit `t_id`, `prev_hash`, `t_type`, `sender_id`, `recipient_id`, `amount`, `sender_remaining_amount`, `sender_signature`.

## Fehlerbehandlung

Alle Funktionen geben `Result<T, String>` zurück. Häufige Fehler: Wallet gesperrt (`WalletLocked`), ungültige Eingaben, Speicherfehler.

**Locking-Fehler:** Wenn eine schreibende Operation fehlschlägt, weil ein anderer Prozess das Wallet verwendet, wird ein Fehler zurückgegeben (z.B. "Wallet-Sperre fehlgeschlagen: Wallet wird bereits von einem anderen Prozess (PID: 1234) verwendet."). Client-Anwendungen sollten diesen Fehler dem Benutzer anzeigen.

## Beispiel-Nutzung

```rust
use human_money_core::app_service::AppService;

let mut app = AppService::new(Path::new("/tmp/wallets")).unwrap();
app.create_profile("Mein Profil", &mnemonic, None, Some("user"), "pass").unwrap();