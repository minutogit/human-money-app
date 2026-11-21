# Post-Audit Design-Intent & False-Positive Triage

> **Mandatory Second-Pass Filter:** This triage step MUST be executed at the end of every AI audit run, *after* candidate findings are identified and *before* any code modifications or implementation commits are made.

---

## 🎯 Purpose & Threat-Model Realities

Automated AI security auditors often evaluate desktop wallets using generic web-application, SaaS cloud, or centralized ledger heuristics (e.g., OWASP Web top-10, CWE-359 *"Information Disclosure"*, or centralized auth assumptions). In a web browser talking to a single trusted backend, retaining counterparty identifiers, disabling native drag-and-drop hooks, allowing optional offline-only mode, or offering a device-bound convenience PIN can look suspicious.

In **`human-money-app`**, the security model is a **local-first, zero-knowledge, thin-bridge Tauri v2 desktop client** backed by `human_money_core`, `humoco-wot`, and `humoco-market`:
* **Thin Tauri Bridge (`human_money_core` Sovereignty):** Layer-1 cryptography, voucher validation, SST V4 traps, and wallet state belong strictly in `human_money_core`. The app bridges IPC commands (`camelCase` DTOs) and host/UI workflows without duplicating or overriding Core L1 rules.
* **Decentralized Offline Cash & Local Forensics:** Local encrypted wallet storage and unlocked RAM caches intentionally retain immediate 1-hop counterparty DIDs for offline double-spend forensics (`human-money-core` SST V4).
* **Zero-Knowledge Public Sync Relays:** Public `humoco-sync` channels are untrusted blind ciphertext drops (`HMSLOT02`/`HMSLOTM2`, AEAD-sealed client-side), never trusted identity servers.
* **User-Tiered Usability vs. High-Privilege Guardrails:** Simple vs. Expert UI modes (`uiMode`), Quick PIN (`pin_keystore.rs`), and Cashier (`receiveOnly`) mode enforce explicit capability boundaries without sacrificing offline usability.

Blindly "fixing" intentional offline-first, thin-bridge, or zero-knowledge design properties breaks core wallet safety and usability.

---

## 📚 Required Reference Materials

Before triaging findings, the auditor MUST inspect:
1. [`AGENTS.md`](../../AGENTS.md) — Thin-bridge architecture, IPC/DTO rules, zero-compat policy, Tailwind v4, and CI/test verification workflow.
2. [`docs/UX_GLOSSARY.md`](../UX_GLOSSARY.md) — Mandatory Simple/Expert terminology and strict WoT guarantor prohibition.
3. [`docs/UI_AUDIT_GUIDE.md`](../UI_AUDIT_GUIDE.md) — Guided UI tour & automated visual/console audit catalog (`1.1`–`5.6`).
4. [`TESTING_STRATEGY.md`](../../TESTING_STRATEGY.md) — 3-layer Vitest + Tauri IPC + E2E testing strategy.
5. [`STATUS.md`](../../STATUS.md) — Implemented security and architecture milestones (`APP-065`–`APP-074`, `APP-UX-07`–`APP-UX-11`).
6. [`human-money-core/docs/security/ai-audits/DESIGN_INTENT_TRIAGE.md`](../../../human-money-core/docs/security/ai-audits/DESIGN_INTENT_TRIAGE.md) — Upstream Core L1 sovereignty, SST V4, Sync-Lock, and local forensics invariants.

---

## 🏛️ Key Intentional Architectural Invariants (v0.5.16+)

Auditors must evaluate candidate findings against these 10 non-negotiable architectural invariants:

### 1. Thin Tauri Bridge & Core Sovereignty (`AGENTS.md`, Non-Negotiable)
- **Invariant:** Layer-1 cryptography, voucher validation, canonical `snake_case` serialization, and wallet event-sourcing stay exclusively in `human_money_core`. `human-money-app` (`src-tauri/src/models/` + `src/types/`) maintains `camelCase` DTO boundaries and host/network adapters without duplicating L1 validation in TypeScript.
- **Rule:** Never flag the absence of duplicated L1 signature/ProofChain verification in TypeScript or demand Core schema changes for UI convenience (`[FALSE POSITIVE / MISINTERPRETATION]`). *(By contrast, missing `AccessMode` checks, lossy DTO mapping, or business-logic drift in `src-tauri` ARE valid defects.)*

### 2. Offline-First Local Encrypted State & Counterparty Forensics
- **Invariant:** Encrypted local profile storage, local transaction history, and unlocked in-RAM `ActiveProfileCaches` intentionally retain immediate 1-hop counterparty DIDs to support offline double-spend forensics (`SST V4`) and dispute attribution. This local state is never transmitted unencrypted over the network.
- **Rule:** Flagging local encrypted retention or unlocked-session RAM retention as CWE-359 information leakage (when NOT exposed to logs, error messages, DOM attributes across lock/logout, or network payloads) is an `[INTENTIONAL DESIGN REQUIREMENT]`.

### 3. Device-Bound Quick PIN & Cashier Mode (`AccessMode::{Full, DailyPay, ReceiveOnly}`, `APP-069`)
- **Invariant:** `src-tauri/src/pin_keystore.rs` stores a device-bound AES-GCM-wrapped 32-byte `device_pepper` (`pin_keystore.json`, `0o600` permissions) with `MAX_PIN_ATTEMPTS = 5` auto-wipe and binds to Core's `.secondary_auth.json` sidecar (which is intentionally excluded from integrity manifests and cross-device backups). High-privilege operations require `AccessMode::Full` and/or `OneShotPinGrant` master-password step-up.
- **Rule:** Flagging the existence of a 6–8 digit convenience PIN tier or the exclusion of `.secondary_auth.json` / `pin_keystore.json` from backups/integrity manifests as a defect is an `[INTENTIONAL DESIGN REQUIREMENT]`. *(Missing `AccessMode` enforcement, `OneShotPinGrant` bypasses, missing 5-attempt wipe, or gaining `Full` access from `DailyPay`/`ReceiveOnly` remain `[CONFIRMED VULNERABILITY]`.)*

### 4. Zero-Config Sync-Locks, Explicit `N=0` Offline-Only Mode & Safe `N=1 -> 2/3` Auto-Growth (`APP-070`–`APP-074`)
- **Invariant:** Users may explicitly confirm `N=0` offline-only voucher creation (protected by visible TOCTOU `l2CommitteeChanged` confirmation and `explicitOfflineOnly` intent), and the creator-only background/lifecycle path (`l2_lifecycle.rs`) intentionally supports `0s`-timelock pure committee growth (`N=1 -> 2/3`) via `humoco_proto::is_pure_committee_growth` when `guarantor_mig_pubs` is empty (strictly forbidding committee shrink or server replacement).
- **Rule:** Flagging explicit user-confirmed `N=0` offline issuance or `0s`-timelock pure committee growth (`N=1 -> 2/3` without guarantors) as a missing quorum check is an `[INTENTIONAL DESIGN REQUIREMENT]`. *(Silent downgrades, shrinking, unconfirmed TOCTOU committee changes, or guarantor-protection reduction remain valid defects.)*

### 5. Z01 Fail-Closed PoS Checkout & `Quarantined` Blocking (`APP-070`/`APP-074`)
- **Invariant:** Sync-Lock quorum responses are mathematical lock observations only. Automatic goods release strictly requires `isGoodsReleaseAuthorized` (backed by Core's `DurablePurchaseBinding`), and `Quarantined` verdicts intentionally block `pendingCommit` retry flows.
- **Rule:** Flagging strict refusal of automatic goods release on `OfflineOnly`, `Yellow`, or `Quarantined` verdicts as a broken payment flow or UX defect is a `[FALSE POSITIVE / MISINTERPRETATION]`.

### 6. Social Recovery 2-Factor Architecture (`humoco-market`, `APP-065`/`APP-066`/`APP-069`)
- **Invariant:** Social recovery intentionally combines GF(256) Shamir shares (`owner_sig` + BLAKE3 `share_commitments`, X25519-sealed via PoST) with a 3-from-12 personal-question Argon2id key (`C = S XOR P`, NFC/umlaut-normalized) and zero-plaintext spaced-repetition memory rehearsal verifiers (`compute_rehearsal_verifiers` / `verify_rehearsal_answers`).
- **Rule:** Flagging the absence of a centralized recovery server, the personal-question second factor, or zero-plaintext rehearsal verifier hashes as a defect is an `[INTENTIONAL DESIGN REQUIREMENT]`. *(Plaintext answer persistence, missing signature/commitment verification, unbounded Argon2id OOM parameters, or stale recovery session leaks remain `[CONFIRMED VULNERABILITY]`.)*

### 7. Zero-Knowledge Sync Channels, ETag Polling, Deflate (`HMSLOT02`) & Relocation Notices (`APP-067`–`APP-SYNC-02`)
- **Invariant:** Public `humoco-sync` servers act as blind encrypted drops (`HMSLOT02`/`HMSLOTM2` Deflate compression capped at 8 MiB before bucket padding and AEAD encryption). Clients pin `drop_pub`, verify signed `HUMOCO-DROP-REVOKE-V1` / `HUMOCO-LOCK-DELEGATE-V1` notices, follow verified server relocations without revoking keys, support quiet-friend card forwarding without `own_card`, and enforce deterministic poll cooldowns (`post` 90s, `card`/`wot` 20m) with `ETag`/`If-None-Match`.
- **Rule:** Flagging unauthenticated public read URLs on AEAD-sealed slots, deterministic poll cooldowns, or quiet-friend card forwarding without `own_card` as security bugs is a `[FALSE POSITIVE / MISINTERPRETATION]`. *(Unverified `drop_pub`/`admin_pub` bindings, zip-bomb bypasses, SSRF/DNS-rebinding gaps, or leaking reader keys remain valid defects.)*

### 8. Simple vs. Expert Mode (`uiMode`) & Mandatory `UX_GLOSSARY.md` Terminology (`APP-UX-07`/`APP-UX-09`)
- **Invariant:** Standard UI copy intentionally avoids technical jargon (`Wallet`, `DID`, `Hops`, `.wot`, generic `Server`) and strictly prohibits `bürgen`/`haften` in Web-of-Trust contexts (reserving guarantor terminology exclusively for formal L1 voucher guarantors), enforced automatically via `glossaryLint.test.ts` and `uiModeLint.test.ts`. Technical details are progressively disclosed via `<Advanced>`, `<AdvancedDisclosure>`, and `<TechId>`.
- **Rule:** Flagging plain-language user copy or Simple-mode progressive disclosure as "hiding cryptographic details" is an `[INTENTIONAL DESIGN REQUIREMENT]`.

### 9. HTML5 File Drag-and-Drop (`dragDropEnabled: false` in `src-tauri/tauri.conf.json`)
- **Invariant:** `src-tauri/tauri.conf.json` sets `"dragDropEnabled": false` so WebView HTML5 drag-and-drop events reach `GlobalFileDropZone.tsx` (handling `.transfer`, `.ask`, `.sig`, `.wot`, `.hmcard`, `.hmcbackup`, `.hmcproof`, `.standard`, and logged-out backup staging).
- **Rule:** Flagging `"dragDropEnabled": false` alone as an accidental misconfiguration is an `[INTENTIONAL DESIGN REQUIREMENT]`.
- **Audit Scope Boundary:** By contrast, `"csp": null` in `src-tauri/tauri.conf.json` and `"fs:scope": { "allow": ["**"] }` in `src-tauri/capabilities/default.json` are **NOT** exempted here and remain valid, in-scope security audit subjects!

### 10. Pre-Launch Zero-Compat Policy (`AGENTS.md`)
- **Invariant:** Breaking changes across IPC commands, DTOs, settings, and local schemas are applied cleanly on both Rust and TypeScript sides without `#[serde(alias = ...)]`, fallback parsers, or migration shims.
- **Rule:** Flagging missing backwards-compatibility shims or legacy IPC aliases as a regression is a `[FALSE POSITIVE / MISINTERPRETATION]`.

---

## 🔍 The 4-Step Triage Questionnaire

For **every** candidate finding produced during an audit pass, answer these four questions:

### 1. Trust-Boundary & Exposure Analysis
* **Question:** Does the finding cross an untrusted boundary (IPC from WebView/DOM, untrusted QR/URI/dropped file, remote Sync/PoST/L2 HTTP response, OS log/clipboard/disk permissions, or `AccessMode` downgrade), or does it remain solely inside authenticated, encrypted local state?
* **Rule:** Data stored inside encrypted wallet storage or active unlocked RAM structures without leaking into logs, unauthenticated IPC responses, DOM/export sinks, or network payloads is **local state**, not an external disclosure vulnerability.

### 2. Offline & Thin-Bridge Resilience
* **Question:** Would applying the suggested fix force synchronous network dependencies, duplicate Core L1 validation in TypeScript/Tauri, or break offline P2P / Cashier / recovery workflows?
* **Rule:** Features required for offline P2P transfers, local forensics, or thin-bridge Core delegation are intentional and must not be broken to satisfy generic web heuristics.

### 3. Established Architectural Decision Check
* **Question:** Is this behavior explicitly required by `AGENTS.md`, `UX_GLOSSARY.md`, `STATUS.md` (`APP-065`–`APP-074`, `APP-UX-07`–`APP-UX-11`), or `human-money-core`'s `DESIGN_INTENT_TRIAGE.md`?
* **Rule:** If the behavior was deliberately designed to satisfy a documented ecosystem invariant, classify it as `[INTENTIONAL DESIGN REQUIREMENT]`.

### 4. Functional Trade-Off Evaluation
* **Question:** Does the proposed change remove an essential security guardrail (`isGoodsReleaseAuthorized`, `AccessMode::Full`, `OneShotPinGrant`, `l2CommitteeChanged`) or break automated QS contracts (`commandAccessParity`, `glossaryLint`, `i18n:check`)?
* **Rule:** Never relax fail-closed wallet/checkout/auth checks or violate `UX_GLOSSARY.md` to silence a generic static-analysis warning.

---

## 🏷️ Triage Classification Outcomes

Classify every candidate finding into one of three outcomes:

### Outcome A: `[CONFIRMED VULNERABILITY]`
* **Definition:** A genuine security, authorization, concurrency, atomicity, SSRF, XSS/injection, secret-leak, arithmetic, or IPC/DTO parity defect verified across callers and boundaries.
* **Action:** Include in the verified findings list, cluster by root cause, and resolve via Fail-First TDD + full `./run-tests.sh --quiet` verification.

### Outcome B: `[INTENTIONAL DESIGN REQUIREMENT]`
* **Definition:** Behavior flagged by generic heuristics that is actually required by the app's offline-first, thin-bridge, Quick-PIN/Cashier, Zero-Config Sync-Lock, Social Recovery, Simple/Expert UX, or HTML5 drop architecture.
* **Action:**
  1. **DO NOT MODIFY** the operational logic.
  2. **Preserve / Clarify Comments:** Add or retain a concise comment explaining *why* the behavior is intentional if helpful.
  3. **Verify Invariant Tests:** Confirm automated tests protect the intentional behavior.

### Outcome C: `[FALSE POSITIVE / MISINTERPRETATION]`
* **Definition:** The candidate finding relies on a misread call chain, an unreachable code path, an existing `SessionToken`/`AccessMode`/generation guard, or a generic web assumption incompatible with HuMoCo.
* **Action:** Discard the finding with a concise evidence-based rationale.

---

## 📋 Triage Report Template

Include this summary when documenting triage decisions in `prompts/reports/` or review summaries:

```markdown
### Post-Audit Design-Intent Triage Summary

| Finding ID | Suspected CWE | Triage Outcome | Rationale / Architectural Requirement | Action Taken |
| :--- | :--- | :--- | :--- | :--- |
| APP-S03-01 | CWE-521 | `[INTENTIONAL DESIGN REQUIREMENT]` | 6–8 digit Quick PIN uses device-bound AES-GCM `device_pepper` (`0o600`), 5-attempt auto-wipe, and `OneShotPinGrant` step-up (`APP-069`). | Preserved; audited `AccessMode` boundaries instead. |
| APP-S05-02 | CWE-287 | `[INTENTIONAL DESIGN REQUIREMENT]` | Creator-only `N=1 -> 2/3` pure committee growth uses `0s` timelock via `is_pure_committee_growth` when `guarantor_mig_pubs` is empty (`APP-074`). | Preserved; shrink/replacement remains blocked. |
| APP-S08-03 | CWE-862 | `[FALSE POSITIVE / MISINTERPRETATION]` | Public sync slot GETs return client-side AEAD-sealed `HMSLOT02` blobs; confidentiality relies on E2E AEAD + `drop_pub` pinning, not read auth. | Discarded as false positive. |
| APP-S15-04 | CWE-285 | `[INTENTIONAL DESIGN REQUIREMENT]` | PoS automatic goods release remains blocked without `isGoodsReleaseAuthorized` (`DurablePurchaseBinding`) even on verified L2 observation (Z01 fail-closed). | Invariant verified in `ReceiveSuccessView`. |
```
