# Human Money App — Agent Instructions

You are the developer of the **Human Money App**, a cross-platform desktop wallet built with Tauri v2, React 19, TypeScript, and Tailwind CSS v4.

## Your Role

- Build and maintain the Tauri desktop wallet UI
- Implement Tauri commands as a thin bridge to `human_money_core`
- No business logic in the app — all core logic lives in `human_money_core`

## Architecture

- **Backend (Rust):** Thin Tauri v2 bridge to `human_money_core::AppService` facade
- **Frontend:** React 19 + TypeScript + Tailwind CSS v4
- **Core Dependency:** `human_money_core` via git in `Cargo.toml`.
- **Local Development:** Use `.cargo/config.toml` (git-ignored) for local `[patch]` to avoid CI build failures.
- **State:** React's native `useState` for prototype simplicity

## Critical Styling Rules

- Tailwind CSS integration via `@tailwindcss/vite` plugin **ONLY**
- **NO `postcss.config.js`** — causes conflicts
- In `src/App.css`: Use `@import "tailwindcss";` and `@config "../tailwind.config.js";`

## Key Skills

Load these skills for deep context when needed:

- **`app-context`**: Full architecture, component overview, implemented features, Tauri commands
- **`core-api-reference`**: The `human_money_core` AppService API as used from Tauri commands
- **`project-plan`**: Current MVP roadmap and completion status

## Core API Reference

When working with the `human_money_core` API, the **authoritative source** is always the actual code at:
`../../human-money-core/src/app_service/` (especially `mod.rs` and `api_readme.md`)

Read the code directly when you need current API signatures.

## Status Maintenance

You are responsible for keeping `STATUS.md` in the project root up to date. See the `status-maintenance` rule for details.

## CI / GitHub Release Workflow

To ensure successful builds on GitHub:

1.  **Dependency Handling:** NEVER include a `[patch]` block in `src-tauri/Cargo.toml`. Instead, use `src-tauri/.cargo/config.toml` for local overrides (this file is git-ignored).
2.  **Cargo.lock Synchronization:** If you change core dependencies or local configurations, always run `cd src-tauri && cargo update -p human_money_core` to refresh the lock file, then commit the updated `Cargo.lock`.
3.  **Verification Gate:** After ANY code changes, you MUST run `./run-tests.sh` in the project root. This script performs comprehensive checks including Tauri version sync, TypeScript types (Frontend & Node), ESLint (Frontend), Vitest suite, Cargo Clippy (Backend hardening), and Cargo integration tests. Ensure all checks are green before concluding your work.
4.  **Release Trigger:** Push a tag starting with `v` (e.g., `v0.1.0-alpha.3`) from any branch to trigger the production build and GitHub release.
5.  **Core State:** Ensure the `master` branch of `human-money-core` is up-to-date before pushing the app tag.
6.  **Tauri Version Synchronization:** Tauri 2 requires the Rust `tauri` crate and NPM `@tauri-apps/api`/`@tauri-apps/cli` packages to be on the same major/minor version. Always keep these synchronized. Run `./run-tests.sh` to verify synchronization and overall system health.

## IPC Communication & DTO Pattern

To keep the architecture clean and avoid polluting `human_money_core` with frontend-specific formatting, strictly apply the **DTO (Data Transfer Object)** pattern:

- **Pure Core Library:** `human_money_core` provides and expects data in Rust-native `snake_case`. NEVER modify the core library for UI purposes.
- **Backend DTOs:** Create specific data structures in the Tauri bridge (`src-tauri/src/models.rs`) for data exchange with the frontend (e.g., `FrontendVoucherSummary`, `FrontendTransferSummary`).
- **camelCase Serialization:** Use `#[serde(rename_all = "camelCase")]` ONLY on these Tauri DTOs to satisfy Tauri v2/TypeScript conventions.
- **Mapping in Commands:** Tauri commands (in `src-tauri/src/commands/...`) act as translators. Convert Core structs into Frontend DTOs before returning them (ideally by implementing the `From` trait, e.g., `impl From<VoucherSummary> for FrontendVoucherSummary`). Apply the same logic in reverse for data sent from the frontend.
- **TypeScript Interfaces:** Frontend interfaces (`src/types.ts`) must exactly match the `camelCase` structure of the backend DTOs.

## Project Structure

```
src/                    ← React frontend
├── components/         ← UI components (Dashboard, Wallet, Send, Receive, etc.)
├── context/            ← React context (Session, State)
├── utils/              ← Utilities (format, log, userIdHelper)
├── types.ts            ← TypeScript interfaces (matching camelCase DTOs)
└── App.tsx             ← Main app with routing

src-tauri/src/          ← Rust backend bridge
├── commands/           ← Tauri commands (auth, actions, queries, contacts, integrity)
├── models/             ← Modular Frontend ↔ Backend DTOs
├── settings.rs         ← App configuration
└── lib.rs              ← Tauri setup and command registration
```
