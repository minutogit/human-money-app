# Human Money App - Testing Strategie

Diese Dokumentation hält die 3 fundamentalen Test-Typen fest, die notwendig sind, um manuelle Tests nach Code-Änderungen auf ein Minimum zu reduzieren. Sie bilden das Fundament der Qualitätssicherung für diese Tauri-App.

## 1. Frontend-Komponententests (Vitest + React Testing Library + `mockIPC`)
- **Fokus:** Testet das Verhalten der Benutzeroberfläche (UI) isoliert.
- **Tools:** Vitest, React Testing Library, `@tauri-apps/api/mocks`.
- **Zweck:** Stellt sicher, dass das UI korrekt auf Nutzerinteraktionen (Klicks, Eingaben) reagiert und die richtigen Daten zur Verarbeitung an das Backend senden *würde*.
- **Vorteil:** Extrem schnell. Schützt vor kaputten Formularen, fehlerhaftem Rendering und React-State-Fehlern.

## 2. Backend Tauri-Command Integrationstests (`assert_ipc_response`)
- **Fokus:** Testet die Schnittstelle (Bridge) zwischen Frontend und Backend.
- **Tools:** Rusts nativer Test-Runner (`cargo test`), `tauri::test::mock_builder`.
- **Zweck:** Simuliert eingehende JSON-Anfragen vom Frontend und verifiziert, dass die Rust-Schicht die Datenstrukturen exakt so versteht, korrekt verarbeitet und das erwartete Resultat (oder den Fehler) zurückgibt.
- **Vorteil:** Verhindert "Silent Failures" (stille Fehler), bei denen Mismatches in den JSON-Typen zwischen TypeScript und Rust dazu führen, dass Befehle ignoriert werden oder abstürzen.

## 3. End-to-End (E2E) "Happy Path" Tests (Playwright)
- **Fokus:** Testet das reibungslose Zusammenspiel aller echten Systemkomponenten (UI + Rust Backend + echte Cryptography im Core).
- **Tools:** Playwright + `tauri-plugin-playwright`.
- **Zweck:** Ein automatisierter Durchlauf aus Nutzersicht (App starten -> Einloggen -> Gutschein erstellen/senden -> Kontostand prüfen).
- **Vorteil:** Höchste Zuversicht. Wenn dieser Test besteht, ist die Applikation als Ganzes funktionsfähig.

---

## Best Practices & Erweiterungen

Dieser 3-Schichten-Ansatz (häufig als "Testing Trophy" bezeichnet) ist branchenweiter **Best Practice** für Tauri/Electron-Apps. 

**Weitere sinnvolle, aber optionale Testarten (für später):**
1. **Property-Based Testing (Fuzzing):** Im Rust-Core Tausende zufällige Inputs generieren, um sicherzustellen, dass das Backend niemals abstürzt (Panic), sondern fehlerhafte Eingaben immer sauber abfängt.
2. **Visuelle Regressionstests:** Playwright kann Screenshots des UI machen und bei Änderungen vergleichen. Warnt dich, wenn eine globale Tailwind-CSS-Änderung aus Versehen das Layout von Buttons zerschossen hat.
3. **Accessibility (A11y) Tests:** Automatische Prüfungen, ob Formulare auch für Screenreader verständlich bleiben.
