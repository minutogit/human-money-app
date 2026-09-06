# TODO / Offene Aufgaben

## Priorität: Core Synchronisation & Breaking Changes (ab human_money_core v0.3)

> **Wichtig:** Ab Version 0.3 von `human_money_core` gab es mehrere Breaking Changes. Bei auftretenden Fehlern muss zwingend `human_money_core` (`../../human-money-core/src/app_service/`) analysiert und angepasst werden.

- [ ] **Breaking Change — Archive-Erstellung (Host-App / Tauri):**
  - Gutschein-Archive müssen künftig zwingend mit `FileVoucherArchive::new_secure(path, wallet_passwort)` gebaut/instanziiert werden.
  - Prüfen und anpassen in den Tauri-Command-Handlern (z. B. `src-tauri/src/commands/transfers.rs`).
- [ ] **Core-Beobachtungen (report-only, ohne deterministischen Test):**
  - Fehlendes `fsync` in `save_wallet`.
  - TOCTOU-Race im PID-Locking (`.wallet.lock`).

---

## Weitere geplante Aufgaben (siehe auch [STATUS.md](STATUS.md))

- [ ] Internationalization (i18n) – German / English Vervollständigung (MVP)
- [ ] Session Timeout UX Optimierung (Activity Log visibility etc.) (MVP)
- [ ] L2 Sync Hardening (Post-MVP)
- [ ] Trust Level Badges für Kontakte (Post-MVP)
- [ ] BLE/NFC Stranger Discovery (Post-MVP)
