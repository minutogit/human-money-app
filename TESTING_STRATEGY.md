# Human Money App — Testing Strategy

This document summarizes the automated testing and verification layers that guard `human-money-app` across Rust, TypeScript, i18n, and visual UI flows.

## 1. Frontend Component, Service & Policy Tests (Vitest + React Testing Library)
- **Tools:** Vitest, React Testing Library, `@tauri-apps/api/mocks`.
- **Coverage:** Component behavior, `BigInt` split formatting, i18n/glossary linting (`glossaryLint.test.ts`, `uiModeLint.test.ts`), and static TypeScript-to-Rust IPC contract checks (`tauriCommands.test.ts`, `commandAccessParity.test.ts`, `eventParity.test.ts`, `serviceReachability.test.ts`).

## 2. Backend & Crypto Tests (Cargo / `src-tauri` + `crates/humoco-market`)
- **Tools:** `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check`.
- **Coverage:**
  - `crates/humoco-market`: SD-Card signing/redaction, Shamir social recovery, `HMSLOT02` compression/AEAD, PoST envelopes, and QR SVG encoding.
  - `src-tauri`: Sync-Lock lifecycle & green-rule checkout (`checkout.rs`, `l2_lifecycle.rs`), SSRF-hardened `sync_client.rs`, `AccessMode` enforcement, backup/recovery flows, and Tauri IPC DTO parity.

## 3. UI Tour & Visual Regression Check (`npm run tour` / `npm run tour:auto`)
- **Catalog:** 31 mock-backed screens (`1.1`–`5.6`) defined in [`scripts/tour-catalog.mjs`](scripts/tour-catalog.mjs).
- **Headless Audit (`npm run tour:auto`):** Captures Desktop & Mobile screenshots, flags horizontal overflows, and catches console errors or unmocked IPC calls into `test-results/tour/audit-report.md`.

## 4. Single Verification Gate (`./run-tests.sh --quiet`)
- Run `./run-tests.sh --quiet` (`--frontend-only`/`-f` or `--backend-only`/`-b` for focused runs) to execute Tauri version sync, TypeScript, ESLint, `npm run i18n:check` (5 locales), Vitest, Cargo fmt/Clippy, and Rust tests in one pass.
