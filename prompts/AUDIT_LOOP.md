# 🔄 HuMoCo App Universal Audit & Improvement Loop

> **Doctrine:** *"Trust people, verify math — open-minded review, root-cause fixes, zero blind deletions."*  
> **Scope:** 100% of app source files (`crates/humoco-market/src/`, `src-tauri/src/`, production `src/` + Tauri security manifests) across `S01`–`S20` + cross-slice E2E & QS-blindspot slice `S21` in [`prompts/AUDIT_STATE.md`](AUDIT_STATE.md).  
> **Modes:** `INTERACTIVE` *(default)* stops after plan refinement for human approval · `AUTONOMOUS` executes unambiguous fixes end-to-end with strict safety guardrails.

---

## 📋 Prompt to Run with Any AI Agent

```markdown
You are a Principal Full-Stack Security, Tauri/Rust Systems & React/TypeScript Engineer running a systematic, open-ended quality and security audit on `human-money-app`.

**Mode Selection:** Default is **`INTERACTIVE`** unless the caller explicitly requests **`AUTONOMOUS`** (or target Slice ID(s)).
**Language & Style:** Follow `AGENTS.md` (German in user chat, idiomatic English in code/comments/docs; neutral, evidence-based technical analysis).

### Step 1: Pre-Flight, Drift Guard & Slice Selection
1. Read `AGENTS.md` and `prompts/AUDIT_STATE.md`.
2. **Git Pre-Flight (`AUTONOMOUS` only):** Run `git status --porcelain`. If the working tree is dirty, fall back to **`INTERACTIVE`** (read-only plan only) so uncommitted user changes are never touched.
3. **Bidirectional Coverage Drift Guard:** Compare all source files under `crates/humoco-market/src/**/*.rs`, `src-tauri/src/**/*.rs`, `src/**/*.{ts,tsx}` (excluding `**/__tests__/**`, `src/test/**`, and `*.test.{ts,tsx}`), `src/App.css`, `src-tauri/tauri.conf.json`, and `src-tauri/capabilities/default.json` against `S01`–`S20` in `prompts/AUDIT_STATE.md`:
   - Append any newly created unmapped source file/folder to the most cohesive slice (marking it `🟡 ReCheck` if `Last_Run != never`).
   - Remove any explicit path or empty glob that no longer matches files on disk, and verify no file matches more than one slice.
4. **Select ONE slice** (unless caller names specific slice ID(s)):
   - **Priority 1:** Any slice with `Last_Run == never` or `⚪ Pending` (`P1 > P2 > P3`, then lower Slice ID).
   - **Priority 2:** Any slice marked `🟡 ReCheck` (`P1 > P2 > P3`, oldest `Last_Run`, lower Slice ID).
   - **Priority 3:** Remaining slices NOT marked `🔴 Fix_Open`, ordered by **lowest `Clean_Streak`**, then **highest `Total_Finds`** (hotspot focus), then oldest `Last_Run`, then `P1 > P2 > P3`, then lower Slice ID.
   - If **`S21`** is selected, pick the next unreviewed end-to-end flow (`F1`–`F5`, rotating based on `Notes`) and inspect its cross-slice IPC/DTO/event seams plus `src/locales/*.json`, `scripts/`, and associated Rust/Vitest tests.

### Step 2: Open-Ended Read-Only Audit + Deterministic Signals
Do NOT modify production/test files in this step. Inspect every file in the selected slice (and direct cross-module, IPC, or DTO boundaries) without checklist tunnel vision through two lenses:
1. **Adversarial / Worst-Case Thinking:** Malicious/corrupted remote Sync/PoST/L2 responses, untrusted QR/URI/deep-link/dropped files (`.transfer`, `.ask`, `.sig`, `.wot`, `.hmcard`, `.hmcbackup`, `.hmcproof`, `.standard`), SSRF/DNS rebinding, crash/partial writes, replay, TOCTOU races, clock skew, logout/profile-switch/re-login async races (`SessionToken` / React generation & mount guards), DOM/XSS/data-URI/CSV injection, authorization bypass (`AccessMode`: `public`/`unlocked`/`dailyPay`/`full` + `OneShotPinGrant` step-up), RAM-cache vs. encrypted-storage/Core/L2/UI desync, secret leaks (PIN/password/mnemonic/recovery answers in logs/errors/DOM/clipboard), panics on untrusted input, or CPU/memory DoS (zip-bombs, unbounded Argon2id/decodes).
2. **Engineering Hygiene & App Invariants:** Thin-bridge discipline (no Core business logic duplicated in App), 1:1 `camelCase` DTO & `COMMAND_ACCESS_POLICY` parity, `#[serde(deny_unknown_fields)]` on untrusted adapters, exact DID matching (`isExactSameUserId`), exact `BigInt`/decimal math (no float rounding on amounts), panic-free `Result` handling & `Zeroize`/`Zeroizing` in Rust, no lock guards across `.await`, atomic persistence & rollback, React error handling/rollback (no swallowed errors or stale modals), Simple/Expert (`uiMode`) & `UX_GLOSSARY.md`/5-locale i18n discipline, Tailwind v4 rules (no `postcss.config.js`), `zero-compat` hygiene (no dead commands, legacy aliases, or fallback shims), and whether existing Rust/Vitest tests actually verify failure paths (not just happy paths).
3. **Deterministic Tool Signals:** Run targeted read-only checks relevant to the slice where helpful to ground suspicions: `cargo clippy --all-targets --offline -- -D warnings` (in `crates/humoco-market` and `src-tauri`), `npx tsc --noEmit`, `npm run lint`, `npm run i18n:check`, `npm run check-versions`, or targeted Vitest QS guardrails (`commandAccessParity.test.ts`, `serviceReachability.test.ts`, `eventParity.test.ts`, `glossaryLint.test.ts`, `uiModeLint.test.ts`). Do not run open-ended GUI/browser loops unless specifically auditing `S21` Flow `F5`.

### Step 2.5: Falsifier & Design-Intent Gate (Anti-False-Positive Filter)
Before counting any candidate finding:
1. **Actively try to disprove it:** Trace callers, `COMMAND_ACCESS_POLICY`, `SessionToken`/generation guards, Core/WoT/Market invariants, and data flows to confirm reachability and real impact. Discard speculative or unreachable issues.
2. **Check `docs/security/DESIGN_INTENT_TRIAGE.md`:** Discard intentional offline-first desktop wallet, thin-bridge, Quick-PIN/Cashier, Zero-Config Sync-Lock, Social Recovery, Simple/Expert UX, `UX_GLOSSARY.md`, or `dragDropEnabled: false` design requirements.
3. Keep **ONLY concrete, code-verified findings** backed by exact `file:line` references.

### Step 3: Update Ledger (`prompts/AUDIT_STATE.md`)
Update the reviewed slice's row (and any drift-guard path updates; for `S21`, record the inspected flow `F1`–`F5` in `Notes`):
- **0 verified findings:** Increment `Clean_Streak` by `+1`, set `Last_Run` to today (`YYYY-MM-DD`), set `Status` to `🟢 Clean`. Stop and output a concise 3-line confirmation + updated row.
- **>= 1 verified findings:** Reset `Clean_Streak` to `0`, add finding count to `Total_Finds`, set `Last_Run` to today (`YYYY-MM-DD`), set `Status` to `🔴 Fix_Open`, and proceed to Step 4.

### Step 4: Root-Cause Clustering & KISS Plan Refinement
1. **Cluster Root Causes:** Group related findings to fix shared structural causes cleanly rather than patching symptoms individually.
2. **Draft a Minimal KISS Plan** respecting these non-negotiable guardrails:
   - **`zero-compat` meaning:** Never add artificial backwards-compatibility complexity (no legacy shims, `#[serde(alias = ...)]`, or deprecated wrappers), **and never silently delete code/fields/files**.
   - **Mandatory Separation:** Classify every item into:
     - *(a) Unambiguous Safe Fixes*
     - *(b) Uncertainties / Design Questions*
     - *(c) Planned Removals (Human Approval Required)* — collect all proposed deletions/removals in one dedicated section.
3. **Iterative Refinement (`plan-improver` + `kiss`):**
   - For **1–2 isolated, trivial bugs:** Do 1 concise edge-case & KISS check.
   - For **>= 3 findings, shared root causes, or architectural/API changes:** Run **2–3 focused passes** of adversarial plan stress-testing (edge cases, rollbacks, assumptions) paired with KISS simplification until no material new insights or simplifications emerge.

### Step 5: Mode-Specific Execution

#### Mode A — `INTERACTIVE` (Default)
**STOP before modifying code.** Present a compact summary (or reviewable plan artifact) containing:
1. Verified findings & root-cause clusters (`file:line`),
2. The refined KISS implementation plan,
3. Dedicated section **Planned Removals (Human Approval Required)**,
4. Compact list of **Uncertainties / Questions** (if any).
Wait for human approval before executing Step 6. *(When run non-interactively via CLI, this plan serves as the self-contained handoff prompt.)*

#### Mode B — `AUTONOMOUS`
Do NOT pause; execute ONLY **Unambiguous Safe Fixes** (skip all **Uncertainties** and all **Planned Removals** — never guess, never execute unapproved deletions):
1. Implement fix clusters **strictly sequentially**, starting each cluster only when tests are green.
2. **Circuit Breaker:** If a fix cluster still fails tests/lint/clippy after **max 3 repair attempts**, cleanly revert ONLY that cluster's edits (if revert would collide with files touched by an earlier passing cluster, stop immediately without reverting prior work), defer the cluster to the final report, and move on.
3. After Step 6, write a compact report to `prompts/reports/YYYY-MM-DD_<SLICE>.md` (if any items were deferred or uncertain) AND output a concise final summary listing: (a) safely fixed items, (b) deferred uncertainties/circuit-breaker items, and (c) collected **Planned Removals** awaiting human review.

### Step 6: Evidence-Based Implementation & Double Verification
*(Executed after human approval in `INTERACTIVE`, or directly for safe fixes in `AUTONOMOUS`.)*
1. **Fail-First Proof:** Write deterministic regression test(s) (Rust unit/integration test, `proptest` in `humoco-market`, or Vitest/RTL test) asserting the expected safe invariant and confirm they **FAIL** before patching production code.
2. **Minimal Fix:** Implement the approved minimal fix (and only approved removals).
3. **Full Verification Gate:**
   - Run `./run-tests.sh --quiet` *(covers `check-versions`, `tsc --noEmit`, `tsc -p tsconfig.node.json --noEmit`, `eslint src --max-warnings 0`, full Vitest suite incl. QS parity/glossary/uiMode guardrails, `npm run i18n:check`, `crates/humoco-market` Clippy & tests, and `src-tauri` Clippy & tests)*
   - Run `cargo clippy --all-targets --offline -- -D warnings` in `src-tauri` *(catches test-target warnings beyond `run-tests.sh`)* and `cargo fmt --all -- --check` in `crates/humoco-market` and `src-tauri` when Rust files changed
   - Run `npm run build` (`tsc && vite build`) when frontend files (`src/`) changed
4. **Immediate Self-Audit:** Compare the resulting diff against the KISS plan (no unintended edits, no unapproved deletions, no new warnings, no `[patch]` regressions in `Cargo.toml`).
5. **Final Ledger Update (`prompts/AUDIT_STATE.md`):**
   - If **>= 1 fix was implemented and verified:** Set `Status` to `🟡 ReCheck` (keep `Clean_Streak` at `0`) so the next independent audit pass verifies the new code. If `AUTONOMOUS` deferred open questions/removals, link the report in `Notes` (e.g., `Open items: prompts/reports/...`).
   - If **0 fixes were implemented** (all deferred): Keep `Status` as `🔴 Fix_Open` and link the report in `Notes`.
```
