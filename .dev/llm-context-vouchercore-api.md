Kontext für Tauri-App-Entwicklung mit voucher_lib
Dies ist die Kontextdatei für die voucher_lib-Bibliothek, die für die Entwicklung von Client-Anwendungen, wie z. B. Tauri-Apps, aufbereitet wurde. Sie bietet eine präzise und vollständige Referenz für die öffentliche API, die zur Interaktion mit der Kernlogik notwendig ist.

1. Projekt & Zweck
   Projektname: voucher_lib

Zweck: Bereitstellung der Kernlogik für ein dezentrales, elektronisches Gutschein-System.

Ziel: Die Bibliothek stellt eine AppService-Fassade bereit, die alle komplexen Operationen kapselt und eine einfache, sichere Schnittstelle für die Anwendungsentwicklung bietet.

2. Architektur & Kernkonzepte
   Dezentraler Ansatz: Das System basiert auf digitalen Gutscheinen (im Grunde signierte Textdateien), die ihre eigene Transaktionshistorie enthalten. Es gibt kein zentrales, globales Ledger.

Offline-Fähigkeit: Transaktionen können offline durchgeführt werden, indem die aktualisierte Gutschein-Datei direkt an den neuen Halter übergeben wird.

Betrugserkennung: Das System ist darauf ausgelegt, "Double Spending" (das mehrfache Ausgeben desselben Gutscheins) kryptografisch nachweisbar zu machen. Die Client-Anwendung muss sich nicht um die Details der Erkennung kümmern, sondern nur auf die Ergebnisse reagieren, die der AppService liefert.

Entkoppelte Speicherung: Die voucher_lib nutzt ein Storage-Trait, um die Logik von der Speicherung zu trennen. Für Client-Anwendungen wird eine Standardimplementierung (FileStorage) bereitgestellt, die alle Daten sicher verschlüsselt im Dateisystem ablegt.

3. Öffentliche API: Das AppService-Modul
   Der AppService ist die einzige Schnittstelle, die für die Entwicklung der Client-Anwendung relevant ist. Er verwaltet den Zustand des Wallets (gesperrt/entsperrt) und stellt alle notwendigen Funktionen bereit.

pub struct AppService
Zustandsverwaltung: Hält den Zustand des Wallets (Locked oder Unlocked). Nach einem login ist der Service im Unlocked-Zustand und alle Operationen sind möglich. logout versetzt ihn zurück in den Locked-Zustand.

Automatische Speicherung: Alle Operationen, die den Wallet-Zustand verändern (z.B. ein Transfer), werden automatisch persistent gespeichert.

Hauptfunktionen (Befehle)
pub fn create_profile(mnemonic: &str, user_prefix: Option<&str>, password: &str) -> Result<(), String>

Erstellt ein komplett neues Benutzerprofil und Wallet und speichert es verschlüsselt. mnemonic und password sind obligatorisch. Ein optionales user_prefix kann für die Erstellung der DID verwendet werden. Der Service wird bei Erfolg in den Unlocked-Zustand versetzt.

pub fn login(mnemonic: &str, password: &str) -> Result<(), String>

Entsperrt ein existierendes Wallet mit der mnemonic_phrase und dem password und lädt es in den Speicher.

pub fn logout(&mut self)

Sperrt das Wallet und entfernt sensible Daten wie private Schlüssel aus dem Speicher. Diese Operation kann nicht fehlschlagen.

pub fn create_transfer_bundle(standard_definition: &VoucherStandardDefinition, local_instance_id: &str, recipient_id: &str, amount_to_send: &str, notes: Option<String>, archive: Option<&dyn VoucherArchive>, password: &str) -> Result<Vec<u8>, String>

Erstellt ein verschlüsseltes SecureContainer-Bundle für einen Transfer an einen Empfänger. Dies ist der Kernprozess zum Senden von Werten. Das Ergebnis (Vec<u8>) sind die serialisierten Daten, die an den Empfänger gesendet werden müssen (z.B. als Datei oder QR-Code). Die Wallet wird automatisch gespeichert.

pub fn receive_bundle(bundle_data: &[u8], archive: Option<&dyn VoucherArchive>, password: &str) -> Result<ProcessBundleResult, String>

Verarbeitet ein empfangenes Bundle. Die Funktion validiert die Transaktion, fügt die Gutscheine zum eigenen Wallet hinzu und gibt ein ProcessBundleResult zurück, das über den Erfolg und die Details der Transaktion informiert. Die Wallet wird automatisch gespeichert.

pub fn create_signing_request_bundle(local_instance_id: &str, recipient_id: &str) -> Result<Vec<u8>, String>

Erstellt ein Bundle, um eine Signaturanfrage für einen Gutschein an einen Bürgen zu senden. Diese Operation verändert den Wallet-Zustand nicht.

pub fn process_and_attach_signature(container_bytes: &[u8], password: &str) -> Result<(), String>

Verarbeitet eine empfangene losgelöste Signatur, fügt sie dem lokalen Gutschein hinzu und speichert den Wallet-Zustand.

Abfragen (Queries)
Diese Funktionen dienen dem reinen Lesezugriff auf das entsperrte Wallet und sind ideal, um die Benutzeroberfläche mit Daten zu befüllen. Sie benötigen keine Passwörter, da sie den Zustand des Wallets nicht verändern.

pub fn get_user_id(&self) -> Result<String, String>

Gibt die eindeutige ID des aktuellen Benutzers zurück (z.B. did:key:z...).

pub fn get_voucher_summaries(&self) -> Result<Vec<VoucherSummary>, String>

Gibt eine vereinfachte Liste aller aktiven Gutscheine im Wallet zurück. VoucherSummary ist eine Struktur, die für die Anzeige in einer Liste optimiert ist.

pub fn get_total_balance_by_currency(&self) -> Result<HashMap<String, String>, String>

Gibt eine Map zurück, die das aggregierte Gesamtguthaben für jede Währung (z.B. "Minuto", "EUR") enthält. Perfekt für eine Dashboard-Anzeige.

pub fn get_voucher_details(&self, local_id: &str) -> Result<VoucherDetails, String>

Ruft detaillierte Informationen zu einem einzelnen Gutschein ab, inklusive seiner Transaktionshistorie. VoucherDetails ist eine umfassende Struktur für eine Detailansicht.