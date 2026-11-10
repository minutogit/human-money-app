# UX-Glossar: Human Money App

Dieses Glossar definiert die verbindliche Terminologie für die Benutzeroberfläche der **Human Money App** (Deutsch und Englisch). Ziel ist maximale Klarheit für Anfänger: **Ein Begriff pro Konzept, überall im UI.**

> [!IMPORTANT]
> **Verbindliche Schreib- und Vokabelregeln:**
> 1. **Kein „Wallet“ im UI:** Der Begriff „Wallet“ bzw. „Geldbörse“ wird in normalen Benutzeroberflächen-Texten vollständig vermieden. Verwende **Profil** für den geschützten Behälter (Login, Backup, Wiederherstellung, Profil reparieren) und **Meine Gutscheine** für den Inhalt (Guthaben, Scheine, Transaktionen). Im Code bleibt `wallet` als technischer Bezeichner erhalten.
> 2. **Striktes Verbot von „bürgen“ / „haften“ im Vertrauensnetzwerk (WoT):** Die Begriffe *bürgen* und *haften* sind ausnahmslos formalen Gutschein-Bürgen vorbehalten (Garanten mit kryptographischer Signatur auf einem Gutschein). Im Web of Trust (WoT) und bei Kontakten dürfen sie **niemals** verwendet werden. Stattdessen formulieren: *„Du stehst freiwillig ein Stück weit für diese Person ein. Wenn sie betrügt, fällt das auch ein wenig auf dich zurück.“*
> 3. **Keine technischen Krypto-Kürzel in Standard-Texten:** Keine DIDs (`did:key`), keine „Hops“, keine Dateiendungen (`.wot`). Diese technischen Details gehören ausschließlich in Tooltips unter „Erweitert“ bzw. Experten-Hilfedialoge.
> 4. **Sync-Dienst bleibt Sync-Dienst:** Keine wechselnden Bezeichnungen wie „Ablage“, „Upload-Ziel“, „Verbindungsdienst“ oder generisch „Server“.

---

## Terminologie-Tabelle

| Konzept | Begriff DE | Begriff EN | Verboten / Ersetzt | Ein-Satz-Erklärung & Regeln |
|---|---|---|---|---|
| **Verbindungs- und Abgleichdienst** | **Sync-Dienst** | **Sync Service** | Sync-Ablage, Ablage, Upload-Ziel, Verbindungsdienst, Server (generisch) | *„Ein Sync-Dienst gleicht deine verschlüsselten Daten mit deinen Kontakten ab, auch wenn ihr nicht gleichzeitig online seid.“* Immer genau diese Bezeichnung verwenden. |
| **Dauerhaftes Profil & Kontakt-Austausch** | **Visitenkarte** | **Business Card** (oder **Contact Card**) | Kontaktkarte, Marktplatz-Karte, Karte | Dauerhaftes, kryptographisch signiertes Profil für den Kontakt-Austausch im Adressbuch und Marktplatz. |
| **Dauerhaftes Angebot auf Visitenkarte** | **Fähigkeit** | **Skill** | Dauerangebot, Leistung, Listing | Dauerhaftes Können oder ständiges Angebot einer Person auf ihrer Visitenkarte (z. B. Reparatur, Beratung, Handwerk). |
| **Befristeter Marktplatz-Eintrag** | **Inserat** (Art: **Angebot** oder **Gesuch**) | **Ad** (**Offer** / **Request**) | Listing, Eintrag, Post | Befristeter Eintrag auf der Pinnwand mit definierter Laufzeit (30, 90 oder 180 Tage). |
| **Marktplatz-Übersicht** | **Pinnwand** | **Board** (oder **Notice Board**) | Feed, Marktplatz-Feed, Börse | Übersicht aller aktuellen Inserate aus dem persönlichen Umfeld. |
| **Vertrauenseinstufungen** | **Vertrauensliste** | **Trust List** | Vertrauensdatei, .wot-Datei, Web of Trust (in Standard-Texten) | Signierte Liste mit deinen persönlichen Vertrauenseinstufungen für Kontakte. |
| **Eigene Kennung** | **Benutzer-ID** | **User ID** | DID, did:key, Public Key | Eindeutige Kennung deines Profils. Technische Bezeichnungen wie `did:key` nur unter „Erweitert“ oder in Tooltips anzeigen. |
| **Datenbehälter (Konto/Sicherheit)** | **Profil** | **Profile** | Wallet, Geldbörse | Der verschlüsselte Schutzbehälter für Schlüssel, Kontakte und Einstellungen (z. B. „Profil entsperren“, „Profil sichern“, „Profil wiederherstellen“). |
| **Guthaben & Scheine** | **Meine Gutscheine** | **My Vouchers** | Wallet, Geldbörse, Guthabenkonto | Der eigentliche Wert-Inhalt (Gutscheine, erstellte Versprechen, empfangene Zahlungen). |
| **Netzwerk-Distanz / Reichweite** | **Kontakte / Freunde** (`+ deren Kontakte`, `Freunde von Freunden`, `Stufen`) | **Contacts / Friends** (`+ their contacts`, `Friends of friends`, `Degrees`) | 1 Hop, Hop-2, Hops | Natürliche Sprache statt Netzwerk-Jargon: „Nur meine Kontakte“, „+ deren Kontakte“, „Freunde von Freunden“, „weitere Stufen“. |
| **Sicherheits- / Konsensvorfall** | **Konflikte** | **Conflicts** | Betrugsmeldungen, Fraud Reports, Sicherheitswarnung (generisch) | Widersprüchliche Ausgaben oder Versionskonflikte, die bereinigt werden müssen. |
| **WoT Stufe 0** | **Als Mensch bestätigt** | **Verified as human** | Level 0, Stufe 0, Existenzbestätigung | *„Ich weiß, dass es diese Person wirklich gibt, kenne sie aber nicht näher.“* (Bestätigt reale Existenz ohne Weiterleitung von Vertrauen). |
| **WoT Stufe 1** | **Bekannt** | **Acquaintance** | Level 1, Stufe 1 | *„Ich kenne die Person.“* |
| **WoT Stufe 2** | **Freund** | **Friend** | Level 2, Stufe 2 | *„Ich kenne die Person gut und vertraue ihr.“* |
| **WoT Stufe 3** | **Enger Vertrauter** | **Close confidant** | Level 3, Stufe 3 | *„Ich vertraue ihr voll und ganz.“* |
| **Verantwortung im Vertrauensnetz (WoT)** | **Für Kontakt einstehen** | **Backing a contact** | bürgen, haften | **Niemals „bürgen“ oder „haften“ im WoT verwenden!** Stattdessen: *„Du stehst freiwillig ein Stück weit für diese Person ein. Wenn sie betrügt, fällt das auch ein wenig auf dich zurück.“* |
| **Gutschein-Garantie (Layer 1)** | **Bürge / Bürgschaft** | **Guarantor / Guarantee** | Vertrauensperson | Nur für formale L1-Bürgen auf Gutscheinen mit kryptographischer Signatur verwenden, die im Einlösungsausfall haften. |
| **Sichtbarkeit von Feldern** | **Alle / Nur meine Kontakte / Niemand** | **Everyone / My Contacts Only / Nobody** | Public, Contacts, Private, Versteckt | Drei klare Sichtbarkeitsstufen für Angaben auf der Visitenkarte mit selektiver Offenlegung (Selective Disclosure). |
