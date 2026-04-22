---
project: human-money-app
version: "0.1.0-alpha.6"
phase: "alpha"
health: "green"
last_updated: "2026-04-22"
blocks: []
blocked_by: []
priority_tasks:
  - id: "APP-001"
    title: "Encrypted Address Book"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-001", "WOT-005"]
    description: "Address book component for managing known contacts with trust levels"
  - id: "APP-002"
    title: "Trust level display for contacts"
    status: "open"
    priority: "high"
    depends_on: ["CORE-001", "WOT-005"]
    description: "Visual trust badges and verification status in contact views"
  - id: "APP-003"
    title: "Stranger discovery workflow (BLE/NFC)"
    status: "open"
    priority: "medium"
    depends_on: ["CORE-002"]
    description: "Face-to-face contact exchange via BLE/NFC"
  - id: "APP-004"
    title: "L2 sync integration hardening"
    status: "open"
    priority: "low"
    depends_on: []
    description: "Harden L2 synchronization for production use"
  - id: "APP-005"
    title: "Stabilize existing features"
    status: "open"
    priority: "high"
    depends_on: []
    description: "Fix remaining bugs and polish existing voucher workflow"
  - id: "APP-006"
    title: "Set up test infrastructure"
    status: "open"
    priority: "high"
    depends_on: []
    description: "Add frontend (Vitest/RTL) and backend (Rust) tests for existing features to ensure stability during future changes"
  - id: "APP-007"
    title: "Multi-Signature Workflows"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Implement GUI for requesting, creating, and attaching additional signatures. Supports flexible encryption (DID, Password, or Cleartext) for .ask/.sig bundles."
  - id: "APP-008"
    title: "Directory Memory for File Dialogs"
    status: "completed"
    priority: "medium"
    depends_on: ["APP-007"]
    description: "Remember the last used directory for all file open/save dialogs to improve workflow efficiency."
  - id: "APP-009"
    title: "User Profile Management"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Full user profile system with metadata encrypted in the wallet. Used for defaulting voucher creator and signature details."
  - id: "APP-010"
    title: "Signature Impact Evaluation Engine"
    status: "completed"
    priority: "medium"
    depends_on: ["APP-007", "APP-009"]
    description: "Real-time evaluation of signature impact against dynamic CEL standard rules with lexical hinting."
  - id: "APP-011"
    title: "Signature Management"
    status: "completed"
    priority: "medium"
    depends_on: ["APP-007"]
    description: "Allow creators to remove additional signatures from vouchers before they enter circulation."
  - id: "APP-012"
    title: "Decentralized Conflict & Reputation"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Multi-layered fraud protection: VIP Gossip for propagation, visual reputation checks (KnownOffender warnings), and local conflict overrides (forgiveness)."
  - id: "APP-013"
    title: "Multi-Language Mnemonic Support"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Support for multi-language seed phrases (BIP-39) including custom high-security German wordlist."
---

# Human Money App — Status

## Current Focus

Tauri v2 desktop wallet prototype. Core voucher workflows (create, send, receive) are functional. Needs stabilization and WoT-based contact management.

## Architecture

- **Frontend**: React 19 + TypeScript + Tailwind CSS v4
- **Backend**: Rust (Tauri v2), thin bridge to `human_money_core` via Tauri commands
- **Core dependency**: `human_money_core` via git in `Cargo.toml`. Local development uses git-ignored `src-tauri/.cargo/config.toml` for `[patch]` to ensure CI compatibility.
- **13 UI components**: Dashboard, CreateVoucher, SendView, ReceiveView, TransactionHistory, Settings, Login, etc.
- **5 Tauri command modules**: actions, auth, queries, utils

## Known Issues

- No automated tests (frontend or backend) — high risk during refactoring
- No WoT integration (depends on CORE-001 and WOT-005)
- Alpha stability — some UI edge cases unpolished

## Recent Milestones

- [x] Multi-platform release workflow (CI)
- [x] Bundle receive functionality
- [x] Transaction history with detail views
- [x] Settings page and in-memory history caching
- [x] Multi-profile support and profile selection
- [x] Flexible authentication and pessimistic locking
- [x] Wallet recovery workflow
- [x] User profile management (Encrypted metadata)
- [x] Encrypted Address Book (App-side persistence)
- [x] Signature Impact Evaluation Engine (Real-time hypothesis testing in UI)
- [x] Voucher Signature Management (Removal of uncirculated signatures)
- [x] Decentralized Conflict Management (VIP Gossip, Reputation, Local Notes, Tested Role Detection)
- [x] Conflict management integration tests (Victim vs. Witness roles)
- [x] Persistent Local Conflict Overrides with Resolution Notes
- [x] CI/Release workflow stabilization (git-ignored local patches)
- [x] Multi-Language Mnemonic Support (BIP-39, custom German wordlists, smart language detection)
- [x] Synchronized with human_money_core (Protocol version support in public profiles)
- [x] Dynamic Signature Hints & Lexical Hinting (missing profile data detection)
- [x] Dashboard UI Polish (Welcome empty state & To-Do area)

## Next Milestones

- [ ] Stabilize existing features and fix edge cases
- [ ] Set up test infrastructure (Vitest + React Testing Library, Rust integration tests)
- [ ] Address book / contact list UI
- [ ] Trust level badges for contacts
- [ ] BLE/NFC stranger discovery
- [ ] L2 sync hardening
