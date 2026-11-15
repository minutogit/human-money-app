# APP-UX-09 — Simple & Expert Mode (Einfacher & Erweiterter Modus)

> **Status:** ✅ Umgesetzt (P0–P4, P6–P10) · **Erstellt/aktualisiert:** 2026-11-15 · **App-Version:** 0.5.6
> **Grundlage:** UI-Tour (32 Screens, `test-results/tour/`), 5 UX-Bereichsanalysen (Einstellungen, Kontakte/WoT, Marktplatz, Geld-Abläufe, Hilfe), Architektur-Analyse und Datenvolumen-Analyse der Sync-Verteilung.
> **Sprache:** Planungsdokument auf Deutsch. Code, Kommentare und Commits auf Englisch.

---

## 0. Ziel & Leitprinzip

**Ziel:** Ein Anfänger soll die App ohne Vorwissen sicher nutzen können. Wer mehr will, schaltet jederzeit mit einem Klick alle Details frei.

> **Leitprinzip:** Im einfachen Modus entscheidet der Nutzer nur, was nur er wissen kann: **wem** er etwas gibt, **wie viel**, **wem er vertraut** und **wie weit man ihn sehen soll**. Alles andere ist ein sicherer Standardwert.

**Grundsätze**

1. **Eine App, ein Schalter.** Es gibt keine zweite Oberfläche und keine doppelten Abläufe. Die Fenster bleiben dieselben, im einfachen Modus zeigen sie nur weniger.
2. **Einheitlich statt Einzellösungen.** Jede Ansicht nutzt dieselben Bausteine (Abschnitt 2). Keine Ansicht baut eigene `if (expert)`-Logik.
3. **Standard: einfach.** Neue Profile starten im einfachen Modus. Der Wechsel ist jederzeit möglich, und die App merkt sich den Modus pro Profil.
4. **Nichts geht verloren.** Voreinstellungen schreiben echte Einstellungen. Wer im erweiterten Modus Einzelwerte ändert, sieht im einfachen Modus „Benutzerdefiniert“.
5. **Sicherheit wird nie wegautomatisiert** (Abschnitt 6).
6. **Zero-Compat:** Scheinoptionen und Duplikate werden gelöscht, nicht nur ausgeblendet. Ein Beispiel ist die WoT-Stufe „4 Hops“, siehe 3.2.

---

## 1. Wie der Nutzer es erlebt

| Situation | Verhalten |
|---|---|
| **Erster Start / neues Profil** | Am Ende des Onboardings kurze Info: „Du nutzt die **einfache Ansicht**. Alle Optionen findest du jederzeit oben über **Erweitert**.“ Es gibt keine Pflichtfrage. Danach folgt die Frage zur Sichtbarkeit (Nah/Mittel/Weit, Standard *Mittel*). |
| **Umschalten** | Kleiner Segment-Schalter **`Einfach │ Erweitert`** in der Kopfzeile (`Header.tsx`, neben `StatusChip`), auf jeder Seite sichtbar. Dieselbe Option steht in den Einstellungen unter „Ansicht“. Auf dem Handy wird er zum Icon-Toggle. |
| **Übergang** | Weiches Ein- und Ausblenden mit Tailwind-Transition, ohne Seitenwechsel. Scroll-Position und eingegebene Formularwerte bleiben erhalten. |
| **Im Formular** | Unter einfachen Feldern steht ein dezenter Link **„Mehr Optionen ▾“** (`AdvancedDisclosure`). Damit lassen sich einzelne Expertenfelder öffnen, ohne den globalen Modus zu wechseln. |
| **Abweichende Werte** | Ist eine versteckte Option nicht auf Standard, erscheint im einfachen Modus ein Badge **„● Angepasst“** am jeweiligen „Mehr Optionen“-Link. |
| **Login-Bildschirm** | Nutzt den zuletzt verwendeten Modus aus dem `localStorage`, damit nichts flackert. Expertenoptionen beim Anlegen (Wortanzahl, Passphrase, Präfix) bleiben hinter „Expertenoptionen“ wie heute. |

---

## 2. Architektur (einheitlich, modular)

### 2.1 Zustand & Speicherung

| Ebene | Umsetzung | Datei |
|---|---|---|
| Typ | `export type UiMode = 'simple' \| 'expert'` | `src/types/api.ts` |
| Persistenz (pro Profil, verschlüsselt) | `AppSettings.uiMode: UiMode` (Standard `simple`), Rust-Feld mit `#[serde(default)]` | `src/types/api.ts:76ff`, `src-tauri/src/settings.rs:53ff` |
| Kaltstart-Cache | `localStorage['humoco_ui_mode']`, wird vor dem Entsperren synchron gelesen (Muster wie bei `humoco_sidebar_more_open`) | `UiModeContext.tsx` |
| Context | `UiModeProvider` umschließt die App, ein Wechsel schreibt sofort in den `localStorage` und asynchron per `save_app_settings` | **neu** `src/context/UiModeContext.tsx` |

```ts
// src/context/UiModeContext.tsx
export interface UiModeContextValue {
  mode: UiMode;
  isExpert: boolean;
  setMode(mode: UiMode): void;
  toggle(): void;
}
export function useUiMode(): UiModeContextValue;
```

Der Entwicklermodus (`devModeUnlocked`, 7 Klicks) bleibt **unabhängig** davon und ist nur für Debug-Funktionen da. Der erweiterte Modus ist kein Easter-Egg.

### 2.2 Bausteine (die einzigen Stellen, die `useUiMode` lesen dürfen)

| Baustein | Zweck | Verhalten einfach / erweitert |
|---|---|---|
| `<Advanced>` | **Ausblenden**: rein technische Elemente (Terminal/JSON, Benchmark, Server-Tokens) | Einfach: nicht gerendert · Erweitert: sichtbar |
| `<AdvancedDisclosure title isCustom>` | **Einklappen**: Formularfelder mit sicherem Standardwert | Einfach: zugeklappt („Mehr Optionen ▾“, ggf. Badge „Angepasst“) · Erweitert: offen. Bei Validierungsfehlern klappt er automatisch auf und fokussiert das Feld. |
| `<TechId value kind>` | **Anders darstellen**: `did:key`, Bundle-, Proof- und Geräte-IDs | Einfach: Identicon und kurzer Fingerabdruck (`…ALICE`) mit Kopierknopf · Erweitert: volle ID |
| `<PresetOrCustom presets value onChange renderCustom>` | **Anders darstellen**: Voreinstellung statt Rohwerten (Sichtbarkeit, Visitenkarten-Profil, Sicherung) | Einfach: Kacheln für die Voreinstellungen (+ „Benutzerdefiniert“, wenn abweichend) · Erweitert: Kacheln **und** darunter die Einzelregler |
| `PageLayout` Prop `advancedActions` | Kopfzeilen-Knöpfe für Experten | Einfach: im „⋯“-Menü gebündelt · Erweitert: als Knöpfe sichtbar |
| `NavItem.mode` | Navigation | Menüpunkte mit `mode: 'expert'` werden im einfachen Modus ausgeblendet |
| `HelpTopic.audience` | Hilfe | Themen mit `'expert'` stehen im einfachen Modus nur in der Gruppe „Für Fortgeschrittene“ (eingeklappt) |
| `<UiModeToggle>` | Der Schalter | in `Header.tsx` und in den Einstellungen unter „Ansicht“ |

**Neue Dateien:** `src/context/UiModeContext.tsx`, `src/components/ui/Advanced.tsx`, `src/components/ui/AdvancedDisclosure.tsx`, `src/components/ui/TechId.tsx`, `src/components/ui/PresetOrCustom.tsx`, `src/components/ui/UiModeToggle.tsx`.

**Ersetzt:** die 4 Einzellösungen zum Auf- und Zuklappen (`Sidebar.tsx:286` „Mehr“, `NetworkProtectionCard.tsx:213`, `AdvancedTab.tsx:66`, `HelpContentRenderer` Deep-Dive) werden auf `AdvancedDisclosure` bzw. eine gemeinsame `Disclosure`-Basis umgestellt.

### 2.3 Regeln gegen Wildwuchs

1. **Kein direktes `useUiMode()` / `isExpert` in Views.** Erlaubt ist es nur in `src/components/ui/*`, `Sidebar.tsx`, `Header.tsx`, `HelpCenterView.tsx` und `UiModeContext.tsx`. Durchgesetzt wird das mit einem Vitest-Lint-Test (`src/__tests__/uiModeLint.test.ts`, Muster wie `glossaryLint`).
2. **Entscheidungsbaum für jedes Element:**
   - Sicherheitsrelevant oder handlungsnotwendig → **immer sichtbar**.
   - Technisches Detail ohne Entscheidung → `<Advanced>` (ausblenden).
   - Entscheidung mit sicherem Standardwert → `<AdvancedDisclosure>` (einklappen).
   - Mehrere technische Regler mit einem gemeinsamen Zweck → `<PresetOrCustom>`.
   - Technische Kennung → `<TechId>`.
3. **Kein Element existiert nur im einfachen Modus.** Der erweiterte Modus ist immer eine Obermenge.
4. **Texte:** Sichtbare Texte im einfachen Modus müssen den Glossar-Lint bestehen. Expertentexte dürfen präzise Fachbegriffe nutzen (Lint mit Allowlist für `*.expert.*`-Keys).

---

## 3. Sichtbarkeit / Reichweite: 3 Stufen

> **Entschieden (2026-11-15):** Es gibt **3 Stufen** (Nah / Mittel / Weit), **kein Hop 4**. Die Gründe stehen in 3.1. Die Architektur bleibt erweiterbar, weil die Zuordnung zentral liegt (3.3).

### 3.1 Befund (Sync-Analyse, Kurzfassung)

- **Weitergabe läuft über Kopien (Re-Publishing).** Karten (mit Inseraten) und WoT-Listen von Freundesfreunden erreichen einen Nutzer nur, weil ein Freund sie entschlüsselt, ggf. kürzt (Redaktion) und in **seinem eigenen**, für seine Kontakte verschlüsselten Slot neu veröffentlicht. Fremde Slots kann man weder abfragen noch entschlüsseln.
- **Der Speicher bleibt linear, aber nur, weil Obergrenzen abschneiden.** Die Client-Konstanten (`SYNC_CARD_MAX_BYTES = 64 KiB`, `SYNC_WOT_MAX_BYTES = 128 KiB`, `src-tauri/src/settings.rs:7-8`), `MAX_FORWARDED_CARDS = 20` (`crates/humoco-market/src/model.rs:34`) und die WoT-Klassenbudgets von ~43 KB (`commands/sync.rs:2037`) schneiden alles Überzählige ab.
- **Folge heute:** Hop 2 kommt nur teilweise an. **Hop 3 kommt praktisch nicht an**, weil die Weiterleitung nach Vertrauensstufe sortiert (`slot.rs:54-61`) und die eigenen Freunde des Weiterleitenden alle Plätze belegen.
- **WoT** ist im Drop-Sync ohnehin auf 3 begrenzt (`humoco-web-of-trust/src/sync.rs:309`, `clamp(1, 3)`). **Die Option „4 Hops“ im WoT-Panel ist eine Scheinoption** und wird gelöscht.
- **Hop 4 verworfen:** Bei ~20 Kontakten pro Person wären es ~35.000 Erreichbare und ~10.000 Karten. Das übersteigt den Kartenspeicher (2.000), es gäbe ständig wechselnde Verweislisten, große Metadaten-Lecks und keinen Vertrauenswert (WoT endet bei 3). Der Nutzen ist gering.
- **Bilder** werden nie kopiert, sondern nur auf Klick vom Server des Autors geladen. Damit sieht der Server des Autors die Abrufe auch von Hop-2/3-Lesern (bekanntes, akzeptiertes Metadaten-Risiko).
- **Die Behebung des Datenflusses** (ETag beim Slot-Abruf, WoT-Unter-Slots pro Distanzklasse, Budget pro Hop, Verweise nur für den öffentlichen Kartenteil) **ist nicht Teil dieses Plans**, sondern gehört in einen separaten Sync-Plan (`docs/plans/SYNC_REACH_PLAN.md` im Meta-Repo, folgt).

### 3.2 Die 3 Stufen (einheitlich für die ganze App)

Eine einzige Voreinstellung **„Wie weit soll man dich finden können?“**, wiederverwendet in Onboarding, Kontakte, Marktplatz und Einstellungen.

| | 🏡 **Nah** | 🏘️ **Mittel** *(Standard)* | 🌄 **Weit** |
|---|---|---|---|
| **Kurztext** | „Nur Menschen, die du direkt kennst.“ | „Deine Kontakte und deren Kontakte.“ | „Dein erweitertes Netzwerk über mehrere Ecken – größter Nutzen.“ |
| **Gut für** | Maximale Privatsphäre | Alltag im Ort / in der Gemeinschaft | Aktive Tauscher, Anbieter, regionale Märkte |
| **WoT-Weitergabe** (`ownListMaxHops`) | 1 | 2 | 3 |
| **Wer darf meine Ablage lesen** (`shareMinLevel`) | Freund (2) | Bekannt (1) | Bekannt (1) |
| **Visitenkarte & Inserate** (`maxHops`) | 1 | 2 | 3 |
| **Bilder automatisch laden (Leserseite)** | Hop 1 | bis Hop 2 | bis Hop 2, darüber auf Klick |

**Hinweise in der Oberfläche:**
- Für alle Stufen: „Wer weniger teilt, bekommt auch weniger Empfehlungen zurück.“ (Reciprocal Routing)
- Bei *Weit*: „Deine Visitenkarte kann von Menschen gefunden werden, die du nicht kennst. Straße und genaue Position bleiben trotzdem privat.“
- **Ehrliche Formulierungen:** „kann dich finden“ statt „sieht dich“. Bis der Sync-Plan umgesetzt ist, kommt Hop 3 im Marktplatz kaum an. Die Texte dürfen nichts versprechen, was die Technik heute nicht hält.

**Unveränderlich:** Stufe 0 („Als Mensch bestätigt“) wird **nie** weitergegeben, unabhängig von der gewählten Stufe.

**Im erweiterten Modus:** gleiche Kacheln, darunter getrennte Regler (WoT-Hops 1–3, Karten-Hops 1–3, `shareMinLevel` 0–3, Bild-Grenze). Sobald ein Wert abweicht, zeigt die Kachelreihe „Benutzerdefiniert“.

### 3.3 Entkopplung UI ↔ Technik (Pflicht)

- **Die Oberfläche kennt nur Stufen-IDs** (`'near' | 'medium' | 'far'`). Welche Werte pro Datenart dahinterstehen, steht **ausschließlich** in **einer** Zuordnung: Frontend `src/config/reachPresets.ts` bzw., bevorzugt, im Backend als Single Source of Truth mit dem Command `apply_reach_preset(preset)` / `get_reach_preset()`.
- `apply_reach_preset` setzt **atomar** `ownListMaxHops`, `shareMinLevel`, Karten-`maxHops` und `mediaAutoLoadMaxHops`. `get_reach_preset` liefert die erkannte Stufe oder `custom`.
- Ändert der spätere Sync-Plan die technischen Werte oder fügt neue Datenarten hinzu, **ändert sich nur diese Zuordnung**. Komponenten und Texte bleiben unverändert.
- Der Typ ist erweiterbar ausgelegt (weitere Stufen oder eine spätere Funktion „Im erweiterten Netzwerk suchen“ wären möglich). Es wird aber **keine** vierte Stufe gebaut.

### 3.4 Leserseite: Datensparen

Neue Einstellung **„Bilder automatisch laden“** (`mediaAutoLoadMaxHops`), im einfachen Modus an die Stufe gekoppelt, im erweiterten Modus einzeln wählbar:
- bis zur Grenze: automatisch laden,
- darüber: Kategorie-Platzhalter mit Knopf „Bild laden“,
- kombiniert mit dem bestehenden **WLAN-only**, das auch für Bilder gilt.

### 3.5 Sofort-Korrektur (Teil von P0)

| # | Maßnahme | Ort |
|---|---|---|
| T5 | WoT-Hop-Option 4 löschen (Zero-Compat): UI auf 1–3, Policy/Validierung auf max. 3. Vorher prüfen, ob Datei- oder Reciprocal-Routing den Wert 4 nutzt; falls ja, ebenfalls auf 3 begrenzen. Default `ownListMaxHops` auf 2 (= *Mittel*). | `WotNetworkOptionsPanel.tsx`, `AddressBook.tsx:235/602`, `humoco-wot` Policy |

Alle weiteren technischen Maßnahmen (Bilder-Grenze auf Autorseite, Relevanzfilter, WoT-Priorisierung, adaptive Poll-Intervalle, ETag, Verweise) gehören in den **Sync-Plan**.

---

## 4. Bildschirm für Bildschirm

Legende: **E** = im einfachen Modus sichtbar · **A** = automatisch / Standardwert · **⋯** = eingeklappt (`AdvancedDisclosure`) · **X** = nur im erweiterten Modus (`Advanced`) · **ID** = `TechId`

### 4.1 Navigation (Sidebar)

| Bereich | Einfach | Erweitert |
|---|---|---|
| Hauptpunkte | **Übersicht** · **Verlauf** · **Kontakte** · **Marktplatz** | zusätzlich **Meine Gutscheine** (Detailliste mit Filtern) |
| Unten | Hilfe · Einstellungen · Abmelden | zusätzlich Konflikte (immer), Support, Fehler melden |
| Konflikte | **Nur als Banner bzw. Menüpunkt, wenn offen** (sicherheitsrelevant, immer sichtbar) | dauerhaft als Menüpunkt |
| Kopf | Name, Avatar, Sperren | zusätzlich **ID** |

Doppelten Titel „Human Money App“ in Sidebar **und** Kopfzeile auflösen. In der Kopfzeile steht künftig der Seitentitel bzw. der `UiModeToggle`.

### 4.2 Übersicht (Dashboard und Wallet zusammengelegt)

- **E:** Guthaben pro Währung (große Kacheln), drei Aktionen **Senden / Empfangen / Erstellen**, offene Anfragen (Bürgschaft, Kontaktaustausch), Sicherungs-Ampel, Erste-Schritte-Checkliste.
- **E:** „Meine Gutscheine anzeigen ▾“ öffnet eine einfache Liste (Betrag, Aussteller, läuft ab in …).
- **A:** Ein fehlendes Profil-Siegel wird beim Start **still erneuert**. Ein Banner erscheint nur bei echtem Handlungsbedarf, dann gelb und mit verständlichem Satz statt Rot mit „mathematische Fallen“.
- **X:** **ID** im Kopf, Umschaltung Netto/Verfügbar, Suche/Sortierung/Filter (Wallet-Ansicht), Integritätsbericht.
- **E:** Leere Zustände immer mit Aktion („Noch keine Gutscheine – [Erstellen] [Empfangen]“).

### 4.3 Senden *(größter Hebel)*

**Bestätigter Fehler:** Das Betragsfeld wird nur gerendert, wenn vorher eine Währung angeklickt wurde (`src/components/send/AssetInventory.tsx:71`). Die automatische Auswahl (`autoSelectGravitational`, `useVoucherSelection.ts:58`) existiert, wird aber dadurch blockiert.

**Einfacher Ablauf: An wen? → Wie viel? → Senden**

| Schritt | Einfach | Erweitert |
|---|---|---|
| 1 Empfänger | **E** Kontaktliste mit Suche (Avatar + Name), „Code einfügen / QR scannen“ | + manuelle ID-Eingabe |
| 2 Betrag | **E** Betragsfeld **immer sichtbar**, Währung daneben (vorausgewählt bei einer Währung) | + Währungs-Pills |
| 3 Zweck | **E** optional | gleich |
| Absender | **A** eigenes Profil | **X** Absendername, anonym senden |
| Gutscheinauswahl | **A** automatisch (bald ablaufende zuerst, Wechselgeld automatisch, **nie Test und Echt gemischt**) | **⋯** „Gutscheine selbst wählen“ (Liste, Teilbeträge) |
| Privatsphäre-Modus | **A** Standard des Gutschein-Standards | **⋯** |
| Visitenkarte mitsenden | **A** aus | **⋯** |
| Zahlungsanforderung einfügen | **⋯** | Kopfzeilen-Knopf |
| Erfolgsseite | **E** „✓ Zugestellt an Bob“ (wenn per Sync zugestellt), sonst klare Übergabe-Optionen | + Datei, QR, Base64, Empfänger-**ID** |

### 4.4 Empfangen

- **E:** Titel **„Empfangen“**. Zuerst **„Mein Code“** (QR mit optionalem Betrag = Zahlungsanforderung), darunter „Datei öffnen“ und Drag & Drop.
- **X:** Dateiendungen (`.transfer/.ask/.sig`), Kassenmodus (PoS) mit Kauf-ID.

### 4.5 Gutschein erstellen

| Feld | Einfach | Erweitert |
|---|---|---|
| Gutscheinart (Standard) | **E** Auswahl mit Kurzerklärung (z. B. „Minuto – 1 Minute deiner Zeit“) | gleich, plus Standards verwalten |
| Betrag | **E** | gleich |
| Test / Echt | **E** Standard *Test*, mit einem Satz Erklärung | gleich |
| Gültigkeit | **A** aus dem Standard | **⋯** |
| Ausstellerangaben | **A** aus dem Profil, eine Zeile: „Deine Angaben (Alice Musterfrau, Freiburg) werden übernommen [Bearbeiten]“ | ganzes Formular |
| Geschlecht | **nur abfragen, wenn der Standard es verlangt** (Regel zum Geschlechterverhältnis der Bürgen; der Ablauf existiert schon) | gleich |
| Sicherheiten, Koordinaten | **X** | sichtbar |

### 4.6 Gutscheindetails

- **E:** Betrag, Status, Ablauf („läuft ab in 2 Jahren“), Aussteller (Name, Ort), Vertrauens-Badge mit Satz, Bürgen.
- **X:** **ID**, „Online-Status prüfen“ (heute **doppelt** vorhanden, eins entfernen), JSON/Terminal `>_`, Signatur- und Verlaufsdetails.
- **Fehler / Bereinigung:** „Geschlechterorientierung“ → „Geschlecht“ und ausblenden, wenn unbekannt. „% · Vertrauensnetz“ ohne Zahl in der echten App prüfen.

### 4.7 Verlauf (Zahlungsverlauf und Aktivitäten zusammengelegt)

- **E:** Liste (an/von Name, Betrag, Zweck, Datum), Filter *Alle / Eingänge / Ausgänge*, Suche.
- **⋯:** Währungs- und Zeitraumfilter.
- **X:** `#bundle_…`-Kennungen, „Ereignis-Protokoll“ (als Aufklappbereich pro Eintrag statt eigener Route `Activities.tsx`, die dann entfällt), CSV-Export.
- **Fehler:** Die Summenzeile mischt Währungen (+10 / −0,00 FreeTaler, −60 Minuto fehlt). Lösung: eine Summe pro Währung.

### 4.8 Kontakte & Vertrauen

| Element | Einfach | Erweitert |
|---|---|---|
| Kopfzeile | **E** „Mein Profil teilen“ (vereint QR-Code und Visitenkarte), „+ Kontakt“ | + Netzwerk & Reichweite, Aktualisieren, Vertrauenspersonen (über `advancedActions`) |
| Kontakt hinzufügen | **E** ein intelligentes Feld „Code, Link oder ID einfügen“ + [QR scannen]; „Andere Wege ▾“ (Datei, manuell) | 4 Kacheln wie heute |
| Kontaktkarte | **E** Avatar, Name, Stufe, höchstens 1 Tag | + **ID**, Sync-Status-Badges, alle Tags, Sync-Hosts |
| Stufe vergeben | **E** 4 menschliche Stufen | gleich |
| Anfragen zum Kontaktaustausch (Handshakes) | **E** Ja/Nein mit vorgeschlagener Stufe | gleich |
| Sichtbarkeit | **E** die 3 Stufen (3.2) | + Einzelregler |
| Vertrauensliste exportieren, Prüfsiegel/Verbände, Sync-Adressen pro Kontakt | **X** | sichtbar |

### 4.9 Marktplatz

**Inserat erstellen**

| Feld | Einfach | Erweitert |
|---|---|---|
| Angebot/Gesuch, Titel, Beschreibung | **E** | gleich |
| Preis | **E** Chips *Kostenlos · Verhandlungssache · Minuto · FreeTaler* + Betrag | + Freitext |
| Bild | **⋯** „Mehr Angaben“ | sichtbar |
| Ortsunabhängig / Versand | **⋯** | sichtbar |
| Laufzeit | **A** 90 Tage | 30/90/180 |
| Anfragen erlauben | **A** aus dem Profil (doppelten Schalter **löschen**) | im Profil |
| Verlängern | **E** 1-Klick „+90 Tage“ direkt auf der Karte | + Modal mit Auswahl |

**Visitenkarte:** `PresetOrCustom` mit 3 Profilen statt 11 Pills.

| Profil | Öffentlich | Nur Kontakte | Niemand |
|---|---|---|---|
| **Privat** | Name, Ort (~1 km) | Organisation, E-Mail, Telefon, Bild, Web | Straße, Geschlecht |
| **Lokal & offen** *(Standard)* | Name, Organisation, Gemeinschaft, Ort, Bild, Web | Straße, E-Mail, Telefon | Geschlecht |
| **Geschäftlich** | alles außer den Feldern rechts | Straße* | Geschlecht |

\* *Feste Leitplanke:* Straße und Hausnummer nie öffentlich. Für öffentlich sichtbare Positionsdaten ist die Genauigkeit fest auf ~1 km gesetzt (Stufe 2). Stufe 3/4 (100 m / 10 m) gibt es nur im erweiterten Modus und nur für „Kontakte“.

- **A:** Gültigkeit der Karte 2 Jahre, Kartenspeicher 2.000, unbekannte Ersteller = an, GPS-Unschärfe automatisch aus dem Ort.
- **E:** **Veröffentlichen bleibt eine bewusste Aktion** (Standard: aus).
- **Begriffe:** *Visitenkarte* = „Was ich dauerhaft anbiete“ · *Pinnwand* = „Aktuelle Anzeigen“.
- **Umkreisfilter ohne eigenen Ort:** ausgegraut, mit dem Hinweis „Ort im Profil hinterlegen“.
- **X:** Vorschau-Perspektive, Speicher-Cap, Feinabstimmung pro Feld, GPS-Präzision.

### 4.10 Einstellungen

**Einfacher Modus: eine Seite mit 6 Blöcken**

| Block | Inhalt |
|---|---|
| **Profil** | Name und Visitenkarte bearbeiten, Sprache |
| **Datensicherung** | Status-Ampel, „Jetzt sichern“, Notfallblatt, Vertrauenspersonen, Hinweis „Welche Sicherung brauche ich?“ |
| **Sichtbarkeit** | die 3 Stufen (3.2) |
| **Abgleich** | *ein* Schalter „Automatisch abgleichen“ mit Status („Verbunden mit sync.humoco.de“). Er bündelt Post, Marktplatz, Vertrauensliste und Sicherung, plus WLAN-only und „Bilder automatisch laden“. |
| **Ansicht** | `UiModeToggle`, automatische Sperre (10 Min) |
| **Gefahrenzone** | Profil vom Gerät löschen |

**Erweiterter Modus:** die bisherigen Tabs (Profil, Sicherheit & Sicherung, Sync-Dienst, Erweitert) mit allen Optionen. **Neu einsortiert:** „Eigenen Sync-Dienst betreiben“ wandert von *Erweitert* zu *Sync-Dienst*, und die Marktplatz-Optionen werden zusammengeführt.

**Feste Standardwerte (einfacher Modus):** `backupProtectionMode = mnemonic_only`, `autoBackupEnabled = true`, `backupFrequency = after_mutation`, `syncBackupEnabled = true`, `postAcceptFromContacts = true`, `marketSyncEnabled = true`, `wotShareViaSync = true`, `activeNetwork = mainnet`, `marketSendCardOnPayment = false`, Sync-Dienste von Kontakten automatisch übernehmen.

**Aufräumen (Zero-Compat):**
- `syncBackupEnabled` erscheint doppelt (Sicherung **und** Sync) und bleibt nur bei der Sicherung.
- „Einstellungen speichern“-Knöpfe werden entfernt, überall wird sofort automatisch gespeichert.
- Die Kopfzeile der Einstellungen wird an die anderen Seiten angeglichen. Der Text „Sicherheitsprotokolle, Netzwerkstandards“ wird ersetzt.

### 4.11 Konflikte & Sperren

- **ForkLock-Sperre:** Ton entschärfen („Dein Profil wurde auf einem anderen Gerät benutzt“), „Epoche“ → **X**, Hilfe-Link ergänzen, Hinweis auf Übergabe bzw. Unterkonto. Im englischen Text „Wallet“ korrigieren.
- **Konfliktliste und -details:** Im einfachen Modus drei Fragen beantworten: *Was ist passiert? Ist mein Gutschein noch gültig? Was soll ich tun?* Hashes, „SST“, „Ephemeral Key“ und „Local Override“ → **X**. `.hmcproof` an das Glossar anpassen.

---

## 5. Hilfe

### 5.1 Neue Themen

| # | Thema | Kernaussage |
|---|---|---|
| H1 | **Welche Sicherung brauche ich?** | „Die 12 Wörter retten deine Identität – deine Gutscheine und Kontakte brauchen zusätzlich eine Sicherungsdatei oder den Sync-Dienst.“ *(wichtigstes Thema)* |
| H2 | **Was tun bei Geräteverlust?** | Checkliste in 3 Schritten |
| H3 | **Wiederherstellung durch Kontakte** | „3–5 Vertrauenspersonen helfen dir, dein Profil ohne Zettel zurückzuholen.“ |
| H4 | **Das Notfallblatt** | Wozu, wo aufbewahren, was steht drauf |
| H5 | **Gutscheinarten: Minuto, FreeTaler & Co.** | Was eine Gutscheinart (Standard) ist, was 1 Minuto bedeutet |
| H6 | **Ablauf & Verfall** | Was am Ablaufdatum passiert, Rolle der Bürgen |
| H7 | **Offline bezahlen & Restrisiko** | „Offline nimm von Fremden nur an, was jemand absichert, dem du vertraust.“ |
| H8 | **Mehrere Geräte** | Warum die App sperrt, Übergabe und Unterkonten |
| H9 | **Profil-Siegel** | ein Satz, ohne Alarm |
| H10 | **Bürge oder Empfehlung?** | Haftung vs. Vertrauen |
| H11 | **Wie weit soll man dich finden können?** | die 3 Stufen, Nutzen vs. Privatsphäre; ersetzt `wotReach` + `wotSharing` |
| H12 | **Einfache und erweiterte Ansicht** | was sich ändert, wo der Schalter ist |

### 5.2 Umschreiben (Jargon raus, Technik in den `deepDiveKey`)

`mnemonic` → „Wiederherstellungswörter“ · `offlineProtection` (ohne SST V4) · `l2Protection` → „Echtzeit-Prüfung beim Bezahlen“ · `privacy`/`settingsPrivacy` „Stealth“ → „Diskrete Zahlung“ · `benutzerId` · `voucherCreator` · `voucherTrust` · `conflicts` · `passphrase` · `subaccount`/`handover` · `deviceBinding` · `backup`.

### 5.3 Struktur des Hilfe-Centers

`HelpTopic` erhält `group` und `audience: 'everyone' | 'expert'`.

1. **Erste Schritte** – Was ist Human Money?, dein Profil, Test vs. Echt, einfache/erweiterte Ansicht
2. **Bezahlen & Gutscheine** – Gutscheine statt Geld, Gutscheinarten, Senden/Empfangen, offline bezahlen, Ablauf
3. **Vertrauen & Kontakte** – Vertrauensnetz, 4 Stufen, Bürge oder Empfehlung, Sichtbarkeit
4. **Marktplatz** – Visitenkarte, Inserate, Sichtbarkeit der Felder
5. **Sicherheit & Notfall** – Welche Sicherung?, Geräteverlust, Vertrauenspersonen, Notfallblatt, mehrere Geräte
6. **Häufige Fragen** – „Ist das Krypto?“, „Was kostet es?“, „Kann ich betrogen werden?“, „Was passiert, wenn der Sync-Dienst weg ist?“
7. **Für Fortgeschrittene** *(audience: expert; im einfachen Modus eingeklappt)* – Sync-Dienst & eigener Server, Echtzeit-Prüfung (L2), Konflikte im Detail, Profil-Siegel, Kassenmodus, Gateways & Tokens

---

## 6. Sicherheits-Leitplanken (gelten in beiden Modi)

| Thema | Regel |
|---|---|
| Vertrauensstufen 1–3 | nie automatisch; nur Stufe 0 bei persönlichem Austausch vorausgewählt |
| Kontaktanfragen von Fremden | nie still annehmen (Schutz vor Fake-Identitäten, Sybil) |
| Bürgschaft signieren | immer ausdrücklich bestätigen (echte Haftung) |
| Test- und Echt-Gutscheine | nie in einem Transfer mischen, auch nicht bei automatischer Auswahl |
| Automatische Gutscheinauswahl | streng linear, keine doppelte Verwendung eines Gutscheins in parallelen Entwürfen |
| Veröffentlichen von Visitenkarte und Inseraten | immer eine bewusste Aktion |
| Straße, genaue Position, Telefon | nie öffentlich als Standard; Positionsunschärfe fest ≥ ~1 km für öffentlich |
| Visitenkarte bei Zahlung | Standard aus (Anonymität wie bei Bargeld) |
| Stufe-0-Einstufungen | nie weitergegeben, unabhängig von der Sichtbarkeitsstufe |
| Sicherungs-Ampel | bleibt in beiden Modi sichtbar, bis eine Sicherung eingerichtet ist |
| Offene Konflikte / ForkLock | immer sichtbar (Banner) |
| Exklusive L2-Server, Testnet | nur im erweiterten Modus, mit Warnung |
| Profil löschen, Vertrauenspersonen widerrufen | Passwort und Warnung |

---

## 7. Umsetzungsphasen

| Phase | Inhalt | Aufwand | Nutzen |
|---|---|---|---|
| **✅ P0 – Schnelle Korrekturen** | Betragsfeld beim Senden immer zeigen; doppeltes „Online-Status prüfen“; leere Zustände mit Aktion; Integritäts-Banner entschärfen; Verlaufssumme pro Währung; Version im Tour-Mock aus `package.json`; **T5** (WoT-Hop 4 löschen, Default 2) | ½–1 Tag | ⭐⭐⭐ |
| **✅ P1 – Grundgerüst** | `uiMode` (TS + Rust), `UiModeContext`, `UiModeToggle` in Kopfzeile und Einstellungen, Bausteine `Advanced`, `AdvancedDisclosure`, `TechId`, `PresetOrCustom`, `PageLayout.advancedActions`, `NavItem.mode`; 4 Einzellösungen zum Auf- und Zuklappen ersetzen; `uiModeLint`-Test | 1–1½ Tage | ⭐⭐⭐ |
| **✅ P2 – Jargon ausblenden** | `TechId` überall (Dashboard, Kontakte, Details, Senden, Verlauf), JSON/Terminal, Bundle-IDs, Ereignis-Protokoll per `Advanced` | ½ Tag | ⭐⭐⭐ |
| **✅ P3 – Senden & Empfangen** | Ablauf „An wen → Wie viel → Senden“, automatische Auswahl, Erfolgsseite, Empfangen mit „Mein Code“ zuerst | 1½–2 Tage | ⭐⭐⭐ |
| **✅ P4 – Sichtbarkeitsstufen** | 3 Stufen Nah/Mittel/Weit (3.2), zentrale Zuordnung + Backend-Commands `apply_reach_preset` / `get_reach_preset` (3.3), `ReachPresetSelector` (Onboarding, Kontakte, Marktplatz, Einstellungen), Leser-Einstellung „Bilder automatisch laden“ (3.4), Status „Benutzerdefiniert“. Mit heutigen Sync-Werten bauen; die volle Wirkung von *Weit* kommt mit dem Sync-Plan, das blockiert aber nicht. | 1½ Tage | ⭐⭐⭐ |
| ~~P5 – Technik-Track~~ | **Verschoben in den separaten Sync-Plan** (`docs/plans/SYNC_REACH_PLAN.md`, Meta-Repo). Nicht Teil dieses Plans. | – | – |
| **✅ P6 – Einstellungen einfach** | Einseitige Ansicht mit 6 Blöcken, ein Schalter „Abgleich“, Duplikate und Speichern-Knöpfe entfernen, Neu-Einsortierung | 1 Tag | ⭐⭐ |
| **✅ P7 – Navigation & Übersicht** | Dashboard und Wallet zusammenlegen (Wallet-Detail = expert), Verlauf übernimmt Activities, Sidebar 4 + 3 | 1½ Tage | ⭐⭐ |
| **✅ P8 – Kontakte & Marktplatz** | Kartenansicht entrümpeln, Kopfzeilen, intelligentes Hinzufügen-Feld, „Mein Profil teilen“, Preis-Chips, 1-Klick-Verlängern, 3 Visitenkarten-Profile | 1½–2 Tage | ⭐⭐ |
| **✅ P9 – Gutschein erstellen** | kurze Form (Art, Betrag, Test/Echt), Profil-Zeile, Geschlecht nur bei Bedarf | 1 Tag | ⭐⭐ |
| **✅ P10 – Hilfe & Konflikttexte** | H1–H12, Umschreibungen, Gruppen und FAQ, `audience`-Flag, Texte für Konflikte und ForkLock | 1–1½ Tage | ⭐⭐ |

**Reihenfolge:** P0 → P1 → P2 → P3 → P4 → P6 → P7 → P8 → P9 → P10.
**Gesamt:** ca. 11–14 Arbeitstage.

### Definition of Done (jede Phase)
- `npm run test` (Vitest) grün, inkl. neuem `uiModeLint` und erweitertem `beginnerFlows.test.tsx`: Im einfachen Modus erscheinen keine `Advanced`-Elemente.
- `npm run i18n:check` (alle 5 Sprachen), Glossar-Lint.
- `cargo clippy` ohne Warnungen und Rust-Tests bei Backend-Änderungen (`settings.rs`, `humoco-market`, `humoco-wot`).
- `npm run tour:auto` in **beiden Modi**: `tourBridge.uiMode`, Tour-HUD-Taste `e` zum Umschalten, `tour-catalog.mjs` je Step optional `uiMode`.
- `STATUS.md` aktualisiert (Task `APP-UX-09`, Version).

---

## 8. Entscheidungen

| # | Frage | Entscheidung |
|---|---|---|
| D1 | Hop 4 / Stufe „Sehr weit“? | **Entfällt.** 3 Stufen, Architektur bleibt erweiterbar (3.3) |
| D2 | Standardstufe für neue Profile | *Mittel* (WoT 2; heute 3) |
| D3 | Stufe im Onboarding abfragen oder still *Mittel* setzen? | Abfragen (ein Bildschirm, *Mittel* vorausgewählt) |
| D4 | Bilder-Grenze auf Autorseite (`image_max_hops`) | → Sync-Plan; hier nur leserseitig (3.4) |
| D5 | Position des Schalters | Kopfzeile (`Header.tsx`), zusätzlich in den Einstellungen; Standard = einfach, keine Pflichtfrage beim Start |
| D6 | „Meine Gutscheine“ im einfachen Modus | in die Übersicht integrieren; Detailansicht nur im erweiterten Modus |

---

## Anhang A — Referenzen (Code)

- Fehler beim Betragsfeld: `src/components/send/AssetInventory.tsx:71`
- WoT-Hop-Clamp: `humoco-web-of-trust/src/sync.rs:309`
- WoT-Standard 3: `src/components/AddressBook.tsx:235, 602`
- Karten-Limits: `crates/humoco-market/src/model.rs:14` (5 Inserate), `:33` (2.000 Karten), `:34` (20 weitergeleitet)
- WoT-Slot-Limit: `src-tauri/src/settings.rs:8` (128 KiB)
- Bilder auf Anfrage: `src-tauri/src/commands/market.rs:989–1025`
- Settings: `src/types/api.ts:76–109`, `src-tauri/src/settings.rs:53–117`, `src/services/settingsService.ts`
- Bestehende Einzellösungen zum Auf- und Zuklappen: `Sidebar.tsx:286`, `NetworkProtectionCard.tsx:213`, `AdvancedTab.tsx:66`, `HelpContentRenderer.tsx`
- Hilfe-Registry: `src/help/helpTopics.ts:48–56`
- Tour: `src/utils/tourBridge.ts:6–14`, `scripts/ui-tour.mjs:27–45`, `scripts/tour-catalog.mjs:1163` (Version fest auf 0.5.1)
