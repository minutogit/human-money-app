---
project: human-money-app
version: "0.1.1"
phase: "beta"
health: "green"
last_updated: "2026-06-23"
blocks: []
blocked_by: []
priority_tasks:
  - id: "APP-031"
    title: "Concept Documentation System"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Integrated a two-tier onboarding and concept documentation system, presenting a swipe-screen Carousel/Short version and a detailed Accordion version. Fully accessible from the Login page (unauthenticated) and Sidebar menu (authenticated), with complete German/English translations."
  - id: "APP-030"
    title: "Global Context-Sensitive Help System"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Implemented a context-sensitive onboarding help system. Includes reusable HelpIcon and HelpModal components, localized non-technical guidance for Mnemonic, Passphrase, Sub-account, and Handover topics, and full unit test coverage."
  - id: "APP-029"
    title: "Bug Reporting (Fehler melden) MVP Feature"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Implemented a transparent and privacy-compliant bug reporting view in the Tauri app. Allows inspecting the local log file, copying it, and opening a new GitHub issue."
  - id: "APP-028"
    title: "Menschlich Miteinander Association Support Integration"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Integrated support view, navigation item, login page teaser, secure external URL handling via tauri-plugin-opener, and fully localized UI strings."
  - id: "APP-001"
    title: "Encrypted Address Book"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-001", "WOT-005"]
    description: "Address book component for managing known contacts with trust levels"
  - id: "APP-021"
    title: "Navigation & Session Context Refactoring"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Centralized application state and navigation logic into React Contexts. Eliminated prop-drilling across the core view hierarchy for improved maintainability."
  - id: "APP-002"
    title: "Trust level display for contacts"
    status: "open"
    priority: "low"
    depends_on: ["CORE-001", "WOT-005"]
    description: "Visual trust badges and verification status in contact views (Post-MVP)"
  - id: "APP-003"
    title: "Stranger discovery workflow (BLE/NFC)"
    status: "open"
    priority: "low"
    depends_on: ["CORE-002"]
    description: "Face-to-face contact exchange via BLE/NFC (Post-MVP)"
  - id: "APP-004"
    title: "L2 sync integration hardening"
    status: "open"
    priority: "low"
    depends_on: []
    description: "Harden L2 synchronization for production use (Post-MVP)"
  - id: "APP-005"
    title: "Stabilize existing features"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Fix remaining bugs and polish existing voucher workflow"
  - id: "APP-006"
    title: "Set up test infrastructure"
    status: "completed"
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
  - id: "APP-014"
    title: "WalletSeal (Rollback Guard)"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Multi-device fork protection with security lockdown, recovery tolerance zones (Soft/Critical warnings), and background seal synchronization."
  - id: "APP-015"
    title: "Wallet Cloning Protection"
    status: "completed"
    priority: "high"
    depends_on: ["APP-014"]
    description: "Anti-cloning system binding wallet state to a unique Host/Instance ID. Prevents accidental state forks and enforces device handovers."
  - id: "APP-016"
    title: "Audit local_instance_id exposure"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Investigate why local_instance_id is visible in JSON view and ensure it is not included in exported/sent vouchers."
  - id: "APP-017"
    title: "Improve Session Timeout UX"
    status: "open"
    priority: "high"
    depends_on: []
    description: "Handle session timeouts gracefully in the UI. Ensure components like Activity Log don't just 'disappear' but prompt for re-auth or show clear state."
  - id: "APP-018"
    title: "Display Voucher Description and Footnote"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Display the voucher description and footnote (if present) in the VoucherDetailsView to provide more context to the user."
  - id: "APP-019"
    title: "FreeTaler Standard Migration"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Replace legacy 'Silber' standard with 'FreeTaler'. Includes core standard definition, signature update, and app-level integration."
  - id: "APP-020"
    title: "IPC Stabilization & DTO Pattern"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Strict separation of Core (snake_case) and Frontend (camelCase) using dedicated DTOs in the Tauri bridge. Ensures architectural integrity and cryptographic stability."
  - id: "APP-022"
    title: "Voucher Divisibility Bug Fix"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Resolved architectural break where the frontend expected 'divisible' but the backend sent 'allow_partial_transfers'. Updated DTOs and interfaces to ensure consistency across the stack."
  - id: "APP-023"
    title: "Stealth Identity Resolution"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-001"]
    description: "Unmasking stealth transaction identities by decrypting privacy_guard in the core library. Integrated React hook to prioritize address book names over truncated DIDs in Dashboard and Activity Log."
  - id: "APP-024"
    title: "Map Coordinate Entry Helpers"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Added automatic coordinate extraction from map links, device-native GPS location detection, and Nominatim-based address geocoding to profile settings and voucher creation forms."
  - id: "CORE-003"
    title: "Double Spend Conflict Classification & Quarantine Fix"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Fixed critical bug where double-spend conflicts affecting the local wallet were incorrectly classified as Witness instead of Victim, and ensured affected local vouchers are correctly quarantined immediately upon detection or proof import."
  - id: "APP-025"
    title: "i18n Localization Foundation"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Established i18n file structure, basic react-i18next runtime initialization, and custom prep and merge node utility scripts for translation workflow validation."
  - id: "APP-026"
    title: "Structured Error DTOs for i18n Localization"
    status: "completed"
    priority: "high"
    depends_on: ["APP-025"]
    description: "Transition error handling from opaque strings to structured FrontendError DTOs allowing localized, interpolated error messages in React frontend."
  - id: "APP-027"
    title: "Import UX Optimization"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Streamlined the import/receive file flow by auto-processing selected files immediately and conditionally prompting for passwords only when required by encryption."
  - id: "APP-032"
    title: "Custom Standard Import & Management"
    status: "open"
    priority: "medium"
    depends_on: []
    description: "Implement support for importing, managing, and utilizing custom voucher standards (TOML files) within the application UI to eliminate manual configuration file edits."
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

- No WoT integration (depends on CORE-001 and WOT-005)
- Alpha stability — some UI edge cases unpolished

## Recent Milestones

- [x] Fixed app hang on wrong password entry in `ReceiveView`: updated early-return paths in the catch block to explicitly reset the processing state and simplified the finally block to always call `setIsProcessing(false)` without relying on stale closures. Added comprehensive Vitest unit tests verifying button responsiveness during single and multiple failed password entry attempts.
- [x] Hardened input robustness for DID keys and user identifiers across the entire application stack: implemented a whitespace-stripping `sanitize_user_id` helper in `human_money_core`, trimmed fields at the Tauri bridge commands (`transfers.rs`, `contacts.rs`, and `queries.rs`), and added synchronous input-cleaning handlers and tests in the React frontend (`SendView.tsx` and `ContactDialog.tsx`) to prevent copy-paste errors.
- [x] Systematically sanitized frontend caught errors: replaced all raw/implicit string interpolations of caught error objects (`[object Object]`) with the centralized `stringifyError` utility across the entire codebase to guarantee type-safe, human-readable logging and troubleshooting output.
- [x] Implemented environment isolation for development: added a tauri.dev.conf.json override file changing the app identifier to 'human.money.app.dev' and window title to 'Human Money App [DEV]' when running via start-dev.sh to prevent development database/profiles collision with real production user data.
- [x] Finalized MVP polish: updated window titles and branding to 'Human Money App', refactored authentication/recovery flows to use type-safe feedback states (removing brittle substring checks), localized recovery warning modals, and synchronized all localization keys across English, German, Spanish, French, and Italian locales
- [x] Fixed navigation race conditions and memory leaks by cleaning up pending timeouts on component unmount and introducing a global route guard for active sessions
- [x] Refined vocabulary (e.g. Trust Status, Trusted Account, Flagged Account, Sendeübersicht) and integrated interactive HelpIcons for voucher-related workflows (Mode, Creator, Signatures, and Signature Requests) with full German/English translations
- [x] Refined Create Transfer privacy interface by integrating global HelpIcon and updating Stealth Mode descriptions across all supported languages (EN, DE, ES, FR, IT)
- [x] Integrated two-tier Concept Documentation System (carousel slideshow and detailed accordion view, accessible pre- and post-login)
- [x] Cleaned up profile creation UI by removing the duplicate recreate profile link
- [x] Global context-sensitive onboarding help system (HelpIcon, HelpModal, translation keys, and view integrations)
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
- [x] Cryptographic Voucher Traceability & Anti-Spoofing (Privacy mode back-tracing)
- [x] User-Controlled Privacy Mode (Vertical Toggle: Frontend -> Tauri -> Core)
- [x] WalletSeal Integration (Fork Detection Lockdown, Recovery Tolerance Zones, Background Sync)
- [x] Storage Integrity Stabilization (Automatic seal updates for all storage writes)
- [x] Wallet Cloning Protection (Instance ID binding, device handover, legacy migration)
- [x] Automated Test Infrastructure (Vitest + Rust Integration Tests)
- [x] Unified Test Script (`run-tests.sh`) and CI Preparation
- [x] BFF Currency Separation & API Synchronization (Mandatory display names, test voucher filtering, and interface alignment)
- [x] UI Layout Standardization (Centralized PageLayout, unified header design, and consistent full-width layout across all views)
- [x] Human-Centered UI Refactoring (Reverted technical jargon to accessible language across all core components)
- [x] Fixed TypeScript errors in CreateVoucher (Unused imports and missing Lock icon)
- [x] UI Label Refactoring (Humanized Profile and Voucher Creation labels for consistency)
- [x] Unified Password Protection for Address Book (Standardized protectAction for contact saving/deletion)
- [x] Address book / contact list UI polish
- [x] Stabilize existing features and fix edge cases
- [x] Audit local_instance_id exposure in JSON view
- [x] Display Voucher Description and Footnote in details (MVP) (APP-018)
- [x] Migrated legacy 'Silber' standard to 'FreeTaler' (Core & App) (APP-019)
- [x] Stabilized core test suite after standard migration (127 tests passing)
- [x] Refined transactionality and state management for FreeTaler precision (4 decimals)
- [x] IPC Stabilization & DTO Pattern Implementation (Strict snake_case core vs camelCase frontend) (APP-020)
- [x] Fixed voucher selection counter in SendView (Synchronized UI state with selection Map)
- [x] Centralized Voucher Standard Parsing (Moved TOML parsing from Frontend to Core AppService)
- [x] Refactored signature hints and role extraction to use type-safe backend-driven DTOs
- [x] Modularized Frontend Service Layer (Decoupled UI from direct Tauri invoke calls via type-safe services)
- [x] Code Quality & Automated Hardening (Integrated ESLint, Clippy, and static analysis into `run-tests.sh` quality gate)
- [x] Navigation & Session Context Refactoring (Eliminated prop-drilling across core view hierarchy) (APP-021)
- [x] Voucher Divisibility Bug Fix (Resolved architectural break and updated DTO/Frontend interface alignment) (APP-022)
- [x] Stealth Identity Resolution (Unmasked anonymous senders via privacy_guard decryption and contact mapping) (APP-023)
- [x] Backend State & Command Handler Refactoring (Centralized session initialization in AppState, consolidated inline DTOs, and simplified read-only commands to Cache-Only)
- [x] DID Prefix / Sub-Account UX Improvement (Simplified profile setup using a clean Q&A format and checkbox toggle for multi-device setups)
- [x] Map Coordinate Entry Helpers (OSM Nominatim Geocoding, GPS location, and Map Link Parsing) (APP-024)
- [x] Tauri Command Integrity & Mismatch Fixes (Added static analysis test validating all frontend invoke calls against backend command registrations, and corrected profile/integrity command name mismatches)
- [x] UI Component Testing for Settings (Implemented React Testing Library component tests for ProfileSettings and SettingsView, validating form submittals, geolocation triggers, tab navigation, and clipboard interactions)
- [x] Fixed Double-Spend Conflict UI Sorting (Sorted conflicting transactions chronologically in both backend DTO mapping and frontend component to ensure the valid earliest transaction is correctly shown as the Winner)
- [x] Fixed Double-Spend Conflict Classification & Quarantine Logic (Resolved bug classifying local victims as witnesses, quarantined losing vouchers automatically on proof import, and verified with integration tests) (CORE-003)
- [x] Hardened Signature Management (Restricted signature removal to incomplete vouchers and prevented deletion of the creator/issuer signature)
- [x] Fixed Voucher Creation Validation Bug (Ensured missing field banner only displays active errors and clears immediately as fields are filled)
- [x] Improved Create Voucher Layout Aesthetics (Reorganized form into vertically stacked sections, unified input/select heights in the Basics form, and expanded the min-height of Service Offer/Needs textareas for better legibility)
- [x] Redesigned Voucher Mode Selection (Replaced checkbox with a segmented switch selector and dynamic contextual descriptions for test vs real vouchers)
- [x] i18n Localization Foundation (Dateistruktur, Prep/Merge-Scripts, react-i18next Setup) (APP-025)
- [x] Structured Domain-Prefixes for i18n (Domain linter, multi-line parsing, en.json auto-sorting, and keySeparator option)
- [x] Structured Error DTOs for i18n Localization (Transitioned error handling from opaque strings to FrontendError DTOs with variable interpolation support)
- [x] Structured AppFacadeError Handling (Refactored AppState and remaining commands to return structured AppFacadeError / FrontendError DTOs to improve i18n error propagation)
- [x] Migrated remaining UI component catch blocks to translateError (SettingsView, ProfileSettings, ContactDialog, SendView, ConflictDetailsView, TransactionHistoryView, SignRequestView, IntegrityReportModal, ExportSigningRequestModal, CreateVoucher, ConflictListView) and stabilized all affected unit tests.
- [x] Localized ProfileSettings component (Batch 4 i18n migration).
- [x] Localized VoucherDetailsView and associated subcomponents (Batch 5 i18n migration: CreatorSection, PolicySection, TimelineSection, ExportDialog, and SignatureRequestBanner).
- [x] Localized AddressBook and ContactDialog components (Batch 6 i18n migration).
- [x] Localized Transaction History and Activities components (Batch 7 i18n migration).
- [x] Localized Conflict Management views (Batch 8 i18n migration).
- [x] Localized Settings and Integrity Report Modal (Batch 9 i18n migration).
- [x] Dynamic Language Detection & Manual Language Selector (Persisted client-side via localStorage).
- [x] Tauri Command Argument Alignment & Static Analysis (Aligned frontend invoke payload keys with backend parameter names and added a static analysis regression test).
- [x] Fixed signature request parsing error in SignRequestView (extracted roles via allowedSignatureRoles instead of obsolete signatureRules and updated test coverage).
- [x] Localized SignRequestView component (Batch 10 i18n migration).
- [x] Fixed Voucher Signature Deserialization Error (Strict DTO pattern alignment for voucher signature requests and added static analysis gate for command parameter types).
- [x] Optimized Import UX (Auto-processing of selected/dropped files, trial decryption without password, and conditional file password prompt) (APP-027)
- [x] Adjusted default Tauri window height from 600 to 720 to prevent content overflow at startup
- [x] Localized voucher status/state labels and Test Voucher badge in VoucherCard and VoucherFilterBar UI components
- [x] Integrated support view for 'Menschlich Miteinander' association, sidebar navigation, login page teaser, and tauri-plugin-opener handling (APP-028)
- [x] Increased font size of advanced passphrase warning during profile creation to improve readability
- [x] Updated login page footer to remove the protocol version and display the device ID with a shield icon prefix at 9px size
- [x] Added minimum window dimensions (640x600) in tauri.conf.json to prevent layout breakdown on small screens
- [x] Implemented Bug Reporting (Fehler melden) feature with monospace log viewer, clipboard copy, and GitHub issues redirection
- [x] Improved profile deletion feedback on wrong password by displaying localized error messages directly inside the modals
- [x] Fixed UI overflow bug in transaction audit log, activities, and dashboard lists by adding min-w-0 to flex layouts to ensure correct text truncation
- [x] Localized signature hints and added dynamic Title Case formatting for custom/arbitrary field checks
- [x] Stabilized ConfirmationModal button layout using grid-cols-2 and fixed width classes to prevent layout shifts and Cancel button deformation during state changes
- [x] Unified onboarding start screen with side-by-side tiles and enforced device prefix recovery compliant with SAI principles
## Next Milestones

- [ ] Internationalization (i18n) - German/English (MVP)
- [ ] Improve Session Timeout UX (Activity Log visibility etc.) (MVP)
- [ ] L2 sync hardening (Post-MVP)
- [ ] Trust level badges for contacts (Post-MVP)
- [ ] BLE/NFC stranger discovery (Post-MVP)
- [ ] Custom Standard Import & Management (Post-MVP)
