---
project: human-money-app
version: "0.5.15"
phase: "beta"
health: "green"
last_updated: "2026-11-20"
blocks: []
blocked_by: []
priority_tasks:
  - id: "APP-073"
    title: "Architecture Audit Remediation (K1–K2, H1–H13, F1–F3), Strict 24-bit PoW Align & 554-Key i18n Cleanup"
    status: "completed"
    priority: "critical"
    depends_on: ["APP-072", "CORE-023", "SYNC-011"]
    description: "Completed systematic KISS + Zero-Compat remediation across all 5 architectural root causes (`K1`–`K2`, `H1`–`H13`, `F1`–`F3`) and hardened QS checks: 1) Identity, Delegation & Egress Security (`K1`, `H8`, `H9`, `H11`): required verified 5-field `HUMOCO-LOCK-DELEGATE-V1` + `admin_pub`/`lock_pub` binding before counting L2 static 404s or classifying `?info` revocations, decoupled signed delegation metadata from health/offender observations (`upsert_verified_lock_probe` + `password`-aware `load_server_offender_list` in `l2_anchor_service.rs`), and hardened `sync_client.rs` / `wot_service.rs` SSRF checks (IPv4-mapped IPv6, NAT64 `64:ff9b::/96`, CGNAT, DNS-pinning resolver); 2) Transactional Boundaries (`H2`, `H3`, `H4`, `H10`, `H13`): added Core logout/cache rollback in `state.rs::initialize_profile_session`, multi-standard exact-UUID `.sig` attachment without guessing, atomic PoST commit/baseline sync + deterministic `ack-<msg_id>`, visible Sync-Lock protection selector with explicit `N=0` opt-in & TOCTOU modal (`voucher.create.l2Unavailable*`) + `?info.ticket_pow_bits` (24-bit cap), and fail-closed `remove_public_sync_impl` retaining error feedback inside the open delete dialog; 3) Async Generation & Delta Merges (`K2`, `H1`, `H6`, `H7`, `H12`): added `modalAttemptGenRef` + `isSubmitting` cancellation guards in `SessionContext`/`AuthModal`, threaded `SessionToken` through async backend sync/PoST flows, treated `OutboxMessageClass::ClaimBearing` strictly with per-target flush checks, replaced whole-map `l2_probe_cache` overwrites with delta upserts, and added async `cardResolveGenRef`/`isMountedRef` guards in `ContactDialog` & `ExportSigningRequestModal`; 4) Unified Flows & Strict Adapters (`H5`, `F2`, `F3`): routed `ReceiveSuccessView` `pendingCommit` retries through shared `ToleranceConfirmationModal` (`forceAcceptToleranceBundle` + `purchaseId`), enforced delimiter-bounded `humoco://card?env=` + `#[serde(deny_unknown_fields)]`, and removed sibling root-DID card forwarding fallbacks; 5) QS & Zero-Compat Cleanup (`F1` + i18n): replaced `AppConfigDto.lifecycle` with trimmed `FrontendLifecycleStateDto`, removed unused `l2ProbeCache`/`deleteRemote`/`DeadQuorumSwap` remnants, fixed latent `error.auth.sessionExpired` -> `sessionTimeout` keys, purged 554 dead i18n keys (`2497` sorted keys across all 5 locales), and upgraded `scripts/i18n-check-keys.js` to hard-error on unused keys. Verified with `humoco-market` (`112/112` tests, Clippy `0` warnings), `src-tauri` (`208/208` tests incl. new `K1`/`H1`/`H2`/`H4`/`H6`–`H13`/`F2`/`F3` regression tests, Clippy `0` warnings), `npm run check-versions` (`2.11`), `npm run i18n:check` (`2497` keys), ESLint (`0` errors on modified/added TS/TSX files), Vitest (`95/95` files, `715/715` tests), and `npm run build` (`tsc && vite build`)."
  - id: "APP-072"
    title: "Zero-Config & Relocation-Safe Sync-Locks UX, Automatic Committee Assembly, N=1->2/3 Auto-Growth & Soft Guarantor Trust Flow"
    status: "completed"
    priority: "high"
    depends_on: ["PROTO-005", "CORE-022", "SYNC-010", "WOT-014"]
    description: "Replaced manual Sync-Lock probe/template/lifecycle/dead-quorum panels (`LockServerProbeSection`, `AnchorTemplateBuilder`, `L2LifecycleSection`, `DeadQuorumSwapPanel`) with automatic Sync-Lock protection across `humoco-market`, `src-tauri`, `src/`, and all 5 locales: 1) Aligned 5-field `HUMOCO-LOCK-DELEGATE-V1` and `"lock"` channel authorization/probing in `admin.rs`/`channel_reader.rs`; 2) Added `LockTicketAuth` (`Direct`/`Stealth`/`Pow` gated by `allow_pow_tickets`), `SecurityPolicy.lock_trust_by_admin_pub` (`get_lock_protection_overview`, `set_lock_server_trust`, `refresh_lock_servers`), and live endpoint resolution (`resolve_server_locator`, `upsert_verified_lock_probe`, and relocation notice following without key revocation); 3) Implemented automatic lock committee creation on voucher issuance with single-subset retry, offline cache reuse, `N=0` fallback, and safe creator-only auto-growth `N=1 -> 2/3` (`l2_lifecycle.rs` + `0s` timelock when `guarantor_mig_pubs` is empty); 4) Replaced blocking guarantor own-server errors with `untrustedServersCount` and 1-click trust-and-sign, plus calm offline/unreachable voucher status notices (`VoucherCard`, `VoucherDetailsView`, `VoucherBasicsForm`, `NetworkProtectionCard`). Verified with `humoco-market` (`112/112` tests, Clippy 0 warnings), `src-tauri` (`199/199` tests incl. 4 new regression tests, Clippy 0 warnings, `rustfmt --edition 2021 --check` on modified files), `npm run check-versions` (`2.11 / 0.5.14`), `npm run i18n:check` (`3042` keys across 5 locales), `eslint` (0 errors on modified TS files), `npm run build` (`tsc && vite build`), and Vitest (`94/94` files, `708/708` tests)."
  - id: "APP-UX-11"
    title: "Systematic End-to-End UX, Workflow & Wiring Audit & Re-Audit Fixes (Simple/Expert/Cashier Parity, .ask Offline L2 Anchors, PoS & Recovery Hardening)"
    status: "completed"
    priority: "high"
    depends_on: ["APP-071", "APP-UX-10"]
    description: "Completed full two-sided E2E UX, workflow, wiring, and post-fix re-audit remediation across `crates/humoco-market`, `src-tauri`, `src/`, and all 5 locales: 1) Auth, Startup & Recovery (`useLoginFlow.ts`/`LoginCredentialPanel.tsx` preserve password and guide PIN->Handover; `NavigationContext.tsx` prevents `checkProfile` clobbering startup `.hmcbackup`/deep-link views; `commands/auth.rs`/`useRestoreBackupFlow.ts` return created `folderName` so `QuickPinCard` works immediately; `SocialRecoveryFlow.tsx` fixes inverted `superseded_epoch` banner and unpaired reply hints; `GuardianSetupModal.tsx`/`IdentityRecoveryCard.tsx` clear plaintext answers on close/open, wire `initialQuestionIds`, and surface partial sync errors; `GuardianReleaseModal.tsx` stabilizes `SessionContext`-independent inspection state; `EmergencyKitModal.tsx` & `App.css` add `@media print`, subaccount/passphrase preview parity, and `SeedPhraseInput.tsx` multi-line `extractHumocoDeepLink` unwrapping); 2) Cashier (`receiveOnly`) Enforcement (`backup_service.rs`, `recovery_service.rs`, `BackupSettingsCard.tsx`, `GuardianReleaseModal.tsx`, `DangerZoneCard.tsx`, `useSyncServiceAdmin.ts`, `SettingsView.tsx` enforce `AccessMode::Full` + step-up confirmation); 3) Payments, PoS & Guarantors (`checkout.rs` populates `receive_payload` on `Quarantined` verdicts; `receiveImportService.ts` & `useVoucherDetails.ts` emit `CONFLICTS_UPDATED`; `useReceiveModals.ts` + `ReceiveSuccessView.tsx` handle `ToleranceZone`/`RecoveryZone` retries, `receiveOnly`, and `isGoodsReleaseAuthorized` without double approval; `CreateVoucher` + `ExportSigningRequestModal` + `SignRequestView` + `commands/vouchers.rs` embed/verify/discover `anchor_manifest_json` in offline `.ask` bundles, capture password for `dismissPendingPostItem`, unblock incompatible `templates[0]` retention in Simple mode, normalize comma decimals and add `step=\"any\"`; `DeadQuorumSwapPanel.tsx` fixes adjudication crash); 4) Contacts, Marketplace, Sync & Settings (`ContactBadge.tsx`, `MarketplaceView.tsx`, `ImportCardModal.tsx`, `MarketPersonView.tsx` use `isExactSameUserId`; `SettingsView.tsx`/`AdvancedTab.tsx`/`NetworkProtectionCard.tsx`/`LockServerProbeSection.tsx` separate `onSettingsSynced` reloads and add catch rollback; `ConnectedSyncsCard.tsx` wires 1-click `handleAdoptReplacementUrl`; `useAddressBook.ts`/`MarketplaceView.tsx`/`standardsService.ts` wire `WOT_UPDATED`/`MARKET_CARD_UPDATED`/`STANDARDS_UPDATED`; purged 49 dead locale keys incl. `settings.coldRefreshButton` and synced all 2701 keys cleanly across de/en/es/fr/it). Verified with `cargo fmt --check`, `cargo clippy --all-targets --offline -- -D warnings` (0 warnings on `humoco-market` & `src-tauri`), `105/105` `humoco-market` tests, `137/137` `src-tauri` tests, `npm run i18n:check`, `npm run lint` (0 errors), `651/651` Vitest tests across `90/90` files (incl. `glossaryLint.test.ts` & `uiModeLint.test.ts`), and `npm run build` (`tsc && vite build`)."
  - id: "APP-071"
    title: "Automated Lock Committee Rotation, Auto Guarantor Veto, Discovery Cascade, Guarantor `Tx_claim` & Dead-Quorum Swap UX"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-021", "SYNC-007", "WOT-013"]
    description: "Implemented full Sync-Lock lifecycle automation, guarantor protection enforcement, long-term discovery, and L1 fallback UI across backend (`src-tauri/src/l2_lifecycle.rs`, `sync_client.rs`, `sync_zip.rs`, `gateway_resolver.rs`, `checkout.rs`, `intent_reconciliation.rs`, `models/`, `commands/`, `lib.rs`, `voucher_standards/`) and frontend (`L2QuorumBadge.tsx`, `ReceiveSuccessView.tsx`, `VoucherDetailsView.tsx`, `NetworkProtectionCard.tsx`, `types/`, `services/`, all 5 locales): 1) Permanent `ServerOffenderList` (`WOT-013`) persisted from equivocations (`EquivocationProof`) and filtered in trusted lock checks, probe cache, and committee assembly; 2) 4-stage `discover_anchor_manifest` cascade with monotonic local/cache/CardStore/AddressBook/`KeySuccessorAttestation` resolution and automatic `409 epoch_frozen` checkout/intent upgrades + retries; 3) Background & manual health worker (`run_l2_lifecycle_tick`, 5-day unhealthy hysteresis + immediate rotation on signed `SyncRevocationNotice`/`410`, crash-resumable 3-phase `propose` -> `fetch_snapshot`/`import` -> `sign_commit`/`finalize` preserving guarantor seats and flagged `reducedGuarantorProtection`); 4) Online guarantor auto-veto (`MigrationVeto`) blocking unauthorized guarantor protection reductions during timelock; 5) Voucher-details UI + commands for partial/full guarantor `Tx_claim` bundle creation (`create_guarantor_claim_bundle`), `DeadQuorumSwap` emergency exchange, `reducedGuarantorProtection` amber badges, Settings lifecycle/guarantor role registration, and `sync_zip.rs` packaging of `lock_handler.php` + `lock_rotation.php` + `[lock]` config blocks in `humoco-sync-setup.zip` (8 files). Verified with `cargo fmt --all --check`, `cargo clippy --all-targets --offline -- -D warnings` (0 warnings), `124/124` `src-tauri` tests (incl. `sync_zip::tests`), `601/601` Vitest tests across 81 files, and `npm run build`."
  - id: "APP-070"
    title: "Sync-Locks Migration (`LockCommittee`), External Probe UI, Anchor Templates & Guarantor Check"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-020", "SYNC-006b"]
    description: "Replaced legacy human-money-node `/v1/*` and regional-mesh client/UI flows cleanly per Zero-Compat with Sync-Lock Committees: 1) Tauri backend (`settings_cmd.rs`, `sync_client.rs`, `checkout.rs`, `gateway_resolver.rs`, `intent_reconciliation.rs`, `vouchers.rs`, `state.rs`) added SSRF-hardened `probe_lock_server_cmd` (`?info` delegation, WAF `POST ?lock`, `?lock_selftest`, key-leak check), `set_own_nominated_lock_server`, `assemble_anchor_template` (Sybil-folding & PoW/signed genesis tickets), upfront `retention_covers(init.deletable_at)` validation, `evaluate_guarantor_anchor_check` (guarantor standards only), `resolve_trusted_lock_keys` (`LÜCKE-03` isolation), Green-Rule `checkout_bundle` (`OfflineOnly`/`Confirmed`/`Yellow`/`Quarantined`), slot-filtered Leap-Lock receipt recording, and static `.lock` reconciliation; 2) React UI (`NetworkProtectionCard.tsx`, `StatusChip.tsx`, `VoucherBasicsForm.tsx`, `CreateVoucher.tsx`, `SignRequestView.tsx`, `L2QuorumBadge.tsx`, `ReceiveView.tsx`, `ReceiveSuccessView.tsx`, `SendView.tsx`, `VoucherDetailsView.tsx`, `retentionQuarter.ts`, all 5 locales) added external probe & committee template builder, explicit `N=0` offline-only mode, guarantor blocking check, PoS traffic light, and Sync-Lock help copy. Verified with `rustfmt --check`, `cargo clippy --all-targets --offline -- -D warnings` (0 warnings), `118/118` `src-tauri` tests, `101/101` `humoco-market` tests, `npm run i18n:check`, `npm run lint` (0 errors), `tsc --noEmit`, `600/600` Vitest tests (81 files), and `npm run build`."
  - id: "APP-069"
    title: "Device-Peppered Quick PIN, Cashier Mode (`receiveOnly`), Master-Password Step-Up & 12-Question Spaced-Repetition Recovery Rehearsal"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-017", "APP-066"]
    description: "Implemented optional 6–8 digit Quick PIN & Cashier Mode (`receiveOnly`) plus hardened Social Recovery UX: 1) Device-bound `pin_keystore.rs` storing AES-GCM encrypted 32-byte `device_pepper` (`app_config_dir/pin_keystore.json`, `0o600` perms), BLAKE3 keyed secret derivation (`MAX_PIN_ATTEMPTS = 5`), automatic slot wipe on 5 failed attempts with master-password fallback, and clean cleanup on profile delete/recovery/password reset; 2) Tauri auth commands (`login_with_pin`, `get_profile_pin_status`, `get_session_access_mode`, `enable_profile_pin`, `disable_profile_pin`, `elevate_session_with_password`) and React UI (`QuickPinCard`, `Login` PIN mode & remaining-attempt counter, `Sidebar` Cashier/Daily badge with 1-click Step-Up, `SessionContext::requireFullAccess` Step-Up modal, `IdentityRecoveryCard`, `Dashboard`, `CreateVoucher`, and `SendView` Cashier guards); 3) Expanded Social Recovery question catalog from 6 to 12 in `humoco-market` and `src/constants/recoveryQuestions.ts` plus zero-plaintext Spaced-Repetition memory rehearsal (`compute_rehearsal_verifiers` / `verify_rehearsal_answers`, `[1d, 7d, 30d, 90d, 180d]` schedule, `RecoveryRehearsalModal`, Dashboard reminder banner & Snooze) across all 5 locales. Verified with 101 humoco-market tests, 117 src-tauri tests, cargo clippy (0 warnings), 591/591 Vitest tests (81 files, including `glossaryLint.test.ts` & `SecurityPinAndRehearsal.test.tsx`), `npm run i18n:check`, and `npm run build`."
  - id: "APP-UX-10"
    title: "Unified Profile Open, Restore & Subaccount UX (`SeedPhraseInput`, 3-Card `RestoreBackupView`, Smart Sync Probe & Local Collision Handling)"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-09", "APP-SYNC-02"]
    description: "Replaced fragmented restore and identity-recreation paths with a unified 'Open / Restore Existing Profile' onboarding architecture under strict Zero-Compat cleanup (deleted `RecreateProfile.tsx`, `SocialAndSheetRecoveryFlow.tsx`, `fetch_and_inspect_backup`, and `restore_from_emergency_kit_uri`). Built `src/components/ui/SeedPhraseInput.tsx` featuring 12/24 pill toggle, smart paste (auto-expands 12 to 24 words without truncation, recognizes `humoco://kit` URIs), 11-language auto-detection, and an `<AdvancedDisclosure>` with clear-text reveal (`Input showToggle`) for passphrase, subaccount prefix (`PrefixInfoModal`), and dictionary. Reduced `RestoreBackupView` to 3 equal cards (Trusted Contacts via `SocialRecoveryFlow`, Words/Emergency Sheet/Subaccount/No-Backup, File/Folder w/ logged-out `.hmcbackup` drag & drop), added backend `probe_sync_backup` (`found`/`not_found`/`unreachable` with `resolvedSyncTargets` adoption in `create_profile`), `preview_mnemonic_identity`, `stage_dropped_backup_bytes`, and `reset_local_password_via_backup`, implemented the Smart Local Collision Card (reset local password vs. replace local profile vs. create subaccount), updated `WalletRecovery`, `Login`, `CreateNewProfile`, `EmergencyKitModal` (12 & 24 words dynamically), `App`/`AppRouter`, `tour-catalog.mjs`, and all 5 locales. Verified with 575/575 Vitest tests (77 files), 112/112 src-tauri tests, 99/99 humoco-market tests, cargo clippy --all-targets (0 warnings), npm run i18n:check, npm run lint (0 errors), and npm run build."
  - id: "APP-SYNC-01"
    title: "Sync Reach: ETag / If-None-Match in sync_client::get_slot & Predictable Channel Cooldowns"
    status: "completed"
    priority: "high"
    depends_on: ["APP-068"]
    description: "Replaced Option<Vec<u8>> in sync_client::get_slot with SlotFetchResult (NotModified/NotFound/Modified), persisted slot_etags and channel_max_bytes_by_write_url in StoredSyncState, enforced fixed poll cooldowns (post 90s, card/wot 20m, force_poll/respect_cooldown overrides), and exposed honest last_sync_at telemetry. Plan: ../docs/plans/SYNC_REACH_PLAN.md."
  - id: "APP-SYNC-02"
    title: "Sync Reach: Deflate Compression (HMSLOT02), Base + Delta Card Bundles, First-Contact 3s Auto-Publish & Quiet-Friend Forwarding"
    status: "completed"
    priority: "high"
    depends_on: ["APP-SYNC-01"]
    description: "Added pure-Rust flate2/miniz_oxide Deflate compression (`HMSLOT02`/`HMSLOTM2`) with 8 MiB zip-bomb cap before bucket padding & AEAD; removed MAX_FORWARDED_CARDS=20 and replaced flat 23,602 B deduction with real recipient bucket overhead + 50/50 Hop-1/Hop-2+ interleave & geo/remote sorting; implemented Base + Delta (`delta` / `<pfx>-delta`) differential card bundles capped at base+delta <= channel_max_bytes with 7-day Base TTL refresh and safe compaction DELETE retry; enabled quiet-friend forwarding without own_card while guarding 0-reader card uploads + cleaning revoked slots when last reader is removed; enabled dynamic active-class WoT budgeting with 413 retry; triggered 3s-debounced card/wot re-seal on trust >= 1 contact onboarding; aligned `far` reach preset with `own_list_max_hops = 3`. Verified with 622/622 Vitest tests (77 files), 106/106 src-tauri tests, 99/99 humoco-market tests, cargo clippy (0 warnings), npm run i18n:check, and npm run build. Plan: ../docs/plans/SYNC_REACH_PLAN.md."
  - id: "APP-UX-09"
    title: "Simple & Expert Mode (`uiMode`): 10-Phase UX Simplification, Visibility Presets, Unified Flows, Help Center & Conflict Overhaul"
    status: "completed"
    priority: "high"
    depends_on: ["APP-068", "APP-UX-08"]
    description: "Completed all UI-track phases (P0–P4, P6–P10; P5 deferred to SYNC_REACH_PLAN.md) of `docs/SIMPLE_EXPERT_MODE_PLAN.md` across `v0.5.6` (plus WoT 3-hop cap in `humoco-web-of-trust` `e578c14`): P0 quick fixes (send amount input always visible, deduplicated sync probe, empty state CTAs, calm integrity banner, per-currency history summary, dynamic tour package version, WoT max-3-hop cap with default 2); P1 `uiMode` infrastructure (`AppSettings.ui_mode`, `UiModeContext`, `UiModeToggle`, `<Advanced>`, `<AdvancedDisclosure>`, `<TechId>`, `<PresetOrCustom>`, `PageLayout.advancedActions`, `NavItem.mode`, `uiModeLint.test.ts`); P2 jargon hiding & `<TechId>` rollout; P3 3-step Send flow & unified Receive QR-first flow; P4 3-tier visibility presets (`near`/`medium`/`far` via `apply_reach_preset`/`get_reach_preset`, `ReachPresetSelector`, `mediaAutoLoadMaxHops`); P6 single-page 6-block Settings with auto-save & unified sync toggle; P7 4+3 Sidebar, merged Dashboard+Wallet, Activity-absorbed History; P8 slim AddressBook & Marketplace presets/chips/1-click ad renewal; P9 streamlined CreateVoucher form with conditional gender prompt; P10 7-group Help Center (`HELP_GROUPS`, required `audience`, 51 topics incl. H1–H12 + `sendReceive` + 4 FAQs, 15 `.deepDive` extractions, removed `wotReach`/`wotSharing`), calm multi-device ForkLock copy, 3-question `ConflictDetailsView` & plain `ConflictListView` summary across all 5 locales (`glossaryLint.test.ts`, `i18n-lock.json`). Verified with 618/618 Vitest tests across 76 test files, 103 src-tauri tests, 96 humoco-market tests, cargo clippy --all-targets (0 warnings), npm run i18n:check, and 62/62 automated UI tour steps (Simple & Expert modes)."
  - id: "APP-068"
    title: "humoco-sync Ecosystem Split, Prefixed slot_id Fix, Rust config.toml Generator & Public Sync Onboarding UX"
    status: "completed"
    priority: "high"
    depends_on: ["APP-067"]
    description: "Moved PHP sync server files into standalone 6th repo `/humoco-sync` (`SYNC-001`–`SYNC-004` ✅; deleted `tools/humoco-sync/` cleanly per Zero-Compat); updated `sync_zip.rs` `include_bytes!` paths and added `generate_rust_sync_config_toml` + `get_rust_sync_config` Tauri command + Host tab `[Copy Rust config.toml]`; fixed prefixed `slot_id` (`<R>_<prefix>`) across `humoco-market` (`format_slot_id`, `parse_slot_id`, `sd_card.rs`) and `src-tauri` (`sync.rs`, `market.rs`, `post.rs`, `backup.rs`, `recovery.rs`); aligned `MockSyncServer` in `commands/sync.rs` with `X-Drop-Prefix`; sanitized `register_url` in `fetch_sync_info`; added `sign_delete_account_request` + `sync_client::delete_account` (`DELETE ?account` with per-channel + media fallback); updated `SyncServiceSettingsCard.tsx` with opt-in community preset, `[Unlock on website]` (`registerUrl?key=<R>`), key copy, `[Check activation]`, and delete confirmation dialog with remote account wipe checkbox across all 5 locales. Verified with 569/569 Vitest tests across 74 test files, 98 src-tauri tests, 96 humoco-market tests, cargo clippy (0 warnings), npm run i18n:check, and npm run build."
  - id: "APP-067"
    title: "Sync-Service Revocation & Graceful Migration (Signed revocation.json, 410 Gone, Grace Period, Auto Contact URL Migration)"
    status: "completed"
    priority: "high"
    depends_on: ["APP-064"]
    description: "Implemented decentralized sync-service revocation and graceful migration across humoco-sync (sync.php), Rust backend (sync_client.rs, state.rs, commands/sync.rs, settings.rs, models/market.rs), and React Settings/Contact UI (de/en/es/fr/it). Added Ed25519-signed HUMOCO-DROP-REVOKE-V1 notices via PUT sync.php?revoke (atomic revocation.json + HTTP 410 Gone past effective_at + self-redirect rejection + clear via effective_at: null), 24h client polling and live probe verification, anti-hijacking drop_pub pinning, automatic replacement sync migration in address books (with 2-slot cap handling), and warning banners with 1-click replacement prefill in SyncServiceSettingsCard."
  - id: "APP-066"
    title: "Standard vs. Expert Recovery UX: Simplified Onboarding, Two-Tier Protection Status, Guardian Selection, Printable Emergency Sheet & 3-Step Restore"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-015", "APP-065"]
    description: "Implemented Standard vs. Expert recovery UX: streamlined Standard-Mode profile creation with prominent fresh-install Restore vs. Create choice (`needs_profile`) compliant with `UX_GLOSSARY.md` (`glossaryLint.test.ts` — using `Profil`/`Sync-Dienst`, never `Wallet`/`Server`) and `retain_mnemonic = false` checkbox in Expert onboarding; enforced two-tier protection banner after the first voucher plus 2-sync-redundancy UX tip; added AddressBook guardian selection (`GuardianSetupModal`, explicitly advising different households) with `Pending`/`Confirmed` ack indicators; triggered immediate `sync_backup` upon guardian/sheet completion; added single guided 3-from-6 personal questions setup; implemented zero-dependency A4 printable Emergency Sheet (`EmergencyKitModal` with `window.print()` + `humoco://kit` SVG QR + 12 printed words); added shared recovery step in `WalletRecovery`, pre-login `pendingDeepLink` buffering in `App.tsx`, and restart-safe 2-button Simple-Mode `RestoreBackupView` (`SocialAndSheetRecoveryFlow` for Contacts vs. Emergency Sheet) with collapsible `Erweiterte Wiederherstellung` (`.hmcbackup`, 24w, BIP-39 passphrase, prefix). Verified with 566/566 Vitest tests across 74 test files, 94 src-tauri integration tests, 93 humoco-market tests, cargo clippy (0 warnings), npm run i18n:check, and 32/32 automated UI tour steps."
  - id: "APP-065"
    title: "Social Recovery Crypto & Sync Discovery Engine (GF(256) Shamir, Owner-Signed Share Commitments, High-Cost Argon2id & 1-Tap URI Reply)"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-015"]
    description: "Implemented pure-Rust GF(256) Shamir secret splitting in `humoco-market`, Ed25519 `owner_sig` + BLAKE3 `share_commitments` + strict KDF bounds (`m<=256MiB, t<=32, p<=4`) against share-poisoning and Argon2id OOM bombs, single canonical 3-question KDF (`question_ids: [u8; 3]` from 6-item catalog, NFC/umlaut-normalized high-cost Argon2id `m=64MiB, t=12, p=1`) for strict 2-factor recovery (`C = S XOR P`), X25519-sealed guardian vault distribution via `post` (`MAX_GUARDIAN_WARDS = 64`, 1 entry per `owner_did`) with explicit `GuardianVaultAck` confirmation and `backup_sync_descriptors` locator tracking, zero-server 1-tap `humoco://recovery-req` / `humoco://recovery-reply` handshake with 6-digit SAS verification and 48h staleness check plus `humoco://kit` URI handling, guardian rotation epochs (advancing `local.rotation_epoch` to `recovered_rotation_epoch` on post-restore), and parallel highest-`(epoch, tx_nonce)` backup mirror selection. Verified with 93 humoco-market unit and property tests, 94 src-tauri tests, 566 Vitest tests, clippy (0 warnings), and 32/32 UI tour steps."
  - id: "APP-UX-08"
    title: "Hardened UX Improvements, Marketplace Settings Tab, Slim AddressBook Header & Quick Contact QR Flow"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-07"]
    description: "Implemented all 6 phases of hardened UX improvements: version bump to 0.5.2; added marketSyncEnabled setting and decoupled market poll from sync drops; eliminated FrontendVoucherTemplateData and fixed dynamic voucher divisibility derivation; standardized ModalShell with integrated X button and default backdrop closing; disentangled settings with dedicated MarketplaceSettingsTab (auto-saving store hops and unknown creators toggle) and moved network reach to modal; relocated Wi-Fi only and shareMinLevel to ConnectionTab; added optional business card attachment in SendView; streamlined AddressBook header with slim 1-line status bar and 'Mein QR-Code'; implemented fast get_my_quick_contact Tauri command generating lightweight humoco://contact QR SVG; added QuickContactModal and non-blocking sync addition; expanded Help Center and HelpModal with collapsible deep-dive sections and L2 protection topic; updated all 5 locales and verified 497/497 Vitest tests, clippy (0 warnings), and i18n checks."
  - id: "APP-UX-07"
    title: "Phase 6 Feinschliff & Absicherung: 32-Step UI Tour, ui-audit Skill Overhaul, Automated Glossary Lint Test & Beginner Flow Verification"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-01", "APP-UX-02", "APP-UX-03", "APP-UX-04", "APP-UX-05a", "APP-UX-05b", "APP-UX-05c", "APP-UX-05d", "APP-UX-05e", "APP-UX-06"]
    description: "Completed Phase 6 polishing and validation: updated tour-catalog.mjs, ui-tour.mjs, and .agents/skills/ui-audit/SKILL.md to 32 canonical steps across 5 chapters (covering Help Center, Marketplace 3 tabs + ProfileCardEditor, Settings 4 tabs, and AddressBook + AddContactModal); removed temporary legacy tab aliases from tourBridge and SettingsView (Zero-Compat); implemented automated Vitest text lint suite (src/locales/__tests__/glossaryLint.test.ts) enforcing docs/UX_GLOSSARY.md forbidden terms and WoT guarantor prohibition; added beginner integration flows test suite (src/__tests__/beginnerFlows.test.tsx); verified 491/491 vitest tests, 73 src-tauri tests, 57 humoco-market tests, clippy (0 warnings), build, and tour:auto (32/32 passed)."
  - id: "APP-UX-06"
    title: "Phase 5 Settings Modularization: 4 Tabs Architecture, Zero-Fallback i18n, Backup Prominence & Zero-Jargon Clean Break"
    status: "completed"
    priority: "high"
    depends_on: ["APP-064"]
    description: "Modularized SettingsView into 4 dedicated tab components under `src/components/settings/` (ProfileTab, SecurityBackupTab, ConnectionTab, AdvancedTab), each < 400 lines; added top status banner and primary one-click action in BackupSettingsCard with 'Nach jeder Änderung' as recommended default; separated SyncServiceSettingsCard into consumer vs host modes; eliminated all inline fallback strings in `t(...)` calls across all locales; integrated deep-linking for all section aliases; updated Vitest suites (28/28 tests) and verified automated UI tour (28/28 passed)."
  - id: "APP-UX-05e"
    title: "Phase 4e Profil-Vereinheitlichung außerhalb des Marktplatzes: Single Source of Truth & Concept Docs"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-05d"]
    description: "Unified profile management to a single source of truth in Marketplace 'Meins'; removed redundant profile/needs editing from ProfileSettings and CreatorIdentityForm; wired business card share artifacts across AddressBook and Marketplace; updated concept documentation."
  - id: "APP-UX-05d"
    title: "Phase 4d Marktplatz Frontend: 3-Tab Architecture (BoardTab, PeopleTab, MineTab) & ProfileCardEditor"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-05c"]
    description: "Rebuilt Marketplace frontend with 3 intuitive tabs (BoardTab Pinnwand, PeopleTab Menschen, MineTab Meins), AdCard, AdEditorModal, RenewAdModal, and ProfileCardEditor featuring field-level selective disclosure (Everyone/Contacts/Nobody) and live preview; deleted obsolete listing components and i18n keys."
  - id: "APP-UX-05c"
    title: "Phase 4c Tauri-Backend Marktplatz: MARKET_OWN_PROFILE_KEY, SD-Card Signing & Clean Break Commands"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-05b"]
    description: "Implemented new encrypted storage MARKET_OWN_PROFILE_KEY, auto-renewing rebuild_and_sign_own_card, build_share_artifacts, and clean break commands (get_my_market_profile, save_my_market_profile, set_card_published, upsert_ad, renew_ad, delete_ad, get_market_view); removed legacy contact card commands."
  - id: "APP-UX-05b"
    title: "Phase 4b Crate humoco-market: Market-View, Pinnwand (build_ad_board) & 16-Hex Inquiries"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-05a"]
    description: "Implemented MarketPerson, build_market_view, build_ad_board with descending renewal sorting, MarketInquiry / MarketInquiryReply targeting Ad or Person, and distance-based card forwarding with honest recipient privacy stripping."
  - id: "APP-UX-05a"
    title: "Phase 4a Crate humoco-market: SD-Card Format, Skill/Ad Content Types & Validation"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-01"]
    description: "Clean break in humoco-market crate: eliminated legacy listings in favor of Skill and Ad content types, built SdCard format with 16-byte CSPRNG salts, 128-bit SHA-256 truncated digests, 32-digest decoy padding, 180-day max duration, and accept_envelope versioning."
  - id: "APP-UX-04"
    title: "Phase 3 Adressbuch 'Kontakte & Vertrauen': 4 Human Trust Levels, AddContactModal & Reach Selector"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-01", "APP-UX-02"]
    description: "Overhauled AddressBook to 'Kontakte & Vertrauen': replaced numeric levels with 4 human stages ('Als Mensch bestätigt', 'Bekannt', 'Freund', 'Enger Vertrauter'), integrated 4-tile AddContactModal, business card sharing modal, natural language reach selector, and clear empty states."
  - id: "APP-UX-03"
    title: "Phase 2 Dashboard & Navigation: StatusChip, Cause-Based Security Warnings & FirstStepsChecklist"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-01", "APP-UX-02"]
    description: "Consolidated 3 header badges into 1 unified StatusChip with detailed status popover, introduced specific cause-based security warning banners, FirstStepsChecklist empty state, top primary action bar, and streamlined sidebar navigation with collapsible 'Mehr' section."
  - id: "APP-UX-02"
    title: "Phase 1 Hilfe-System: Central HELP_TOPICS Registry, InfoHint Inline Components & HelpCenterView"
    status: "completed"
    priority: "high"
    depends_on: ["APP-UX-01"]
    description: "Established centralized HELP_TOPICS registry, InfoHint inline explanation component with 'Mehr erfahren →' links, HelpModal markdown-light renderer, and searchable HelpCenterView replacing legacy ConceptView."
  - id: "APP-UX-01"
    title: "Phase 0 UX-Glossar: Unified Human-Centric Terminology & Zero-Jargon Foundation"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Created docs/UX_GLOSSARY.md establishing mandatory terminology (Sync-Dienst, Visitenkarte, Fähigkeit, Inserat, Pinnwand, Vertrauensliste, Benutzer-ID, Profil / Meine Gutscheine, Konflikte); eliminated forbidden jargon across all UI keys."
  - id: "APP-064"
    title: "Sync Service & Address Book Overhaul: Per-Contact Sync Management, Health Probing, Rate-Limit Cooldown & Channel Member Auto-Push"
    status: "completed"
    priority: "high"
    depends_on: ["APP-063"]
    description: "Completed full Sync Service & Address Book overhaul: per-contact sync service management with URL validation, live probing, health status pills (connected, unauthorized, unreachable) in AddressBook and ContactDialog; auto-activation of friend sync targets on contact save; debounced channel member push across active syncs; global rate-limit cooldown manager with Retry-After header parsing; post sync event consolidation; unified 'Sync-Dienst' across all 5 locales (53 Vitest test suites / 431 tests, 70 src-tauri tests, 55 humoco-market tests passing with 0 warnings)."
  - id: "APP-063"
    title: "UI/UX Refactoring: Header Badges Z-Index & Opacity, Settings 4-Tabs Reorganization, Sync Service Terminology & Adaptive AddressBook"
    status: "completed"
    priority: "high"
    depends_on: ["APP-062"]
    description: "Enhanced app UX: raised Header z-index and made badge popovers 100% opaque (`bg-white dark:bg-gray-900`) preventing overlap behind sticky boxes; unified terminology from legacy 'Ablage' to 'Sync-Dienst'; enabled marketplace sync by default; harmonized backup settings with 'Backups erlauben' and configurable frequency; reorganized SettingsView into 4 intuitive tabs (Profile, Security & Backups, Sync & Network, Standards & General); made AddressBook top banner adaptive/collapsible with trust ratings wording and prominent sync service recommendations."
  - id: "APP-062"
    title: "Channel post Dual-Delivery Payments, SST-V4-Safe Retries, Receipts, Async Co-Signing, Async WoT Handshake & Inquiries"
    status: "completed"
    priority: "high"
    depends_on: ["APP-061", "CORE-007", "CORE-010"]
    description: "Implemented channel `post` workflows in src-tauri and UI: Dual-Delivery payments for online/offline receivers with SST-V4-safe retries (exact child lock preservation preventing self-inflicted double spending), non-blocking receipt delivery, async voucher co-signing (`VoucherForSigning` to `DetachedSignature`), async WoT handshake exchange, and 1-step marketplace inquiries with rate-limited slot uploads."
  - id: "APP-061"
    title: "humoco-market PostBoxEntry Crypto, tag16 Routing, 4-Tier Padding, Dummy Boxes & If-Match Slot Merging"
    status: "completed"
    priority: "high"
    depends_on: ["APP-058"]
    description: "Implemented channel `post` slot crypto in `humoco-market`: `PostBoxEntry` containers, 16-byte blind recipient tags (`tag16`), 4-tier CSPRNG bucket padding (4 KiB, 16 KiB, 64 KiB, 256 KiB), indistinguishable dummy boxes, multi-device `If-Match` slot merging (HTTP 412 handling), 1-step marketplace inquiry models, and 10 msg/min recipient rate limiter (10/10 post unit/proptests passing)."
  - id: "APP-060"
    title: "Channel backup Debounced Padded Upload, Drop Restore Wizard, Grace-Key Search & Mandatory L2 WalletSeal Spend Lock"
    status: "completed"
    priority: "high"
    depends_on: ["APP-058", "APP-041"]
    description: "Implemented channel `backup`: 30s-debounced 64-KB bucket-padded `.hmcbackup` upload to personal drop slots, drop restore wizard in React frontend supporting mnemonic recovery with 180-day grace-key candidate search, and mandatory L2 `WalletSeal` offline-spend lock check preventing double spending on restored wallets."
  - id: "APP-059"
    title: "Channel wot Multi-Class SlotBlob Sync & Segregated Trust-List Disclosures"
    status: "completed"
    priority: "high"
    depends_on: ["APP-058", "WOT-011"]
    description: "Implemented channel `wot` multi-class `SlotBlob` synchronization in `humoco-market` and `src-tauri`: segregated trust list packaging per distance class (`DropSyncSubpackage`), independent 1-to-1 Sent-Log isolation, and non-blocking background synchronization with configured friend drops."
  - id: "APP-058"
    title: "Clean-Break 64-Hex Ed25519 Drop Auth, 1-Inode Rate Limiter, Binary PUT + If-Match, Key Rotation & Drop UI"
    status: "completed"
    priority: "high"
    depends_on: ["APP-057"]
    description: "Implemented clean-break unified 64-hex Ed25519 authentication for stealth and public drops, 1-inode 64-KB fixed binary IP rate limiter (`ratelimit.bin.php`), raw binary PUT with atomic temp files and HTTP `If-Match` ETag concurrency, 180-day key rotation with verifiable transition certificates, `<DropStatusBadge />` in Header, and 3-section Drop settings UI in `SettingsView`."
  - id: "APP-057"
    title: "Ablagen Frontend, Address Book Trust Impact, Settings, i18n & E2E Market Integration"
    status: "completed"
    priority: "high"
    depends_on: ["APP-056"]
    description: "Implemented full Drop Management modal (DropManagementModal.tsx) with two tabs for drop sync/upload targets and own drop administration/setup wizard with ZIP export & HTTP connection verification; integrated non-blocking background drop syncing in MarketplaceView and MyOfferModal; added shareMinLevel and marketPollWifiOnly in SettingsView; added 'Hat eine Ablage' badge and inline trust change permission preview in AddressBook and ContactDialog; completed i18n in all 5 locales; 44 Vitest test suites (379 tests) passing."
  - id: "APP-056"
    title: "humoco-market Drop & Slot Crypto with Blind Multi-Recipient CSPRNG Padding & Deterministic Test Vectors"
    status: "completed"
    priority: "high"
    depends_on: ["APP-055"]
    description: "Implemented cryptographically blind Drop access derivation (derive_write_token, derive_access_id, derive_admin_token), member access policy calculation with WoT Level 1-3 direct + Hop-2 clamping and allow/deny overrides, CardBundle packaging, and sealed slot blobs with CSPRNG bucket padding (16/32/64/128/256), XChaCha20Poly1305 payload encryption, and per-recipient ChaCha20Poly1305 ephemeral keys without plaintext DIDs. Generated fixed deterministic test vectors (31/31 crate unit/proptests passing)."
  - id: "APP-055"
    title: "humoco-market Signed Contact Cards, Pure-Rust QR Generator & src-tauri Piggybacking Backend Integration"
    status: "completed"
    priority: "high"
    depends_on: ["APP-054", "CORE-014"]
    description: "Implemented pure-Rust ISO/IEC 18004 Byte-Mode ECC-L QR code SVG generator, ContactCard JWS EdDSA signing and verification with limits and prefix normalization, LRU CardStore with hop-bounded forwarding and offender checks, unified MarketView combining voucher and card listings with profile selection precedence, Tauri IPC commands (get_market_view, get_own_contact_card, save_own_contact_card, delete_own_contact_card, export_forward_card, import_contact_card), and non-blocking piggyback exchange in transfer and checkout flows (49/49 Rust backend tests + 360/360 Vitest tests passing)."
  - id: "APP-054"
    title: "humoco-drop Standalone Drop PHP Server, ZIP Packaging Tool & Blind Storage Backend"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Implemented standalone zero-dependency drop.php server with SHA-256 constant-time admin authorization, atomic slot writes and reads, quota enforcement, and automated zip builder tool."
  - id: "APP-053"
    title: "Proof-Carrying L2 Client Hardening (Parent-Bound 401 PoW, 1-RTT Offline-Buyer Checkout, Unoccupied Retry & Verified Anchor Sync)"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-012", "CORE-013", "NODE-001", "NODE-003"]
    description: "Fixed parent-bound stateless PoW directly from `401` challenge responses (`HUMOCO_V1_POW_STATELESS`, zero extra roundtrips), switched regional mesh view polling to public `/v1/regional-mesh-view`, upgraded PoS `checkout_bundle` to 1-RTT `POST /v1/checkout` supporting offline buyers and upstream conflict recording, enabled immediate exact-byte intent re-dispatch on verified 14/20 `Unoccupied` proofs, and wired signature-verified `WalletSeal` anchor sync (42 Rust tests + 328 Vitest tests passing)."
  - id: "APP-052"
    title: "Global WoT Reach Selector (1–4 Hops), Passwordless Multi-Contact .wot Sharing & Progressive-Disclosure Provider Seals"
    status: "completed"
    priority: "high"
    depends_on: ["APP-044", "APP-051", "CORE-011", "WOT-010"]
    description: "Replaced confusing per-contact hop buttons in `ContactDialog` with a global `1..4 Hops` network reach selector + `<HelpIcon topic=\"wotReach\" />` in `AddressBook` (`set_wot_max_hops`, automatic legacy contact normalization), added 1-click passwordless `.wot` sharing encrypted for up to 256 rated contacts (`ContainerConfig::TargetDids`), integrated `wot-provider-filter+v1` auto-detection (unencrypted & `SecureContainer`), inline provider rating in `ReceiveView`, progressive-disclosure `'Prüfsiegel & Verbände'` tab in `AddressBook` (`get_wot_providers`, `set_wot_provider_trust`, `remove_wot_provider`, opt-in HTTPS `refresh_wot_provider_url`), >1-year staleness indicators, and full EN/DE/ES/FR/IT translations (41 Rust WoT tests + 328 Vitest tests passing)."
  - id: "APP-051"
    title: "Unified Page Header, Navigation History & Shared UI Primitives Consolidation"
    status: "completed"
    priority: "high"
    depends_on: ["APP-050"]
    description: "Fixed header title column squishing ('Adressbuch' single-character wrap), button overlaps, double-scrollbars, and sticky header collisions by establishing a single scroll container in `App.tsx`, anti-squish flex-basis guarantees in `PageLayout` + reusable `BackButton`, a 20-entry `NavigationContext` history stack across all views, and shared `AlertBanner`, `SegmentedTabs`, `ModalShell`, and `PageState` primitives migrated across 16+ views with 41 new unit tests (325 total passing)."
  - id: "APP-050"
    title: "Guided Guarantor Workflow, Issuer Redemption Shortcut & Double-Spend Proof Export/Import"
    status: "completed"
    priority: "high"
    depends_on: ["APP-048"]
    description: "Added upfront guarantor requirement notices during voucher creation, concrete parity/signature progress pills and direct `.sig` import in `VoucherDetailsView`, address-book contact picker in signing request modals, inline gender setup & disclosure safeguards in `SignRequestView`, 1-click 'Redeem with Issuer' routing, and `.hmcproof` double-spend proof export/import (`export_double_spend_proof` / `import_double_spend_proof`)."
  - id: "APP-049"
    title: "Professional Account Statement, Unified Activity Navigation & UTF-8 BOM CSV Export"
    status: "completed"
    priority: "high"
    depends_on: ["APP-046"]
    description: "Consolidated sidebar history under a bank-grade 'Account Statement' view (with secondary access to the technical lifecycle event log), added full-text search across resolved address-book names/currencies/receipt IDs, direction/currency/date-range filters, period totals, 1-click 'Save as Contact' for unknown DIDs, and RFC-4180 UTF-8 BOM CSV export."
  - id: "APP-048"
    title: "Desktop Sharing Helpers (Contact Cards, Payment Requests, Compact Privacy & Global Drag-and-Drop)"
    status: "completed"
    priority: "high"
    depends_on: ["APP-045"]
    description: "Implemented `humoco://contact` and `humoco://pay` copy/paste builders and parsers, 'Copy My Contact Card' and direct `.wot` file import in `AddressBook`, a 'Request Payment' tab in `ReceiveView`, 1-click payment request paste & compact collapsible privacy summary in `SendView`, 'Show in File Manager' post-save helper, and app-wide `GlobalFileDropZone`."
  - id: "APP-047"
    title: "Silent Background Auto-Backup Sync, Mount-Safe USB Protection & Header Backup Status Badge"
    status: "completed"
    priority: "high"
    depends_on: ["APP-045"]
    description: "Added automatic debounced differential background backup sync (`run_auto_backup_sync` / `get_backup_status`) for unlocked `mnemonic_only` wallets with strict mount-point safety (`folderDisconnected` when USB/NAS is unplugged without creating empty directories), a 3-state `BackupStatusBadge` in `Header`, smart 14-day/unconfigured Dashboard reminders, and a 3-path setup guide in `BackupSettingsCard`."
  - id: "APP-046"
    title: "Switchable Balance Modes (Spendable vs. Net Balance), Breakdown Popover, Expiry Alerts & Wallet Search/Sort"
    status: "completed"
    priority: "high"
    depends_on: ["APP-045"]
    description: "Extended `FrontendAggregatedBalance` with exact fixed-point `i128` calculations (`receivedAmount`, `ownAvailableAmount`, `ownCirculatingAmount`, `netBalance`) without modifying Layer-1 Core. Added Dashboard 'Spendable' vs. 'Net Balance' switcher, hover/tap `(i)` breakdown popover, 30-day voucher expiry alert banner, full-text search/sorting/origin filters in `WalletView`, and interactive human-readable `<TrustBadge />` popovers."
  - id: "APP-045"
    title: "Onboarding Clarity, Just-in-Time Profile UX, Manual Session Lock & Send/Receive Polish"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Clarified Login/Restore/Password-Reset flows, made `userPrefix` optional with a dedicated `profileName` input in `RecreateProfile`, tucked expert passphrase/multi-device warnings into collapsible sections, shifted profile fields to non-blocking just-in-time guidance with auto-prefill on voucher creation, wired the `SendView` contact picker modal, added manual session lock in `Sidebar`, hid PoS cashier mode behind an advanced setting, and removed erroneous ledger terminology across all 5 locales."
  - id: "APP-044"
    title: "Full Offline-First Web of Trust Integration (Contacts, Voucher TrustBadges, .wot Exchange, Piggybacking & s_wot)"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-001", "CORE-010", "WOT-005"]
    description: "Integrated `humoco-wot` end-to-end across Tauri backend and React 19 UI (EN/DE/ES/FR/IT): encrypted `wot_register` synchronized atomically with `AddressBook` (canonical DID deduplication, zero auto-migration of legacy tags), human-semantic contact trust levels & reach selector in `ContactDialog`, `<TrustBadge />` (color + percentage + primary bridge contact + path view) across `VoucherCard`, `VoucherDetailsView`, `ReceiveSuccessView`, and `SignRequestView`, authenticated `.wot` file export/import (`TargetDid` & `Password`), automatic 2-phase delta piggybacking in Public & Stealth transfers for `level >= 1` contacts, recipient-perspective `s_wot` coin selection, and 1-click conflict trust revocation in `ConflictDetailsView`."
  - id: "APP-043"
    title: "Real WalletSeal Anchor Upload, L2 Voucher Reconciliation, PoS Trust Fallback & SST V4 Forensics"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Added backend and bridge commands for real WalletSeal anchor uploads (`flush_pending_anchor_sync`), live L2 voucher reconciliation (`reconcile_rollback`), dirty-checked L2 gateway cache refreshes (`refresh_l2_gateway_cache`), and voucher L2 status inspection (`check_voucher_l2_status`). Wired PoS checkout with receive payload recording (`ReceiveSuccessPayload`, history entry) and structured error codes for serverless/quorum failures."
  - id: "APP-042"
    title: "Quiet Serverless L2 Mode, 3-State Network Status Badge & Accessible Network Settings"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Implemented quiet mode for regional mesh renewal and intent reconciliation when no custom L2 servers or regional mesh sources are configured, avoiding spamming console or debug logs. Introduced 3-state network status badge (`NoServers`, `Offline`, `Online`) and improved accessibility in Network Settings."
  - id: "APP-041"
    title: "Encrypted Wallet Backups (.hmcbackup & Differential Sync Folder) and 2-Path Restore Wizard"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Integrated Core differential encrypted wallet backups (`.hmcbackup` archive and differential folder export) and implemented a 2-path restore wizard supporting clean full restore and differential merge."
  - id: "APP-040"
    title: "Host-Owned L2 Intent Reconciliation Supervisor"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Added `src-tauri/src/intent_reconciliation.rs`: a host-owned supervisor that periodically reconciles unresolved L2 send intents while the wallet is unlocked. It skips intents younger than 30 seconds, expires overdue intents, guards per-intent work with an in-flight set, best-effort observes retained originals, ensures an owner-bound regional view cache, begins and completes quorum reconciliation, and cleans up terminal intents. A 15-second worker is spawned during Tauri setup, and `submit_l2_send_intent` now also cleans up terminal intents in its terminal and expired early-return branches. Three unit tests cover locked wallets, young intents and expired intents."
  - id: "APP-039"
    title: "Fail-Closed Regional Mesh Cache Expiry & Renewal (F05/R05)"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Fixed Finding F05 in the Tauri host: regional mesh authority views now carry the earliest signed-view expiry, expired caches are rejected fail-closed, checkout proactively refreshes missing or near-expiry caches, settings invalidation triggers a best-effort refresh, and a host-owned 30-second worker renews the cache only while the wallet is unlocked. Added unit coverage for expired-cache rejection and impending-expiry detection."
  - id: "APP-038"
    title: "Z08 Productive PoS Checkout, Replay Idempotency & Z10 Retained-Choice Reconciliation"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Completed productive UI and backend integration: SettingsView regional mesh sources & cold refresh on login, ReceiveView PoS checkout mode & cart ID, ReceiveSuccessView idempotency warning on replay (suppressing goods release on replay), TransferSuccessView L2 transport observation disclaimer, robust non-swallowing intent collection, and complete submit_l2_send_intent terminal reconciliation flow."
  - id: "APP-037"
    title: "Z01 Fail-Closed Goods-Release Boundary: Unit Tests & Enforcement (transfers.rs)"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Documented and enforced the Z01 fail-closed boundary in `src-tauri/src/commands/transfers.rs`: `evaluate_checkout_release` (raw outcome strings) and `verify_live_checkout_and_release` (live quorum proofs) can no longer be reasoned about only via doc comments — the fail-closed logic is extracted into dedicated helpers (`map_untrusted_outcome_str`, `evaluate_untrusted_checkout_release`, `parse_live_checkout_ipc`, `verify_untrusted_live_checkout`, `checkout_release_gate`) that both commands delegate to. A caller-supplied \"Confirmed\" string maps down to `Found`, malformed `proof_json`/`expected_context_json` are rejected as `FrontendError` before verification, and even a mathematically valid 14/20 recipient-challenge-bound live proof is defensively clamped to `can_release_goods_automatically == false` (an observation only) until durable purchase state (Z08) is wired. New `z01_fail_closed_tests` module (4 tests) pins all of this, including sub-quorum/zero-challenge/context-mismatch/forged-signature rejections. Backend-only dev-dependencies (`blake3`, `ed25519-dalek`) were added to build real signed quorum proofs; no new packages in the dependency graph."
  - id: "APP-036"
    title: "L2 Connectivity Pre-Flight & Verified Checkout Audit Trail (M6.1 / M6.2)"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "M6.1: `create_transfer_bundle` now performs a pre-flight L2 connectivity check via `settings_cmd::resolve_active_l2_nodes_internal` with the cached settings. In exclusive custom server mode, zero reachable nodes aborts the send with the structured error `error.network.l2Offline` (new i18n key in EN/DE/ES/FR/IT) instead of silently degrading to an unconfirmable transfer; offline-first transfers remain untouched in all other modes. M6.2: `verify_live_checkout_and_release` persists a verified checkout record (resolved local voucher, amount/unit, payer, confirmed child lock, quorum vote count) in the transaction history via `state.append_to_history` whenever the live quorum proof authorizes the goods release. The persistence is fail-closed: a release is not handed out when it cannot be recorded."
  - id: "APP-035"
    title: "Prevent automatic goods release via caller-supplied outcome"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Fixed Finding F02 in the Tauri bridge (`evaluate_checkout_release`): an untrusted, caller-provided raw outcome string could be mapped to `L2ProcessingOutcome::Confirmed` and thereby authorize automatic goods release. The raw string \"Confirmed\" is now mapped to `Found`, and the returned verdict is defensively forced to `can_release_goods_automatically == false`. Only `verify_live_checkout_and_release` (live, recipient-challenge-bound quorum proof) may authorize automatic goods release."
  - id: "APP-034"
    title: "Prevent unverified L2 payment confirmation"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "UI correction implemented; full frontend verification pending. Removed fabricated 15/20 FINAL from received bundle display. Unverified presentation claims cannot produce confirmed badges or release advice. Current-proof verification, durable purchase binding and actual payment transport remain unimplemented; this UI correction does not complete A09 or establish global finality."
  - id: "APP-033"
    title: "human_money_core v0.3+ Breaking Changes & Archive Integration"
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Prepare host app for human_money_core v0.3+ breaking changes. Migrated read queries to with_wallet/with_wallet_and_identity, updated AppFacadeError handling, integrated FileVoucherArchive, upgraded TrapData to V3 SST, and ensured full frontend backward compatibility."
  - id: "APP-032"
    title: "Custom Standard Import & Management"
    status: "completed"
    priority: "medium"
    depends_on: []
    description: "Decoupled On-Demand Voucher Standard Management (.standard): Implemented container inspection, import preview with metadata, cleartext/password export, standard deletion with in-use safeguards, ReceiveView drag-and-drop integration, and a dedicated Standards Manager UI in Settings."
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
    title: "Trust level display for contacts & vouchers"
    status: "completed"
    priority: "high"
    depends_on: ["CORE-001", "WOT-005"]
    description: "Delivered in APP-044: Human-semantic contact trust ratings, network overview banner, and color/percentage/bridge-contact <TrustBadge /> across contact, voucher, sign-request, and receive views."
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
    status: "completed"
    priority: "high"
    depends_on: []
    description: "Separated `isAuthenticated` vs `isSessionActive` in auth state, ensuring unsaved form state and view context are preserved when session times out. Added clickable sidebar lock indicator for immediate manual locking and fixed the `sessionTimeoutSeconds = 0` auto-logout bug."
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
---

# Human Money App — Status

## Current Focus

Tauri v2 desktop/mobile wallet (`v0.5.1` Beta). Core voucher workflows, switchable Spendable/Net balance views, silent mount-safe auto-backups, `humoco://` contact & payment request sharing, global drag-and-drop, bank-grade Account Statement with CSV export, guided guarantor/redemption/proof workflows, unified navigation history & shared UI primitives (`APP-051`), global 1–4 hops reach selector, passwordless multi-contact `.wot` sharing & progressive-disclosure `BinaryFuse64` provider seals (`APP-052`), L2 PoS checkout, full Offline-First Web of Trust (`humoco-wot`), multi-purpose 4-channel sync infrastructure (`APP-054`–`APP-062`), and full Sync Service & Address Book overhaul (`APP-063`, `APP-064`: 4-tab settings, z-index/opacity fixes, per-contact sync URLs, live probing, health pills, rate-limit cooldown, and debounced member auto-push) are fully integrated across 5 locales.

## Architecture

- **Frontend**: React 19 + TypeScript + Tailwind CSS v4
- **Backend**: Rust (Tauri v2), thin bridge to `human_money_core` and `humoco_wot` via Tauri commands
- **Core & WoT dependencies**: `human_money_core` and `humoco-wot` via git in `Cargo.toml`. Local development uses git-ignored `src-tauri/.cargo/config.toml` for `[patch]` to ensure CI compatibility.
- **UI components**: Dashboard, CreateVoucher, SendView, ReceiveView, VoucherDetailsView, AddressBook, SignRequestView, ConflictDetailsView, TransactionHistory, Settings, Login, etc.
- **Tauri command modules**: auth, backups, contacts, profile, queries, settings, standards, transfers, vouchers, wallet, wot

## Known Issues

- Optional BLE/NFC face-to-face stranger discovery (`APP-003`) remains Post-MVP (standalone `.wot` files and transaction piggybacking cover offline/online MVP exchange).

## Recent Milestones

- [x] enforced & unit-tested the Z01 fail-closed goods-release boundary (APP-037): `evaluate_checkout_release` and `verify_live_checkout_and_release` cannot mint automatic goods release from untrusted IPC parameters alone (a caller `"Confirmed"` string maps down to `Found`; a valid 14/20 recipient-challenge-bound live proof verifies positively but stays an observation, `can_release_goods_automatically == false`, until Z08 durable purchase state is wired). Contract enforced by the new `z01_fail_closed_tests` module in `src-tauri/src/commands/transfers.rs`.
- [x] Implemented M6.1 (L2 connectivity pre-flight) and M6.2 (verified checkout audit trail): `create_transfer_bundle` is now an async command that checks the reachability of the active L2 nodes before a bundle is created and reports an informative offline error (`error.network.l2Offline`) when the app is pinned to custom servers and none of them is reachable. `verify_live_checkout_and_release` writes a transaction-history record for every goods release that was authorized by a live, recipient-challenge-bound quorum proof, so a release decision always leaves an auditable trail.
- [x] Fixed Finding F02 (automatic goods release via caller-supplied outcome): the Tauri command `evaluate_checkout_release` mapped the untrusted raw outcome string `"Confirmed"` to `L2ProcessingOutcome::Confirmed`, which `CheckoutReleaseVerdict::new` turns into `can_release_goods_automatically == true` — letting any frontend caller bypass live-proof verification and unlock automatic goods release. The raw string is now mapped to `Found`, the returned verdict is defensively forced to `can_release_goods_automatically == false`, and the invariant is documented on the command. Only `verify_live_checkout_and_release` may authorize automatic goods release.
- [x] Migrated to human_money_core v0.3.0 and bumped app version to 0.2.0: adapted AppFacadeError variants, converted read-only queries to with_wallet / with_wallet_and_identity, upgraded TrapData to V3 SST protocol (trap_r, trap_s), integrated concrete FileVoucherArchive, and maintained backward compatibility for voucher standard templates in frontend models.
- [x] Hardened seed/mnemonic input sanitization and cleaned up recovery views: created a centralized `cleanSeedText` utility (`src/utils/seedUtils.ts`) that sanitizes BIP-39 seed input (lowercasing, removing digits, punctuation like commas/dots/dashes/colons, tabs, newlines, and collapsing multiple spaces). Unified and refactored `CreateNewProfile.tsx`, `RecreateProfile.tsx`, and `WalletRecovery.tsx` to utilize this shared utility for both phrase (textarea) and words (individual inputs) modes, backed by a comprehensive unit test suite (`seedUtils.test.ts`).
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
- [x] Decoupled On-Demand Voucher Standard Management (.standard): Implemented container inspection, import preview with metadata, cleartext/password export, standard deletion with in-use safeguards, ReceiveView drag-and-drop integration, and a dedicated Standards Manager UI in Settings (APP-032).
## Next Milestones

- [x] Core v0.3+ Breaking Changes & Archive Migration (`FileVoucherArchive::new_secure`) (APP-033)
- [ ] Internationalization (i18n) - German/English (MVP)
- [x] Improve Session Timeout UX (Activity Log visibility etc.) (APP-017)
- [ ] L2 sync hardening (Post-MVP)
- [ ] Trust level badges for contacts (Post-MVP)
- [ ] BLE/NFC stranger discovery (Post-MVP)

### Core-Notes & Breaking Changes (human_money_core v0.3+)
- **Breaking Changes in Core ab v0.3:** Bei auftretenden Fehlern oder Core-Synchronisationen muss `human_money_core` analysiert werden.
- **Archive-Erstellung (Tauri Host):** Archive müssen künftig mit `FileVoucherArchive::new_secure(path, wallet_passwort)` gebaut werden.
- **Offene Punkte Core (report-only, ohne deterministischen Test):** Fehlendes `fsync` in `save_wallet` und TOCTOU-Race im PID-Locking.


### 2026-10-23 — Configured regional mesh verification

Registered authenticated recipient source-policy configuration and required bounded
signed regional views for live-checkout verification through Core. Missing views
fail closed. Preserved the Z01 goods-release gate: caller-supplied context and a
valid current quorum do not create a durable purchase authorization. Automatic
view refresh/UI and the durable purchase producer remain open.
Validation: `./run-tests.sh --backend-only --quiet` passed using locally resolved
Core/Proto siblings. No frontend or published Git dependency claim is made.

### 2026-10-23 — Exact-byte dispatch and backend purchase APIs

Registered durable intent submission, retained-choice observation and transactional
purchase/challenge commands. Gateway retries preserve signed bytes, isolate f2f
credentials by exact origin, disable redirects and share one 800 ms budget.
Login starts bounded public cold discovery; cached routing avoids ping probes in
the new intent send command. Public discovery conveys URLs, never committee
authority. Regional policy remains recipient configured and authenticated.
Validation: backend test script and TypeScript type check passed. Complete
productive UI orchestration, signed roster refresh, terminal resolution and full
App/Core/Node/QUIC-to-PoS acceptance remain open.

### 2026-10-24 — Fail-closed regional cache expiry and renewal (F05/R05)

Regional mesh view caches now expire at the earliest signed-source validity
boundary and reject expired entries before checkout use. Checkout and a
host-owned 30-second renewal worker refresh missing or near-expiry caches while
the wallet is unlocked; settings changes invalidate the old authority and
trigger a proactive refresh. Two unit tests cover expired-cache rejection and
impending-expiry detection.

Validation: `./run-tests.sh --backend-only --quiet` and
`cargo test checkout::tests -- --nocapture` passed.

### 2026-10-24 — Host-owned L2 intent reconciliation supervisor

Added a 15-second host supervisor that reconciles unresolved L2 send intents
while the wallet is unlocked. Overdue intents are expired, intents younger than
30 seconds are skipped, per-intent overlap is prevented by an in-flight set,
retained originals are observed best-effort, and live quorum reconciliation is
completed only after verifying that the wallet owner and cached regional views
still match. Terminal intents are cleaned up after resolution, and the manual
`submit_l2_send_intent` early-return paths now perform the same cleanup.

Validation: `cargo test` in `src-tauri` passed (23 tests, including the three
new intent reconciliation tests).

### 2026-10-30 — v0.3.0: Encrypted Backups, Serverless-Ready L2 UX & Session Stabilization

Bumped `human-money-app` to v0.3.0. Integrated Core encrypted backups (`.hmcbackup` / differential sync folder) and 2-path restore wizard (`APP-041`). Introduced serverless quiet mode for regional mesh renewal and intent reconciliation, 3-state network status badge, and network settings hardening (`APP-042`). Wired real WalletSeal anchor sync, network-backed rollback reconciliation, voucher L2 status queries, and durable PoS checkout recording (`APP-043`). Resolved session timeout state preservation and fixed immediate auto-lock when `sessionTimeoutSeconds = 0` (`APP-017`). All 31 Rust unit tests and TypeScript suites pass offline.

### 2026-10-31 — Full Offline-First Web of Trust Integration (`APP-002` / `APP-044`)

Integrated `humoco-wot` across the Tauri host and React 19 UI in all 5 locales (EN, DE, ES, FR, IT). Added encrypted `wot_register` state synchronized atomically with `AddressBook` (canonical DID matching, no auto-migration of free-text tags), human-semantic contact trust ratings and recommendation reach controls, `<TrustBadge />` (color + percentage + primary bridge contact + connection path) across voucher cards/details/receive/sign-request views, Ed25519-authenticated `.wot` trust file export/import (`TargetDid` & `Password`), automatic 2-phase delta piggybacking inside Public & Stealth `TransactionBundle`s for rated (`level >= 1`) contacts, `s_wot` Gravitational Coin Selection bonus, and 1-click trust revocation on verified double-spend conflicts.

### 2026-10-31 — v0.4.0: End-to-End Desktop UX Overhaul, Balance Modes, Auto-Backup & Account Statement (`APP-045`–`APP-050`)

Bumped `human-money-app` to `v0.4.0`. Delivered comprehensive user-centric improvements across all 5 locales without modifying Layer-1 Core:
- **Onboarding & Quick-Wins (`APP-045`)**: Disambiguated backup restore vs. password reset vs. empty seed derivation, fixed optional `userPrefix` + `profileName` in `RecreateProfile`, added non-blocking just-in-time profile guidance with auto-prefill on voucher creation, wired the `SendView` contact picker, added 1-click session locking in `Sidebar`, and tucked PoS cashier mode into advanced settings.
- **Balance Psychology & Wallet Tools (`APP-046`)**: Added fixed-point `i128` breakdown (`received`, `ownAvailable`, `ownCirculating`, `netBalance`), Dashboard switcher between *Spendable* and *Net Balance* with hover/tap `(i)` popovers, 30-day expiry warnings, full-text search/sorting/origin filters in `WalletView`, and interactive `<TrustBadge />` explanations.
- **Silent Auto-Backup (`APP-047`)**: Added debounced background differential sync for unlocked `mnemonic_only` wallets with USB/mount disconnection safety, a 3-state `BackupStatusBadge` in `Header`, and a 3-path setup guide.
- **Desktop Exchange (`APP-048`)**: Added copy/paste `humoco://contact` cards and `humoco://pay` payment requests, direct `.wot` import in `AddressBook`, collapsible `PrivacyToggle`, file-manager reveal helpers, and app-wide `GlobalFileDropZone`.
- **Account Statement (`APP-049`)**: Unified history navigation into a bank-style Account Statement with resolved contact search, direction/currency/period filters, quick contact saving, and UTF-8 BOM CSV export.
- **Guarantor, Redemption & Proof Workflows (`APP-050`)**: Added upfront guarantor notices, parity progress pills, direct `.sig` attachment, inline gender setup & disclosure safeguards in `SignRequestView`, 1-click issuer redemption routing, and `.hmcproof` double-spend proof export/import. All 280 Vitest tests and 38 Rust unit tests pass.

### 2026-11-01 — Navigation History, Shared UI Primitives & Global Reach / Provider Seals (`APP-051`, `APP-052`)

- **Page Header & Navigation History (`APP-051`)**: Unified single scroll container in `App.tsx`, anti-squish header flex guarantees in `PageLayout`, 20-entry backstack history navigation (`NavigationContext`), and shared `AlertBanner`, `SegmentedTabs`, `ModalShell`, and `PageState` primitives across 16+ views.
- **Global WoT Reach Selector & Provider Seals (`APP-052`)**: Replaced per-contact hop buttons with a global 1–4 hops reach selector (`set_wot_max_hops`), 1-click passwordless `.wot` export encrypted for up to 256 rated contacts (`CORE-011`), auto-detection and import of `BinaryFuse64` signed provider seals (`wot-provider-filter+v1`, `WOT-010`), progressive-disclosure 'Prüfsiegel & Verbände' tab in AddressBook (`get_wot_providers`, `set_wot_provider_trust`, `remove_wot_provider`, opt-in HTTPS refresh), inline provider rating in ReceiveView, staleness indicators (>1 year), and full 5-locale translations (41 Rust WoT tests + 328 Vitest tests passing).

### 2026-11-01 — Proof-Carrying L2 Client Hardening (`APP-053`)

- **Parent-Bound 401 PoW (`APP-053`)**: Fixed stateless PoW computation directly from `401 Unauthorized` challenge responses (`HUMOCO_V1_POW_STATELESS`, parent-bound, zero extra roundtrips).
- **Public Mesh View & 1-RTT Checkout**: Switched regional mesh view polling to unauthenticated 5s-cached `/v1/regional-mesh-view`, upgraded PoS `checkout_bundle` to 1-RTT `POST /v1/checkout` supporting offline buyers and upstream conflict recording via `MissingLocks.anchor_lock`.
- **Unoccupied Retries & Anchor Sync**: Enabled immediate exact-byte intent re-dispatch on verified 14/20 `Unoccupied` quorum proofs and wired signature-verified `WalletSeal` anchor sync (42 Rust unit/integration tests + 328 Vitest tests passing).

### 2026-11-08 — Market Waves 3A: Signed Contact Cards, Drop/Slot Crypto & Backend IPC (`APP-055`, `APP-056`)

- **`humoco-market` Crate (`APP-055`, `APP-056`)**:
  - Implemented standalone `#![forbid(unsafe_code)]` crate in `human-money-app/crates/humoco-market`.
  - Added pure-Rust ISO/IEC 18004 Byte-Mode QR Code SVG generator (`qr.rs`, ECC-L, Versions 1..=40, up to 2900 bytes).
  - Implemented canonical Ed25519 JWS card signing & verification (`card.rs`, `hmc-card+jwt`, base64url unpadded).
  - Implemented in-memory & encrypted LRU `CardStore` (`card_store.rs`, hop bounding, known offender check, eviction).
  - Implemented unified `MarketView` (`market_view.rs`) combining voucher listings and card listings, profile precedence, and trust tiers.
  - Implemented cryptographically blind Drop access derivation and access control (`drop_access.rs`, `MemberPolicy`, direct + Hop-2 clamping, allow/deny lists).
  - Implemented multi-recipient sealed slot blobs (`slot.rs`, `CardBundle`, CSPRNG bucket padding 16/32/64/128/256, XChaCha20Poly1305 + ChaCha20Poly1305 trial decryption, zero plaintext DIDs).
  - Generated deterministic test vectors (`tests/vectors/drop_access.json`).
  - All 31 unit tests and proptests passing with 0 clippy warnings.
- **Tauri Host IPC & Piggyback Integration (`APP-055`)**:
  - Registered Tauri commands: `get_market_view`, `get_own_contact_card`, `save_own_contact_card`, `delete_own_contact_card`, `export_forward_card`, `import_contact_card`.
  - Added settings: `max_store_hops` (default 2), `market_send_card_on_payment` (default false).
  - Wired transparent, non-blocking piggyback exchange in transfer creation and checkout receipt.
  - All 49 backend unit tests and 360 Vitest tests passing with 0 warnings.

### 2026-11-08 — Market Wave 5A: Ablagen Frontend, Drop Sync, Address Book Integration & Full Verification (`APP-057`)

- **Drop Management Modal (`APP-057`)**:
  - Built `DropManagementModal.tsx` with two clean tabs: "Ablagen nutzen" (sync actions, feedback display, discovered drops list with authorization badges, and selection of up to 3 upload targets) and "Eigene Ablage (Admin)" (guided setup wizard with policy configuration, ZIP package export via native save dialog + base64 fallback, HTTPS base URL validation, live connection testing, and full admin dashboard).
  - Added admin controls for channel `card`: member counters (direct, hop 2, total / max), editable card policy, allow/deny list management, active slot viewer, manual 32-char hex Access-ID slot blocking, blocked slot unblocking, and local admin credentials reset.
- **Marketplace & Offer Sync Integration**:
  - Added non-blocking drop sync on mount and manual pull-to-refresh in `MarketplaceView.tsx`, triggering reload when new cards are imported.
  - Added "Ablagen" header button in `MarketplaceView.tsx` opening `DropManagementModal`.
  - Integrated automatic non-blocking drop upload on card save and drop slot cleanup on card deletion in `MyOfferModal.tsx`.
- **Address Book & Settings Integration**:
  - Added `shareMinLevel` selector and `marketPollWifiOnly` toggle to Marketplace settings in `SettingsView.tsx` alongside quick link to Drop Management.
  - Added "Hat eine Ablage" (`hasDrop`) badge on contact cards in `AddressBook.tsx`.
  - Added real-time permission change preview in `ContactDialog.tsx` on trust level changes (`previewContactTrustChange`), clearly indicating gained/lost reading and writing permissions.
  - Wired automatic non-blocking drop members recalculation and push on contact save/delete in `contactService.ts`.
- **i18n & Full Verification**:
  - Added all Phase 3 keys across German, English, Spanish, French, and Italian and updated `i18n-lock.json`.
  - Added unit test suites for `DropManagementModal.test.tsx` (5 tests), `AddressBook.test.tsx` (6 tests), and `WotContacts.test.tsx` (13 tests).
  - All 44 Vitest test suites (379 tests), `tsc`, `eslint`, `i18n:check`, and full `./run-tests.sh -q` passing with 0 errors and 0 warnings.

### 2026-11-08 — Multi-Purpose Drop Infrastructure: Clean-Break Auth, Channels wot/backup/post, Dual-Delivery & SST-V4 Retries (`APP-058`–`APP-062`)

- **Clean Break & Server Hardening (`APP-058`)**:
  - Implemented unified 64-hex Ed25519 authentication for stealth and public drops (`X-Drop-Signature`, `X-Drop-Timestamp`, raw binary PUT + HTTP `If-Match` ETags) with zero bearer token leftovers.
  - Implemented 1-inode 64-KB fixed binary IP rate limiter (`ratelimit.bin.php`, 4096 entries, $O(1)$ under `flock`) and automatic 180-day key rotation with verifiable transition certificates.
  - Added Header `<DropStatusBadge />` and 3-section Drop settings in `SettingsView`.
- **Channel `wot` Distance-Class Synchronization (`APP-059`)**:
  - Integrated `WOT-011` multi-class `SlotBlob` synchronization in `humoco-market` and Tauri bridge, segregating trust lists across distance classes with independent Sent-Log tracking.
- **Channel `backup` Debounced Upload & Restore Wizard (`APP-060`)**:
  - Implemented 30-second debounced 64-KB bucket-padded `.hmcbackup` upload to personal drop slots.
  - Built drop restore wizard in `RestoreBackupView.tsx` with 180-day grace-key candidate search and mandatory L2 `WalletSeal` offline-spend lock check.
- **Channel `post` Crypto, Padding & If-Match Slot Merging (`APP-061`)**:
  - Implemented `humoco-market` `post.rs`: `PostBoxEntry` containers, 16-byte blind recipient tags (`tag16`), 4-tier CSPRNG bucket padding (4/16/64/256 KiB), indistinguishable dummy boxes, multi-device `If-Match` merge (HTTP 412), 1-step marketplace inquiry models, and 10 msg/min recipient rate limiter.
- **Channel `post` End-to-End Workflows & UI Integration (`APP-062`)**:
  - Implemented Dual-Delivery payments for online/offline receivers with strict SST-V4-safe retries (exact child lock preservation preventing self-inflicted double spending).
  - Implemented non-blocking payment receipts, async voucher co-signing (`VoucherForSigning` to `DetachedSignature`), async WoT handshake exchange, and 1-step marketplace inquiries.
- **Full Verification**:
  - All 47 Vitest test suites (399 tests) passing.
  - All 55 `humoco-market` unit/proptests passing.
  - All 61 `src-tauri` integration/unit tests passing (116 total Rust tests, 515 total app tests).
  - 0 ESLint warnings, 0 TypeScript errors, 0 Clippy warnings.

### 2026-11-10 — v0.5.1: Sync Service & Address Book Overhaul (`APP-063`, `APP-064`)

- **Settings 4-Tabs Reorganization & UX Enhancements (`APP-063`)**:
  - Raised Header z-index and made badge popovers 100% opaque (`bg-white dark:bg-gray-900`) preventing overlap behind sticky headers.
  - Reorganized SettingsView into 4 clean tabs: Profile, Security & Backups, Sync & Network, Standards & General with badge deep-link navigation.
  - Enabled marketplace sync channel by default and streamlined backup settings with 'Backups erlauben' and configurable frequency selector.
- **Sync Service & Address Book Integration (`APP-064`)**:
  - Implemented per-contact sync service management in `ContactDialog.tsx` (up to 2 sync URLs) with format validation, HTTPS probing via `probe_sync_write_url`, and reachability health indicators.
  - Added dynamic sync health status pills on contact cards (`connected`, `unauthorized`, `unreachable`) in `AddressBook.tsx` with popover showing last checked timestamp and URL.
  - Added interactive pull-to-refresh & sync trigger for sync services in AddressBook with global rate-limit cooldown protection (`useRateLimitCooldown`, `rateLimitManager.ts`).
  - Implemented debounced push channel members sync (`triggerDebouncedChannelMembersSync`) when saving/updating contacts so remote sync services update member lists automatically.
  - Wired real-time trust change impact preview showing gained/lost read/write permissions for Market, Sync, WoT, and Post channels.
  - Migrated legacy `dropEvents.ts` to unified `syncEvents.ts` and hardened `postSync.ts`.
  - Added HTTP `Retry-After` header parsing and exponential client backoff in `sync_client.rs`.
  - Complete i18n synchronization for all new keys across English, German, Spanish, French, and Italian.
- **Full Verification**:
  - All 53 Vitest test suites (431 tests) passing.
  - All 70 `src-tauri` integration/unit tests passing.
  - All 55 `humoco-market` unit/proptests passing (125 total Rust tests, 556 total app tests).
  - 0 TypeScript errors, 0 ESLint errors, 0 Clippy warnings (`-D warnings`).





