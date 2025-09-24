### Kontext für Tauri-App-Entwicklung mit voucher_lib
Dies ist die Kontextdatei für die voucher_lib-Bibliothek, die für die Entwicklung von Client-Anwendungen, wie z. B. Tauri-Apps, aufbereitet wurde. Sie bietet eine präzise und vollständige Referenz für die öffentliche API, die zur Interaktion mit der Kernlogik notwendig ist.

### 1. Projekt & Zweck
**Projektname:** voucher_lib

**Zweck:** Bereitstellung der Kernlogik für ein dezentrales, elektronisches Gutschein-System.

**Ziel:** Die Bibliothek stellt eine `AppService`-Fassade bereit, die alle komplexen Operationen kapselt und eine einfache, sichere Schnittstelle für die Anwendungsentwicklung bietet.

### 2. Architektur & Kernkonzepte
**Dezentraler Ansatz:** Das System basiert auf digitalen Gutscheinen (im Grunde signierte Textdateien), die ihre eigene Transaktionshistorie enthalten. Es gibt kein zentrales, globales Ledger.

**Offline-Fähigkeit:** Transaktionen können offline durchgeführt werden, indem die aktualisierte Gutschein-Datei direkt an den neuen Halter übergeben wird.

**Betrugserkennung:** Das System ist darauf ausgelegt, "Double Spending" (das mehrfache Ausgeben desselben Gutscheins) kryptografisch nachweisbar zu machen. Die Client-Anwendung muss sich nicht um die Details der Erkennung kümmern, sondern nur auf die Ergebnisse reagieren, die der `AppService` liefert.

**Entkoppelte Speicherung:** Die `voucher_lib` nutzt ein `Storage`-Trait, um die Logik von der Speicherung zu trennen. Für Client-Anwendungen wird eine Standardimplementierung (`FileStorage`) bereitgestellt, die alle Daten sicher verschlüsselt im Dateisystem ablegt.

### 3. Öffentliche API: Das AppService-Modul
Der `AppService` ist die einzige Schnittstelle, die für die Entwicklung der Client-Anwendung relevant ist. Er verwaltet den Zustand des Wallets (gesperrt/entsperrt) und stellt alle notwendigen Funktionen bereit.

**`pub struct AppService`**
**Zustandsverwaltung:** Hält den Zustand des Wallets (`Locked` oder `Unlocked`). Nach einem `login` ist der Service im `Unlocked`-Zustand und alle Operationen sind möglich. `logout` versetzt ihn zurück in den `Locked`-Zustand.

**Automatische Speicherung:** Alle Operationen, die den Wallet-Zustand verändern (z.B. ein Transfer), werden automatisch persistent gespeichert.

#### Hauptfunktionen (Befehle)
**`pub fn new(storage_path: &Path) -> Result<Self, String>`**

Initialisiert einen neuen `AppService` im `Locked`-Zustand. Erstellt eine `FileStorage`-Instanz für den angegebenen Pfad. Das Verzeichnis wird bei Bedarf erstellt.

**`pub fn create_profile(mnemonic: &str, passphrase: Option<&str>, user_prefix: Option<&str>, password: &str) -> Result<(), String>`**

Erstellt ein komplett neues Benutzerprofil und Wallet und speichert es verschlüsselt. `mnemonic` und `password` sind obligatorisch. Eine optionale `passphrase` kann zur weiteren Absicherung der Schlüsselableitung verwendet werden. Ein optionales `user_prefix` kann für die Erstellung der DID verwendet werden. Der Service wird bei Erfolg in den `Unlocked`-Zustand versetzt.

**`pub fn login(password: &str) -> Result<(), String>`**

Entsperrt ein existierendes Wallet mit dem `password` und lädt es in den Speicher.

**`pub fn recover_wallet_and_set_new_password(mnemonic: &str, passphrase: Option<&str>, new_password: &str) -> Result<(), String>`**

Stellt ein Wallet mithilfe der Mnemonic-Phrase wieder her und setzt ein neues Passwort. Diese Funktion ist für den Fall vorgesehen, dass ein Benutzer sein Passwort vergessen hat. Bei Erfolg wird der Service in den `Unlocked`-Zustand versetzt.

**`pub fn logout(&mut self)`**

Sperrt das Wallet und entfernt sensible Daten wie private Schlüssel aus dem Speicher. Diese Operation kann nicht fehlschlagen.

**`pub fn create_new_voucher(standard_toml_content: &str, lang_preference: &str, data: NewVoucherData, password: &str) -> Result<Voucher, String>`**

Erstellt einen brandneuen Gutschein, fügt ihn zum Wallet hinzu und speichert den Zustand. Verifiziert zuerst die Standard-Definition, bevor der Gutschein erstellt wird.

**`pub fn create_transfer_bundle(standard_definition: &VoucherStandardDefinition, local_instance_id: &str, recipient_id: &str, amount_to_send: &str, notes: Option<String>, archive: Option<&dyn VoucherArchive>, password: &str) -> Result<Vec<u8>, String>`**

Erstellt ein verschlüsseltes `SecureContainer`-Bundle für einen Transfer an einen Empfänger. Dies ist der Kernprozess zum Senden von Werten. Das Ergebnis (`Vec<u8>`) sind die serialisierten Daten, die an den Empfänger gesendet werden müssen (z.B. als Datei oder QR-Code). Die Wallet wird automatisch gespeichert.

**`pub fn receive_bundle(bundle_data: &[u8], standard_definitions_toml: &HashMap<String, String>, archive: Option<&dyn VoucherArchive>, password: &str) -> Result<ProcessBundleResult, String>`**

Verarbeitet ein empfangenes Bundle. Die Funktion validiert die Transaktion, fügt die Gutscheine zum eigenen Wallet hinzu und gibt ein `ProcessBundleResult` zurück, das über den Erfolg und die Details der Transaktion informiert. Die Wallet wird automatisch gespeichert. Der Caller muss die benötigten Standard-Definitionen als TOML-Strings bereitstellen.

**`pub fn create_signing_request_bundle(local_instance_id: &str, recipient_id: &str) -> Result<Vec<u8>, String>`**

Erstellt ein Bundle, um eine Signaturanfrage für einen Gutschein an einen Bürgen zu senden. Diese Operation verändert den Wallet-Zustand nicht.

**`pub fn create_detached_signature_response_bundle(voucher_to_sign: &Voucher, signature_data: DetachedSignature, original_sender_id: &str) -> Result<Vec<u8>, String>`**

Erstellt eine losgelöste Signatur als Antwort auf eine Signaturanfrage. Diese Operation wird vom Bürgen aufgerufen und verändert dessen Wallet-Zustand nicht.

**`pub fn process_and_attach_signature(container_bytes: &[u8], standard_toml_content: &str, password: &str) -> Result<(), String>`**

Verarbeitet eine empfangene losgelöste Signatur, fügt sie dem lokalen Gutschein hinzu und speichert den Wallet-Zustand.

**`pub fn save_encrypted_data(name: &str, data: &[u8], password: &str) -> Result<(), String>`**

Speichert einen beliebigen Byte-Slice verschlüsselt auf der Festplatte. Diese Methode nutzt den gleichen sicheren Verschlüsselungsmechanismus wie das Wallet selbst. Ideal, um anwendungsspezifische Daten (z.B. Konfigurationen, Kontakte) sicher abzulegen.

**`pub fn load_encrypted_data(name: &str, password: &str) -> Result<Vec<u8>, String>`**

Lädt und entschlüsselt einen zuvor gespeicherten, beliebigen Datenblock. Aus Sicherheitsgründen wird das Passwort für jede Leseoperation benötigt, um den Entschlüsselungsschlüssel abzuleiten.

#### Hilfsfunktionen (Statische Methoden)
Diese Funktionen sind Teil des `AppService`, benötigen aber keinen initialisierten Zustand (weder `Locked` noch `Unlocked`) und können jederzeit aufgerufen werden.

**`pub fn generate_mnemonic(word_count: u32) -> Result<String, String>`**

Erzeugt eine neue, kryptografisch sichere BIP-39 Mnemonic-Phrase (Seed-Wörter). Ideal, um einem neuen Benutzer bei der Profilerstellung eine Phrase vorzuschlagen. `word_count` ist typischerweise 12 oder 24.

**`pub fn validate_mnemonic(mnemonic: &str) -> Result<(), String>`**

Überprüft eine gegebene Mnemonic-Phrase auf ihre Gültigkeit (korrekte Wörter, gültige Prüfsumme). Dies ist nützlich, um dem Benutzer bei der Eingabe zur Wiederherstellung eines Wallets sofortiges Feedback zu geben, bevor der eigentliche Login-Versuch unternommen wird.


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


**`pub fn get_total_balance_by_currency(&self) -> Result<HashMap<String, String>, String>`**

Gibt eine `HashMap` zurück, die das aggregierte Gesamtguthaben für jede Währung (z.B. "Minuto", "EUR") enthält. Perfekt für eine Dashboard-Anzeige oder einen Gesamtüberblick über die Guthaben.

Die Map hat das Format:
* **Schlüssel (`String`):** Die Währungseinheit (z.B. `Minuten`, `EUR`).
* **Wert (`String`):** Der aufsummierte Gesamtbetrag als kanonischer String.




**`pub fn get_voucher_details(&self, local_id: &str) -> Result<VoucherDetails, String>`**

Ruft detaillierte Informationen zu einem einzelnen Gutschein ab, inklusive seiner Transaktionshistorie. `VoucherDetails` ist eine umfassende Struktur für eine Detailansicht.