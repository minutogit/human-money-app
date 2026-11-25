# Test Audit State

- **Repository:** `/home/basti/Coding/humoco/human-money-app`
- **Updated at HEAD:** `1ab2edd`
- **Total Test Files:** 165 | **Total Test Functions:** 1421

| Test File | Prio | Tests | Scan (C/H/M) | Status | Verified SHA | Proof / Notes |
|---|---|---:|---|---|---|---|
| `src-tauri/src/checkout/tests.rs` | P1 | 19 | 0/0/0 | 🟢 Verified | `a5f8e0c` | 19/19 pass; cargo-mutants precommit.rs collect_verified_precommit_conflicts 5/6 viable caught (2 unviable, 1 missed); Revert-Red trusted_keys.rs:176 S04-F8 confirmed |
| `crates/humoco-market/tests/recovery_tests.rs` | P1 | 23 | 0/0/0 | 🟢 Verified | `a5f8e0c` | 23/23 pass; cargo-mutants recovery.rs pass1 42/46 viable caught (1 unviable) + pass2 validate_kdf_params 25/25 caught after inclusive KDF bounds |
| `crates/humoco-market/tests/security_audit_s02_market_loop.rs` | P1 | 16 | 0/0/0 | 🟢 Verified | `a5f8e0c` | 16/16 pass; fixed wrong-gate VP8L chunk-size placebo; cargo-mutants media.rs 19/28 caught, sd_card.rs 19/27 caught |
| `crates/humoco-market/tests/slot_tests.rs` | P1 | 18 | 0/0/0 | 🟢 Verified | `cd3d0dc` | 18/18 pass; cargo-mutants slot.rs verify_card_bundle/target_recipient_bucket/unpad_backup_slot_bytes 27/28 viable caught (1 unviable, 1 missed) |
| `src-tauri/src/services/market_service/tests.rs` | P1 | 30 | 0/0/0 | 🟢 Verified | `cd3d0dc` | 30/30 pass (nextest 351/351); fixed h5 wrong-gate JWS signature tamper placebo; cargo-mutants ads.rs compute_event_expiry/validate_ad_input 22/35 caught |
| `crates/humoco-market/tests/post_tests.rs` | P1 | 12 | 0/0/0 | 🟢 Verified | `cd3d0dc` | 12/12 pass; cargo-mutants post.rs 81/140 viable caught (1 unviable, 59 missed) |
| `crates/humoco-market/tests/sync_access_tests.rs` | P1 | 11 | 0/0/0 | 🟢 Verified | `acf9cb3` | 11/11 pass; removed silent golden-vector rewrite & hardened 16 .is_err() to MarketError::Validation; cargo-mutants sync_access.rs 26/29 caught |
| `crates/humoco-market/tests/card_tests.rs` | P1 | 24 | 0/0/0 | 🟢 Verified | `acf9cb3` | 24/24 pass; hardened 11 .is_err(), AcceptOutcome wildcards & geo-eviction outcomes; cargo-mutants sd_card.rs validate_sd_card/verify_sd_card 51/66 viable caught (2 unviable) |
| `crates/humoco-market/tests/audit_wave1_regressions.rs` | P1 | 4 | 0/0/0 | 🟢 Verified | `acf9cb3` | 4/4 pass; hardened 10 .is_err() & 5 wildcards to exact MarketError/AcceptOutcome variants; cargo-mutants media.rs open_media_blob 5/10 caught & recovery.rs upsert_ward 11/16 caught |
| `src-tauri/src/services/recovery_service/tests.rs` | P1 | 17 | 0/0/0 | 🟢 Verified | `acf9cb3` | 17/17 pass (352/352 lib); RAII TempDirGuard, split 3-of-5 mega-test, removed let _ discards & hardened 10 .is_err() / 4 .is_some() / s07_f19; cargo-mutants complete.rs 8/11 & session.rs 18/26 viable caught |
| `src-tauri/src/l2_lifecycle/tests.rs` | P1 | 26 | 0/0/0 | 🟢 Verified | `171f0d0` | `cargo test --lib (359/359 pass, 26/26 l2_lifecycle::tests); cargo mutants rotation_policy.rs: 21/26 CAUGHT` |
| `crates/humoco-market/tests/security_audit_s01_market_loop.rs` | P1 | 12 | 0/0/0 | 🟢 Verified | `171f0d0` | `cargo test (148/148 humoco-market pass, 12/12 s01_market_loop); cargo mutants recovery.rs: 12/30 CAUGHT` |
| `src-tauri/src/commands/auth.rs` | P1 | 14 | 0/0/0 | 🟢 Verified | `171f0d0` | `cargo test --lib (359/359 pass, 14/14 commands::auth::tests); cargo mutants commands/auth.rs: 2/7 CAUGHT` |
| `src-tauri/src/models/market.rs` | P1 | 1 | 0/0/0 | 🟢 Verified | `171f0d0` | `cargo test --lib (359/359 pass, 1/1 s11_tests); Revert-Red on FrontendSaveMyMarketProfileRequest deny_unknown_fields` |
| `crates/humoco-market/tests/media_tests.rs` | P1 | 14 | 0/0/0 | 🟢 Verified | `efd5939` | `cargo test (150/150 humoco-market, 14/14 media_tests); fixed PNG short-len & avatar prefix placebos; cargo mutants media.rs: 52/52 CAUGHT` |
| `src/components/auth/__tests__/CreateNewProfile.test.tsx` | P1 | 19 | 0/0/0 | 🟢 Verified | `9c2354c` | `vitest 19/19 (98 files / 752 tests full pass); deferred seed promise replaces setTimeout sleep; Revert-Red CreateNewProfile.tsx password.length < 8` |
| `crates/humoco-market/tests/market_view_tests.rs` | P1 | 16 | 0/0/0 | 🟢 Verified | `efd5939` | `cargo test (150/150 humoco-market, 16/16 market_view_tests); cargo mutants market_view.rs: 14/14 viable CAUGHT (2 unviable)` |
| `src-tauri/src/checkout/mod.rs` | P1 | 0 | 0/0/0 | 🟢 Verified | `c4bc457` | `0 inline tests; #[cfg(test)] mod tests covered by verified sibling tests.rs` |
| `src-tauri/src/commands/integrity.rs` | P1 | 4 | 0/0/0 | 🟢 Verified | `efd5939` | `cargo test --lib (364/364 pass, 4/4 commands::integrity::tests); cargo mutants commands/integrity.rs: 3/3 viable CAUGHT (2 unviable)` |
| `src-tauri/src/commands/l2.rs` | P1 | 6 | 0/0/0 | 🟢 Verified | `efd5939` | `cargo test --lib (364/364 pass, 6/6 commands::l2::tests); cargo mutants commands/l2.rs: 4/4 viable CAUGHT (1 unviable)` |
| `src-tauri/src/l2_lifecycle/mod.rs` | P1 | 0 | 0/0/0 | 🟢 Verified | `c4bc457` | `0 inline tests; #[cfg(test)] mod tests covered by verified sibling tests.rs` |
| `src-tauri/src/l2_lifecycle/rotation_runner.rs` | P1 | 0 | 0/0/0 | 🟢 Verified | `c4bc457` | `0 inline tests; #[cfg(test)] test helpers only, covered by verified sibling l2_lifecycle/tests.rs` |
| `src-tauri/src/models/conflicts.rs` | P1 | 2 | 0/0/0 | 🟢 Verified | `efd5939` | `cargo test --lib (364/364 pass, 2/2 models::conflicts::tests); fixed missing positive attribution baseline; cargo mutants models/conflicts.rs: 2/2 viable CAUGHT (4 unviable)` |
| `src-tauri/src/services/backup_service/mod.rs` | P1 | 0 | 0/0/0 | 🟢 Verified | `c4bc457` | `0 inline tests; #[cfg(test)] mod tests covered by verified sibling tests.rs` |
| `src-tauri/src/services/backup_service/tests.rs` | P1 | 26 | 0/0/0 | 🟢 Verified | `dbf62cf` | 0/0/0 C/H/M; 26/26 pass; 5/5 viable mutants caught + Revert-Red USB is_dir() |
| `src-tauri/src/services/l2_anchor_service/mod.rs` | P1 | 0 | 0/0/0 | 🟢 Verified | `c4bc457` | `0 inline tests; #[cfg(test)] mod tests covered by verified sibling tests.rs` |
| `src-tauri/src/services/l2_anchor_service/tests.rs` | P1 | 11 | 0/0/0 | 🟢 Verified | `dbf62cf` | 0/0/0 C/H/M; 11/11 pass; 13/13 mutants caught (overview 5/5, sources 8/8) |
| `src-tauri/src/services/market_service/mod.rs` | P1 | 0 | 0/0/0 | 🟢 Verified | `c4bc457` | `0 inline tests; #[cfg(test)] mod tests covered by verified sibling tests.rs` |
| `src-tauri/src/services/market_service/view_and_import.rs` | P1 | 0 | 0/0/0 | 🟢 Verified | `c4bc457` | `0 inline tests; #[cfg(test)] test helper only, covered by verified sibling market_service/tests.rs` |
| `src-tauri/src/services/recovery_service/mod.rs` | P1 | 0 | 0/0/0 | 🟢 Verified | `c4bc457` | `0 inline tests; #[cfg(test)] mod tests covered by verified sibling tests.rs` |
| `src-tauri/src/services/wot_service.rs` | P1 | 14 | 0/0/0 | 🟢 Verified | `dbf62cf` | 0/0/0 C/H/M; 14/14 pass; 22/23 mutants caught (+1 equiv) + Revert-Red deny_unknown_fields |
| `src/components/auth/__tests__/EmergencyKitModal.test.tsx` | P1 | 8 | 0/0/0 | 🟢 Verified | `c29e83a` | `vitest 8/8 (98 files / 779 tests full pass); deterministic word idx, baseElement portal SVG check & null/error guards; Revert-Red EmergencyKitModal.tsx not-retained guard` |
| `src/components/auth/__tests__/ForkLockOverlay.test.tsx` | P1 | 9 | 0/0/0 | 🟢 Verified | `9c2354c` | `vitest 9/9 (752/752 pass); Revert-Red ForkLockOverlay.tsx needs_profile navigation on last profile delete` |
| `src/components/auth/__tests__/GuardianReleaseModal.test.tsx` | P1 | 6 | 0/0/0 | 🟢 Verified | `c29e83a` | `vitest 6/6 (98 files / 779 tests full pass); approve IPC, error paths & Fail-First RED->GREEN QR-SVG sanitization fix (GuardianReleaseModal.tsx QrCodeDisplay)` |
| `src/components/auth/__tests__/GuardianSetupModal.test.tsx` | P1 | 8 | 0/0/0 | 🟢 Verified | `c29e83a` | `vitest 8/8 (98 files / 779 tests full pass); trust-level filter, answer bounds & zero-server warning; Revert-Red GuardianSetupModal.tsx areAnswersValid` |
| `src/components/auth/__tests__/Login.test.tsx` | P1 | 10 | 0/0/0 | 🟢 Verified | `9c2354c` | `vitest 10/10 (752/752 pass); Revert-Red useLoginFlow.ts pinWipedMaxAttempts fallback & wrong-password clear` |
| `src/components/auth/__tests__/RestoreBackupView.test.tsx` | P1 | 12 | 0/0/0 | 🟢 Verified | `c29e83a` | `vitest 12/12 (98 files / 779 tests full pass); collision replace, retry probe & pw/inspect validation; Revert-Red useRestoreBackupFlow.ts replaceFolderName` |
| `src/components/auth/__tests__/SecurityPinAndRehearsal.test.tsx` | P1 | 6 | 0/0/0 | 🟢 Verified | `9c2354c` | `vitest 6/6 (752/752 pass); Revert-Red RecoveryRehearsalModal.tsx protectAction retry & QuickPinCard.tsx pin mismatch` |
| `src/components/auth/__tests__/SeedPhraseInput.test.tsx` | P1 | 8 | 0/0/0 | 🟢 Verified | `c29e83a` | `vitest 8/8 (98 files / 779 tests full pass); 11-lang invalid path, kit error & allowKitUri=false guard; Revert-Red SeedPhraseInput.tsx allowKitUri check` |
| `src/components/auth/__tests__/SocialRecoveryFlow.test.tsx` | P1 | 11 | 0/0/0 | 🟢 Verified | `c29e83a` | `vitest 11/11 (98 files / 779 tests full pass); link rejection, pw/passphrase/unreachable errors & Fail-First RED->GREEN request QR-SVG fix (RecoveryRequestAndSharesStep.tsx QrCodeDisplay)` |
| `src/components/auth/__tests__/WalletRecovery.test.tsx` | P1 | 9 | 0/0/0 | 🟢 Verified | `c29e83a` | `vitest 9/9 (98 files / 779 tests full pass); short pw, single/empty profile & error paths; Revert-Red WalletRecovery.tsx newPassword.length < 8` |
| `src/components/conflicts/__tests__/ConflictViews.test.tsx` | P1 | 16 | 0/0/0 | 🟢 Verified | `cc02db3` | `vitest 16/16 pass; tested blockedByOffender 0-trusted warning, 512B UTF-8 cap, bare root DID revocation, SST & witness conflicts; Revert-Red ConflictDetailsView !blockedByOffender` |
| `src/components/contacts/__tests__/WotContacts.test.tsx` | P1 | 19 | 0/0/0 | 🟢 Verified | `cc02db3` | `vitest 19/19 pass; added ContactDialog self-active block, subaccount null trust, sibling inheritance & rejection tests; Revert-Red ContactDialog isExactSelf` |
| `src/components/marketplace/__tests__/AdCard.test.tsx` | P1 | 18 | 0/0/0 | 🟢 Verified | `82896e2` | `vitest 18/18 pass; Fail-First RED->GREEN AdCard manualLoadedKey across imageToken changes, hardened warning colors, resolveAuthorHops & cellular isOwn` |
| `src/components/marketplace/__tests__/AdEditorModal.test.tsx` | P1 | 16 | 0/0/0 | 🟢 Verified | `82896e2` | `vitest 16/16 pass; added event date guards, 6h edit grace, length limits, preset chips, cardPublished=false & error paths; Revert-Red AdEditorModal maxEventSecs` |
| `src/components/marketplace/__tests__/BoardTab.test.tsx` | P1 | 12 | 0/0/0 | 🟢 Verified | `bb8e068` | `vitest 12/12 pass; Fail-First RED->GREEN BoardTab normalizeCoordinates guard, hardened search/filter chips, reset filters, subaccount coords & hop deferral` |
| `src/components/marketplace/__tests__/ImportCardModal.test.tsx` | P1 | 11 | 0/0/0 | 🟢 Verified | `82896e2` | `vitest 11/11 pass; hardened counter-scan QR SVG, covered contact URI sibling trust, Level-0 delete/clear & errors; Revert-Red ImportCardModal isExactSameUserId` |
| `src/components/marketplace/__tests__/MarketPersonView.test.tsx` | P1 | 13 | 0/0/0 | 🟢 Verified | `82896e2` | `vitest 13/13 pass; Fail-First RED->GREEN openUrl scheme/normalization & address.fullAddress fallback, inquiry, forward & contact state` |
| `src/components/marketplace/__tests__/MarketplaceView.test.tsx` | P1 | 19 | 0/0/0 | 🟢 Verified | `bb8e068` | `vitest 19/19 pass; hardened loading state deferred promise, manual sync toast/error matrix, board/inquiry navigation, events & IPC; Revert-Red useMarketplaceView !hasNewItems` |
| `src/components/marketplace/__tests__/MineTab.test.tsx` | P1 | 12 | 0/0/0 | 🟢 Verified | `bb8e068` | `vitest 12/12 pass; hardened share/publish toggles, delete modal in-flight state, empty states, expired renew & inquiry thread ops; Revert-Red MineTab isExpiredEventDraft` |
| `src/components/marketplace/__tests__/PeopleTab.test.tsx` | P1 | 10 | 0/0/0 | 🟢 Verified | `bb8e068` | `vitest 10/10 pass; Fail-First RED->GREEN PeopleTab normalizeCoordinates guard, hardened badge/location assertions, multi-field search & overflow cap` |
| `src/components/marketplace/__tests__/ProfileCardEditor.test.tsx` | P1 | 16 | 0/0/0 | 🟢 Verified | `bb8e068` | `vitest 16/16 pass; removed dup beforeEach, added skill limits/reordering, forwarded preview filter, errors & preset/rerender state; Revert-Red clampStreetVisibility` |
| `src/components/marketplace/__tests__/RenewAdModal.test.tsx` | P1 | 7 | 0/0/0 | 🟢 Verified | `82896e2` | `vitest 7/7 pass; covered null ad, error rollback, in-flight duplicate guard, late close & live preview; Revert-Red RenewAdModal isSubmitting` |
| `src/components/marketplace/__tests__/ShareCardModal.test.tsx` | P1 | 10 | 0/0/0 | 🟢 Verified | `82896e2` | `vitest 10/10 pass; hardened exact .hmcard bytes & path, covered null disable, write error, reset & unsafe SVG rejection; Revert-Red ShareCardPanel !fileWire` |
| `src/components/settings/__tests__/BackupSettingsCard.test.tsx` | P1 | 18 | 0/0/0 | 🟢 Verified | `cc02db3` | `vitest 18/18 pass; hardened password & try/finally listener cleanup, added toggle/unlink rollbacks, syncBackup & backup-now routing; Revert-Red BackupSettingsCard rollback` |
| `src/components/settings/__tests__/ViewAndLockCard.test.tsx` | P1 | 5 | 0/0/0 | 🟢 Verified | `cc02db3` | `vitest 5/5 pass; added decimal flooring, null/external sync, in-flight lock & disabled guard; Revert-Red ViewAndLockCard parsed >= 0` |
| `src/components/ui/__tests__/L2QuorumBadge.test.tsx` | P1 | 14 | 0/0/0 | 🟢 Verified | `cc02db3` | `vitest 16/16 pass (14 fn); hardened standalone props & added quorum precedence, yellow/0-node/NaN/100% clamp & rose conflict bar; Revert-Red L2QuorumBadge unsupportedConfirmation` |
| `src/locales/__tests__/i18nIntegrity.test.ts` | P1 | 4 | 0/0/0 | 🟢 Verified | `9c2354c` | `vitest 4/4 (752/752 pass); removed silent typeof skips & weak asserts; Revert-Red de.json missing anchorIssue key` |
| `src/utils/__tests__/signatureHints.test.ts` | P1 | 15 | 0/0/0 | 🟢 Verified | `cc02db3` | `vitest 15/15 pass; removed eslint-disable/any, covered 2-field and, whitespace/address, deduplication, ISO-5218 rules & gentle hints; Revert-Red signatureHints genderBalanceMale` |
| `src-tauri/src/sync_client/tests.rs` | P2 | 34 | 0/0/0 | 🟢 Verified | `c4bc457` | `34/34 pass (385/385 lib); Fail-First RED->GREEN revocation vk.is_weak(); cargo mutants revocation.rs 21/21 viable caught (1 unviable) + ssrf.rs 8/8 viable caught (1 unviable)` |
| `src-tauri/src/commands/vouchers.rs` | P2 | 11 | 0/0/0 | 🟢 Verified | `1ab2edd` | `11/11 pass (402/402 lib); split 5 mega-tests, removed outer-JSON if-let-Ok skip, bare is_err/is_some & TempDir discards, added edge/locked-fallback checks; cargo mutants open_voucher_signing_request_impl 7/7, resolve_manifest_candidate 5/5 & evaluate_guarantor_anchor_check_with_service 23/23 caught (35/35 total)` |
| `src-tauri/src/services/sync_engine/channel_reader.rs` | P2 | 11 | 0/0/0 | 🟢 Verified | `1ab2edd` | `11/11 pass (402/402 lib); Fail-First RED->GREEN Commit 1 e0a9a11 sticky revocation & disabled revoked public_sync; removed accept if-let-Ok, split mega-tests & checked HTTP mocks; cargo mutants apply_revocation_notice 2/2 & channel_reader targets 22/22 viable caught (1 unviable)` |
| `src/components/wallet/__tests__/Dashboard.test.tsx` | P2 | 9 | 1/0/0 | ⚪ Pending | — | — |
| `src-tauri/src/commands/contacts.rs` | P2 | 8 | 0/0/0 | 🟢 Verified | `c4bc457` | `8/8 pass; hardened roundtrip, get_cached_contacts & exact descriptor validation errors + DID norm & delete tests; cargo mutants validate_contact_sync_descriptor(s) 7/7 caught` |
| `src-tauri/src/services/fs_helpers.rs` | P2 | 8 | 0/0/0 | 🟢 Verified | `c4bc457` | `8/8 pass; Fail-First RED->GREEN collect_custom_standards_from_dir symlinks/1MiB guard, RAII TempDirGuard & exact errors; cargo mutants 15/17 caught (+2 equiv is_symlink||!is_dir)` |
| `src-tauri/src/services/post_service/tests.rs` | P2 | 27 | 0/5/40 | ⚪ Pending | — | — |
| `src-tauri/src/services/sync_endpoints.rs` | P2 | 6 | 0/0/0 | 🟢 Verified | `c4bc457` | `6/6 pass; split into focused unit tests with exact errors & single-field/overlap/sanitize boundary checks; cargo mutants 58/63 + recheck 25/25 caught (63/63 total)` |
| `src-tauri/src/services/transfer_service.rs` | P2 | 9 | 0/4/15 | ⚪ Pending | — | — |
| `src-tauri/src/services/settings_service.rs` | P2 | 10 | 0/0/0 | 🟢 Verified | `1ab2edd` | `10/10 pass (402/402 lib); split 2 mega-tests, TempDirGuard, exact preset/fault errors, wake parity incl. wifi-unlock & DailyPay/Full/conflict override gates; cargo mutants presets 13/13, patch_ui_preferences_impl 18/18 & patch_security_policy_impl 22/22 caught (53/53 total)` |
| `src-tauri/src/services/sync_engine/mod.rs` | P2 | 22 | 0/2/41 | ⚪ Pending | — | — |
| `src-tauri/src/commands/utils.rs` | P2 | 2 | 0/1/2 | ⚪ Pending | — | — |
| `src-tauri/src/services/preflight.rs` | P2 | 2 | 0/1/7 | ⚪ Pending | — | — |
| `src-tauri/src/services/sync_coordinator.rs` | P2 | 5 | 0/1/3 | ⚪ Pending | — | — |
| `src-tauri/src/services/sync_engine/card_sync.rs` | P2 | 4 | 0/1/6 | ⚪ Pending | — | — |
| `src/components/settings/__tests__/SyncServiceSettingsCard.test.tsx` | P2 | 16 | 0/1/0 | ⚪ Pending | — | — |
| `src-tauri/src/commands/profile.rs` | P2 | 4 | 0/0/4 | ⚪ Pending | — | — |
| `src-tauri/src/commands/standards.rs` | P2 | 1 | 0/0/1 | ⚪ Pending | — | — |
| `src-tauri/src/intent_reconciliation.rs` | P2 | 6 | 0/0/2 | ⚪ Pending | — | — |
| `src-tauri/src/models/wallet.rs` | P2 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src-tauri/src/services/balance_service.rs` | P2 | 6 | 0/0/2 | ⚪ Pending | — | — |
| `src-tauri/src/services/common.rs` | P2 | 3 | 0/0/0 | ⚪ Pending | — | — |
| `src-tauri/src/services/post_service/handlers.rs` | P2 | 2 | 0/0/2 | ⚪ Pending | — | — |
| `src-tauri/src/services/post_service/mod.rs` | P2 | 0 | 0/0/0 | ⚪ Pending | — | — |
| `src-tauri/src/services/post_service/outbox.rs` | P2 | 0 | 0/0/0 | ⚪ Pending | — | — |
| `src-tauri/src/services/sync_engine/admin.rs` | P2 | 0 | 0/0/0 | ⚪ Pending | — | — |
| `src-tauri/src/sync_client/mod.rs` | P2 | 0 | 0/0/0 | 🟢 Verified | `c4bc457` | `0 inline tests; #[cfg(test)] mod tests covered by verified sibling tests.rs` |
| `src-tauri/src/sync_zip.rs` | P2 | 3 | 0/0/1 | ⚪ Pending | — | — |
| `src/components/profile/__tests__/QuickContactModal.test.tsx` | P2 | 5 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/settings/__tests__/AutoSyncSummaryCard.test.tsx` | P2 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/wallet/__tests__/CreateVoucher.test.tsx` | P2 | 12 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/wallet/__tests__/FirstStepsChecklist.test.tsx` | P2 | 4 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/wallet/__tests__/SignRequestView.test.tsx` | P2 | 10 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/wallet/__tests__/VoucherDetailsView.test.tsx` | P2 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src/context/__tests__/NavigationContext.test.tsx` | P2 | 11 | 0/0/0 | ⚪ Pending | — | — |
| `src/context/__tests__/SessionContext.test.tsx` | P2 | 18 | 0/0/0 | ⚪ Pending | — | — |
| `src/context/__tests__/SessionContextEvents.test.tsx` | P2 | 2 | 0/0/0 | ⚪ Pending | — | — |
| `src/context/__tests__/UiModeContext.test.tsx` | P2 | 2 | 0/0/0 | ⚪ Pending | — | — |
| `src/services/__tests__/commandAccessParity.test.ts` | P2 | 4 | 0/0/0 | ⚪ Pending | — | — |
| `src/services/__tests__/eventParity.test.ts` | P2 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src/services/__tests__/serviceReachability.test.ts` | P2 | 4 | 0/0/0 | ⚪ Pending | — | — |
| `src/services/__tests__/tauriCommands.test.ts` | P2 | 4 | 0/0/0 | ⚪ Pending | — | — |
| `src/sync/__tests__/syncBridge.test.ts` | P2 | 3 | 0/0/0 | ⚪ Pending | — | — |
| `src/test/browserStorage.test.ts` | P2 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src/utils/__tests__/walletEvents.test.ts` | P2 | 7 | 0/0/0 | ⚪ Pending | — | — |
| `src/utils/__tests__/humocoUri.test.ts` | P3 | 28 | 4/0/0 | ⚪ Pending | — | — |
| `src/help/__tests__/helpTopics.test.ts` | P3 | 5 | 3/0/13 | ⚪ Pending | — | — |
| `src/components/shell/__tests__/StatusChip.test.tsx` | P3 | 8 | 2/0/0 | ⚪ Pending | — | — |
| `src/locales/__tests__/glossaryLint.test.ts` | P3 | 8 | 1/0/0 | ⚪ Pending | — | — |
| `src-tauri/src/state/tests.rs` | P3 | 22 | 0/12/26 | ⚪ Pending | — | — |
| `src/utils/__tests__/imageCompression.test.ts` | P3 | 6 | 0/8/0 | ⚪ Pending | — | — |
| `src-tauri/src/pin_keystore.rs` | P3 | 3 | 0/7/2 | ⚪ Pending | — | — |
| `src-tauri/src/settings.rs` | P3 | 4 | 0/4/0 | ⚪ Pending | — | — |
| `src-tauri/src/models/post.rs` | P3 | 1 | 0/3/0 | ⚪ Pending | — | — |
| `src-tauri/src/models/voucher.rs` | P3 | 1 | 0/1/0 | ⚪ Pending | — | — |
| `src-tauri/src/test_support.rs` | P3 | 1 | 0/1/0 | ⚪ Pending | — | — |
| `run-tests.sh` | P3 | 0 | 0/0/0 | ⚪ Pending | — | — |
| `src-tauri/src/gateway_resolver.rs` | P3 | 5 | 0/0/3 | ⚪ Pending | — | — |
| `src-tauri/src/lib.rs` | P3 | 5 | 0/0/2 | ⚪ Pending | — | — |
| `src-tauri/src/models/error.rs` | P3 | 5 | 0/0/0 | ⚪ Pending | — | — |
| `src-tauri/src/state/mod.rs` | P3 | 0 | 0/0/0 | ⚪ Pending | — | — |
| `src/__tests__/AppLaunchQueue.test.tsx` | P3 | 4 | 0/0/0 | ⚪ Pending | — | — |
| `src/__tests__/beginnerFlows.test.tsx` | P3 | 8 | 0/0/0 | ⚪ Pending | — | — |
| `src/__tests__/uiModeLint.test.ts` | P3 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/contacts/__tests__/AddContactModal.test.tsx` | P3 | 10 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/contacts/__tests__/AddressBook.test.tsx` | P3 | 23 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/contacts/__tests__/ContactDialog.test.tsx` | P3 | 10 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/contacts/__tests__/TrustLevelRadioCards.test.tsx` | P3 | 6 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/send/__tests__/PrivacyToggle.test.tsx` | P3 | 3 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/settings/__tests__/SettingsView.test.tsx` | P3 | 16 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/shell/__tests__/GlobalFileDropZone.test.tsx` | P3 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/shell/__tests__/NotificationsAndAvatar.test.tsx` | P3 | 4 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/shell/__tests__/Sidebar.test.tsx` | P3 | 7 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/standards/__tests__/StandardsManager.test.tsx` | P3 | 6 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/support/__tests__/BugReportView.test.tsx` | P3 | 6 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/support/__tests__/HelpCenterView.test.tsx` | P3 | 7 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/support/__tests__/SupportView.test.tsx` | P3 | 3 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/transfer/__tests__/ReceiveSuccessView.test.tsx` | P3 | 14 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/transfer/__tests__/ReceiveView.test.tsx` | P3 | 7 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/transfer/__tests__/SendView.test.tsx` | P3 | 27 | 0/0/1 | ⚪ Pending | — | — |
| `src/components/transfer/__tests__/TransactionHistoryView.test.tsx` | P3 | 11 | 0/0/1 | ⚪ Pending | — | — |
| `src/components/transfer/__tests__/TransferSuccessView.test.tsx` | P3 | 4 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/ui/__tests__/HelpModal.test.tsx` | P3 | 4 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/ui/__tests__/InfoHint.test.tsx` | P3 | 3 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/ui/__tests__/NewPrimitives.test.tsx` | P3 | 9 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/ui/__tests__/PageLayout.test.tsx` | P3 | 21 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/ui/__tests__/QrScannerModal.test.tsx` | P3 | 2 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/ui/__tests__/TrustBadge.test.tsx` | P3 | 9 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/ui/__tests__/UiModePrimitives.test.tsx` | P3 | 6 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/ui/__tests__/UiPrimitives.test.tsx` | P3 | 28 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/voucher/__tests__/ExportSigningRequestModal.test.tsx` | P3 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/voucher/__tests__/TimelineSection.test.tsx` | P3 | 3 | 0/0/0 | ⚪ Pending | — | — |
| `src/components/voucher/__tests__/VoucherBasicsForm.test.tsx` | P3 | 3 | 0/0/1 | ⚪ Pending | — | — |
| `src/help/__tests__/HelpContentRenderer.test.tsx` | P3 | 6 | 0/0/0 | ⚪ Pending | — | — |
| `src/hooks/__tests__/useContactResolver.test.ts` | P3 | 4 | 0/0/0 | ⚪ Pending | — | — |
| `src/hooks/__tests__/useCopyFeedback.test.ts` | P3 | 3 | 0/0/0 | ⚪ Pending | — | — |
| `src/hooks/__tests__/useParsedStandards.test.ts` | P3 | 2 | 0/0/0 | ⚪ Pending | — | — |
| `src/hooks/__tests__/useRateLimitCooldown.test.ts` | P3 | 7 | 0/0/0 | ⚪ Pending | — | — |
| `src/hooks/__tests__/useVoucherCreation.test.ts` | P3 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src/hooks/__tests__/useVoucherFilters.test.ts` | P3 | 7 | 0/0/0 | ⚪ Pending | — | — |
| `src/hooks/__tests__/useVoucherSelection.test.ts` | P3 | 1 | 0/0/0 | ⚪ Pending | — | — |
| `src/utils/__tests__/csvExport.test.ts` | P3 | 14 | 0/0/0 | ⚪ Pending | — | — |
| `src/utils/__tests__/geoUtils.test.ts` | P3 | 24 | 0/0/0 | ⚪ Pending | — | — |
| `src/utils/__tests__/humocoFileClassifier.test.ts` | P3 | 5 | 0/0/0 | ⚪ Pending | — | — |
| `src/utils/__tests__/seedUtils.test.ts` | P3 | 5 | 0/0/0 | ⚪ Pending | — | — |
| `src/utils/__tests__/userIdHelper.test.ts` | P3 | 12 | 0/0/0 | ⚪ Pending | — | — |
