---
name: project-plan
description: 4-phase MVP project plan for the Human Money App, from core transaction to full-scale deployment.
---

# Human Money App — Project Plan

## Phase 1: Core Transaction (End-to-End)

**Goal:** Ensure the basic ability: Create voucher → Send → Receive.

1. **UI Foundation & Responsiveness** — Tailwind CSS layout, mobile-ready from start ✅
2. **Voucher Creation** — `CreateVoucher.tsx` + `create_new_voucher` command ✅
3. **Send & Receive** — `SendView.tsx` + `ReceiveView.tsx` with QR/file transfer ✅
4. **Stabilization & Polish** — Fix bugs and polish core workflows ✅ (APP-005)

## Phase 2: Usability & Management

**Goal:** Make the app actually usable.

1. **Enhanced Voucher Overview** — Dashboard with summaries + `VoucherDetailsView.tsx` ✅
2. **Contact Management (Address Book)** — Simple UI for contacts (Name + User-ID), stored via `save/load_encrypted_data` ✅ (APP-001, APP-015)
3. **Guarantor Flow** — Signing request UI + detached signature response ✅ (APP-007)
4. **Audit local_instance_id** — Ensure local-only metadata isn't leaked in JSON views or exports ✅ (APP-016)
5. **Session Timeout UX** — Graceful handling of expired sessions in read-views (e.g. Activity Log) 🔜 (APP-017)
6. **Internationalization (i18n)** — `i18next` integration (German/English) 🔜

## Phase 3: Extended Functionality (Post-MVP)

1. **Trust Integration** — Trust badges, WoT display (depends on WOT specs + CORE-001) ⬜
2. **Discovery** — BLE/NFC contact exchange ⬜

## Phase 4: Scaling & Polish (Post-MVP)

1. **Dynamic Voucher Standard UI** — Form fields generated from TOML definition ⬜
2. **Performance** — Optional SQLite, pagination, lazy loading ⬜
3. **L2 sync hardening** — Robust synchronization for production ⬜ (APP-004)

## Current Status

Phase 1 is complete. Phase 2 is mostly complete (i18n, local_instance_id audit, and session UX pending). See `STATUS.md` for current task tracking.
