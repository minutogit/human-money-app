# Human Money App (`human-money-app`) — Agent Instructions

You are an Engineer on **Human Money App**, the Desktop and Mobile wallet host built with **Tauri v2**, **React 19**, **TypeScript**, and **Tailwind CSS v4**.

## Language & Pre-Launch Policies

- **User / Maintainer chat:** Always reply in **German**.
- **Code, comments, skills, docs & git commits:** Idiomatic **English** (except localized UI strings and existing German guides).
- **Pre-Launch Zero Backwards Compatibility:** The HuMoCo network is not live yet. Make clean breaks (*Subtraction before Construction*) across Tauri commands, DTOs, local storage schemas, and backup formats; never retain `#[serde(alias = ...)]` or fallback shims. Scope discipline: delete directly superseded code; report unrelated legacy code for approval rather than sweeping it unprompted.

## Core Paradigm & UX Compass

1. **Bottom-Up Trust Economy (Mobile-Primary Reality):** Designed for local/regional value promises (Minuto, FreeTaler) and open to worldwide use. Desktop and Mobile (Android/iOS via Tauri v2) are co-equal, with **Mobile increasingly primary in everyday life** — always respect narrow screens, touch ergonomics, low bandwidth, and offline QR/file/NFC/BLE flows.
2. **NO Blockchain, NO Global Ledger:** There are NO blocks, consensus, mining, staking/speculation coins, gas fees, custody accounts, or global credit scores.
3. **Hard Security in Background, Radical Simplicity in Foreground:** Never weaken cryptographic guarantees, while keeping technical complexity hidden in **Simple Mode** (technical IDs/details belong under `<Advanced>` / `<TechId>` in Expert Mode). Strictly follow [`docs/UX_GLOSSARY.md`](docs/UX_GLOSSARY.md) (never use `"Wallet"`, `"DID"`, `"Hops"`, or `"bürgen"/"haften"` in WoT copy — enforced by `glossaryLint.test.ts` and `uiModeLint.test.ts`).
4. **Trust People, Verify Math:** Hard-block ONLY on math/crypto failures or proven fraud (`KnownOffender` / equivocation). Warn transparently on unknown WoT paths or offline L2 and let the human decide. Distinguish formal voucher guarantor liability from subjective WoT social responsibility.
5. **Offline-First L1 + Optional `humoco-sync` Comfort & Launch L2:** Core payment/voucher/WoT flows must work 100% offline. `humoco-sync` (PHP shared hosting for decentralization + Rust daemon) provides optional zero-knowledge comfort infrastructure (backups, marketplace, cards, WoT, async drops) plus launch **Sync-Lock** PoS protection (`LockCommittee`, Green-Rule traffic light). `human-money-node` is frozen (`NODE-004`) as an optional future worldwide L2 upgrade path ([`../docs/adrs/001-layer2-anchor-extensibility-hmnn.md`](../docs/adrs/001-layer2-anchor-extensibility-hmnn.md)).

## Strict 4-Layer Architecture & Boundaries

- **`human_money_core` (L1 Sovereignty):** Canonical `snake_case` voucher/wallet/SST-V4 engine. NEVER alter Core for UI convenience; live API reference is [`../human-money-core/src/app_service/api_readme.md`](../human-money-core/src/app_service/api_readme.md) and `mod.rs`.
- **`humoco-wot` (Subjective Trust):** Standalone `#![forbid(unsafe_code)]` crate (`TrustRegister`, distance-class Drop-Sync, `BinaryFuse64` seals, `ServerOffenderList`).
- **`crates/humoco-market` (App & Comfort Crypto Crate):** `#![forbid(unsafe_code)]` crate for marketplace SD-Cards, QR SVG, GF(256) Shamir social recovery, `HMSLOT02` slots, and PoST envelopes — intentionally separate from `human_money_core` so Core stays lean and `humoco-market` remains extractable into its own repo later.
- **`src-tauri/` (Thin Host & IPC Bridge):** 17 command modules (`commands/mod.rs`), 14 `camelCase` DTO modules (`src-tauri/src/models/`), `services/` (incl. `sync_engine/`, `post_service/`), `state/`, `l2_lifecycle.rs`, `sync_client.rs`, `sync_zip.rs`. Zero Core business logic; enforce `AccessMode::{Full, DailyPay, ReceiveOnly}` + `COMMAND_ACCESS_POLICY`, `OneShotPinGrant`, `SessionToken` async generation guards, `AuthorizedSession`, `EncryptedBatchCommit`, and fail-closed Z01 `isGoodsReleaseAuthorized`.
- **`src/` (React 19 Presentation Layer):** 3 contexts (`SessionContext`, `NavigationContext`, `UiModeContext`), typed `src/services/` + `commandPolicy.ts`, `src/hooks/`, `src/types/`, 15 domain subdirs under `src/components/`, and 5 locales (`en`, `de`, `es`, `fr`, `it`). Zero business/crypto logic in React; exact `BigInt` decimal formatting.

## Styling, DTOs, CI & Security Guardrails

- **DTO Parity:** Core is `snake_case`; `src-tauri/src/models/*.rs` (`#[serde(rename_all = "camelCase")]`) and `src/types/*.ts` MUST match 1:1 with zero shims (`commandAccessParity.test.ts`, `eventParity.test.ts`, `tauriCommands.test.ts`).
- **Tailwind CSS v4:** `@tailwindcss/vite` ONLY (`src/App.css` uses `@import "tailwindcss";` + `@config "../tailwind.config.js";`). **NO `postcss.config.js`.**
- **Cargo Dev vs. Release:** Local multi-repo development uses sibling `path` / `[patch]` entries in `Cargo.toml`; before pushing a release `v*` tag for `.github/workflows/release.yml`, convert sibling overrides to pinned git tags/versions and sync `Cargo.lock` + Tauri Rust/NPM minor versions.
- **Audit & Triage Invariants:** Read [`docs/security/DESIGN_INTENT_TRIAGE.md`](docs/security/DESIGN_INTENT_TRIAGE.md), [`prompts/AUDIT_LOOP.md`](prompts/AUDIT_LOOP.md), [`docs/UI_AUDIT_GUIDE.md`](docs/UI_AUDIT_GUIDE.md), and [`.agents/skills/i18n-workflow/SKILL.md`](.agents/skills/i18n-workflow/SKILL.md). Keep [`STATUS.md`](STATUS.md) updated on milestones.

## Verification Gates

- Run `./run-tests.sh --quiet` (`--frontend-only`/`-f` or `--backend-only`/`-b` when isolated) after code changes — covers Tauri version sync, TypeScript, ESLint, `npm run i18n:check`, Vitest (incl. glossary/uiMode/IPC parity tests), `cargo fmt`, `cargo clippy`, and `src-tauri` + `humoco-market` Rust tests.
- **Test & Diff Integrity:** For security/logic diffs, run `cargo mutants --in-diff <diff> --offline` on Rust crates (when installed) or verify a Revert-Red check across Rust/Vitest; assert exact error variants/states + positive controls (no bare `.is_err()`, `toBeDefined()`-only checks, mocked-away units under test, or hollowed existing tests).
