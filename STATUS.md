---
project: human-money-app
version: "0.1.0-alpha.0"
phase: "alpha"
health: "yellow"
last_updated: "2026-04-05"
blocks: []
blocked_by: []
priority_tasks:
  - id: "APP-001"
    title: "Contact list / address book UI"
    status: "open"
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
---

# Human Money App — Status

## Current Focus

Tauri v2 desktop wallet prototype. Core voucher workflows (create, send, receive) are functional. Needs stabilization and WoT-based contact management.

## Architecture

- **Frontend**: React 19 + TypeScript + Tailwind CSS v4
- **Backend**: Rust (Tauri v2), thin bridge to `human_money_core` via Tauri commands
- **Core dependency**: `human_money_core` via git with local `[patch]` to `../../human-money-core`
- **13 UI components**: Dashboard, CreateVoucher, SendView, ReceiveView, TransactionHistory, Settings, Login, etc.
- **5 Tauri command modules**: actions, auth, queries, utils

## Known Issues

- No automated tests (frontend or backend) — high risk during refactoring
- No contact management / address book
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

## Next Milestones

- [ ] Stabilize existing features and fix edge cases
- [ ] Set up test infrastructure (Vitest + React Testing Library, Rust integration tests)
- [ ] Address book / contact list UI
- [ ] Trust level badges for contacts
- [ ] BLE/NFC stranger discovery
- [ ] L2 sync hardening
