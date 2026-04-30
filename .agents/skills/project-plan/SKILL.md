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

## Phase 2: Usability & Management

**Goal:** Make the app actually usable.

1. **Enhanced Voucher Overview** — Dashboard with summaries + `VoucherDetailsView.tsx` ✅
2. **Contact Management (Address Book)** — Simple UI for contacts (Name + User-ID), stored via `save/load_encrypted_data` ✅ (APP-001)
3. **Guarantor Flow** — Signing request UI + detached signature response ✅ (APP-007)
4. **Internationalization (i18n)** — `i18next` integration (German/English) 🔜

## Phase 3: Extended Functionality (Post-MVP)

1. **Trust Integration** — Trust badges, WoT display (depends on WOT specs + CORE-001) ⬜
2. **Discovery** — BLE/NFC contact exchange ⬜

## Phase 4: Scaling & Polish (Post-MVP)

1. **Dynamic Voucher Standard UI** — Form fields generated from TOML definition ⬜
2. **Performance** — Optional SQLite, pagination, lazy loading ⬜

## Current Status

Phases 1 is complete. Phase 2 is in progress (address book is the priority). See `STATUS.md` for current task tracking.
