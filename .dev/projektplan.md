#clientWalletapp
# Angepasster Projektplan: Human Money App MVP

Dieser Plan ist so strukturiert, dass die Kernfunktionalität schnellstmöglich implementiert und getestet werden kann. Jede Phase baut auf der vorherigen auf und priorisiert Features nach dem unmittelbaren Mehrwert für den Endnutzer.

### Phase 1: Die Kern-Transaktion (End-to-End-Test)

**Ziel:** Die grundlegende Fähigkeit der App sicherstellen: Ein Nutzer kann einen Gutschein erstellen, ihn an einen anderen Nutzer senden und dieser kann ihn empfangen.

1. **UI-Grundgerüst & Responsivität (Mobile-Ready von Anfang an)**

    - **Aufgabe:** Das grundlegende Layout-System mit Tailwind CSS aufbauen. Alle von Anfang an erstellten Komponenten müssen responsiv sein, um auf Desktop und mobilen Geräten gut auszusehen.

    - **Begründung:** Eine solide, responsive Basis ist entscheidend und vermeidet aufwändige Umbauten später. Dies ist eine Grundlage für alle folgenden UI-Arbeiten.

2. **Gutschein-Erstellung implementieren**

    - **Frontend:** `CreateVoucher.tsx`-Komponente mit einem einfachen Formular für die `create_new_voucher`-Funktion erstellen.

    - **Backend:** Den `tauri::command` für `create_new_voucher` implementieren.

    - **Ergebnis:** Ein Nutzer kann einen neuen Gutschein in seinem Wallet erzeugen.

3. **Gutscheine Senden & Empfangen**

    - **Frontend (Senden):** `SendVoucher.tsx`-Komponente implementieren. Diese ermöglicht die Auswahl eines Gutscheins und die Eingabe der Empfänger-ID. Sie ruft `create_transfer_bundle` auf und stellt das Ergebnis (`Vec<u8>`) als QR-Code und als Datei-Download zur Verfügung.

    - **Frontend (Empfangen):** `ReceiveVoucher.tsx`-Komponente erstellen. Diese erlaubt das Scannen eines QR-Codes oder den Upload der Bundle-Datei und ruft damit `receive_bundle` auf.

    - **Backend:** Die Tauri-Befehle für `create_transfer_bundle` und `receive_bundle` implementieren.

    - **Ergebnis:** Der komplette Transaktionszyklus ist funktionsfähig.


### Phase 2: Benutzerfreundlichkeit & Verwaltung (Das "Nutzbar-Machen")

**Ziel:** Die App von einem reinen Funktions-Prototyp zu einem Werkzeug machen, das man tatsächlich verwenden möchte.

1. **Verbesserte Gutschein-Übersicht & Details**

    - **Frontend:** Die `Dashboard.tsx`-Komponente erweitern, um die `VoucherSummary`-Liste anzuzeigen. Ein Klick auf einen Gutschein öffnet eine neue `VoucherDetails.tsx`-Ansicht, die alle relevanten Daten des Gutscheins (inkl. Transaktionshistorie) anzeigt.

    - **Backend:** Den `get_voucher_details`-Befehl via Tauri verfügbar machen.

    - **Begründung:** Nutzer müssen sehen und verwalten können, was sie besitzen. Dies ist essenziell für das Vertrauen in die App.

2. **Kontaktverwaltung (Einfaches Adressbuch)**

    - **Aufgabe:** Eine einfache UI zur Verwaltung von Kontakten (Name + User-ID) erstellen. Beim Senden eines Gutscheins kann aus dieser Liste ein Empfänger ausgewählt werden.

    - **Implementierung:** Die Kontaktdaten werden über die bereits vorhandenen Befehle `save_encrypted_data` und `load_encrypted_data` sicher im Nutzerprofil gespeichert.

    - **Begründung:** Dies ist ein **enormer** UX-Gewinn. Niemand möchte mit kryptischen `did:key:...`-Schlüsseln manuell arbeiten. Dieses Feature hat höchste Priorität nach der Kernfunktionalität.


### Phase 3: Erweiterte Funktionalität

**Ziel:** Komplexere, aber wichtige Features des Gutschein-Systems implementieren.

1. **Bürge-Funktionalität (Guarantor-Flow)**

    - **Frontend:** Komponenten implementieren, die Signaturanfragen anzeigen und es dem Nutzer (als Bürge) ermöglichen, diese zu signieren (`create_detached_signature_response_bundle`) und dem ursprünglichen Sender zurückzuschicken. Der Sender kann die Signatur dann an seinen Gutschein anhängen (`process_and_attach_signature`).

    - **Backend:** Die entsprechenden Tauri-Befehle implementieren.

    - **Begründung:** Erschließt einen wichtigen Anwendungsfall des Systems, ist aber für die einfache Wertübertragung nicht zwingend notwendig.


### Phase 4: Ausbau & Skalierbarkeit (Langfristige Ziele)

**Ziel:** Die App für eine breitere Nutzerbasis vorbereiten und die Architektur für die Zukunft rüsten.

1. **Internationalisierung (i18n)**

    - **Frontend:** Eine Bibliothek wie `i18next` integrieren, um alle UI-Texte übersetzbar zu machen.

    - **Backend:** Sicherstellen, dass die `lang_preference` korrekt an die `human_money_core` übergeben wird.

2. **Dynamisches UI für Gutschein-Standards**

    - **Frontend:** Die `CreateVoucher.tsx`-Komponente so erweitern, dass die Eingabefelder dynamisch anhand der TOML-Definition des Gutschein-Standards generiert werden.

    - **Backend/Speicher:** Die TOML-Dateien der Standards lokal und verschlüsselt mit `save/load_encrypted_data` speichern, damit sie nicht jedes Mal neu geladen werden müssen.

3. **Architektur & Performance**

    - **Datenbank-Integration:** Evaluierung und optionale Implementierung einer In-Memory-SQLite-Datenbank, um die Performance bei sehr vielen Gutscheinen zu verbessern. Die DB-Datei selbst wird weiterhin verschlüsselt mit den `save/load_encrypted_data`-Methoden gespeichert.

    - **Performance-Optimierung:** Analyse von Ladezeiten bei großen Datenmengen und ggf. Implementierung von Paginierung oder Lazy Loading.