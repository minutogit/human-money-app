# Human Money App — Agent Instructions

You are the developer of the **Human Money App**, a cross-platform desktop wallet built with Tauri v2, React 19, TypeScript, and Tailwind CSS v4.

## Your Role

- Build and maintain the Tauri desktop wallet UI
- Implement Tauri commands as a thin bridge to `human_money_core`
- No business logic in the app — all core logic lives in `human_money_core`

## Architecture

- **Backend (Rust):** Thin Tauri v2 bridge to `human_money_core::AppService` facade
- **Frontend:** React 19 + TypeScript + Tailwind CSS v4
- **Core Dependency:** `human_money_core` via git with local `[patch]` in `src-tauri/Cargo.toml`
- **State:** React's native `useState` for prototype simplicity

## Critical Styling Rules

- Tailwind CSS integration via `@tailwindcss/vite` plugin **ONLY**
- **NO `postcss.config.js`** — causes conflicts
- In `src/App.css`: Use `@import "tailwindcss";` and `@config "../tailwind.config.js";`

## Key Skills

Load these skills for deep context when needed:

- **`app-context`**: Full architecture, component overview, implemented features, Tauri commands
- **`core-api-reference`**: The `human_money_core` AppService API as used from Tauri commands

## Core API Reference

When working with the `human_money_core` API, the **authoritative source** is always the actual code at:
`../../human-money-core/src/app_service/` (especially `mod.rs` and `api_readme.md`)

Read the code directly when you need current API signatures.

## Status Maintenance

You are responsible for keeping `STATUS.md` in the project root up to date. See the `status-maintenance` rule for details.

## Project Structure

```
src/                    ← React frontend
├── components/         ← 13 UI components
├── context/            ← React context
├── utils/              ← Logging utilities
├── types.ts            ← TypeScript interfaces
└── App.tsx             ← Main app with routing

src-tauri/src/          ← Rust backend bridge
├── commands/           ← Tauri commands (actions, auth, queries, utils)
├── models.rs           ← Frontend ↔ Backend data types
├── settings.rs         ← App configuration
└── lib.rs              ← Tauri setup and command registration
```
