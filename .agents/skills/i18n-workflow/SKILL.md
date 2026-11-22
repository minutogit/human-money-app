---
name: i18n-workflow
description: Authoritative guide for the 5-locale i18n workflow, 21 domain prefixes, AST/Rust validation scripts, and UX glossary enforcement in human-money-app.
---

# Human Money App — i18n Translation Workflow

Localization rules and tooling for the 5 supported UI locales (`en`, `de`, `es`, `fr`, `it`).

## 1. Directory Layout & Scripts

```text
src/
├── i18n.ts                  ← Loads en/de/es/fr/it, localStorage('app_language') + navigator fallback
└── locales/
    ├── en.json              ← Source of truth (alphabetically sorted)
    ├── de.json / es.json / fr.json / it.json
    └── i18n-lock.json       ← Translation state & ai_hint metadata

scripts/
├── i18n-check-keys.js       ← Full parity, domain, AST placeholder & Rust scan (npm run i18n:check)
├── i18n-prep.js             ← Generate i18n-task.json for target locale (npm run i18n:prep -- <lang>)
└── i18n-merge.js            ← Validate & merge i18n-result.json (npm run i18n:merge -- <lang>)
```

## 2. 21 Valid Domain Prefixes (`scripts/i18n-check-keys.js`)

Every key MUST start with `<domain>.` using one of these exact 21 `VALID_DOMAINS`:

`common`, `auth`, `profile`, `dashboard`, `wallet`, `voucher`, `contacts`, `transfer`, `history`, `conflict`, `integrity`, `settings`, `standards`, `error`, `gender`, `marketplace`, `help`, `helpCenter`, `statusChip`, `sidebar`, `backup`.

## 3. Linter & Quality Gates (`npm run i18n:check`)

- **5-Locale Parity & Sorting:** All 5 JSON files (`en`, `de`, `es`, `fr`, `it`) must exist, match `en.json` keys 1:1, match `{{placeholder}}` variables across locales, and be alphabetically sorted (`npm run i18n:check -- --fix` sorts files in place).
- **TS AST Call-Site Check:** Parses `src/**/*.ts(x)` via TypeScript AST to ensure every `t('key', { ... })` provides all required `{{var}}` arguments.
- **Cross-Language Rust Scan:** Scans `src-tauri/src/**/*.rs` for `"error.<...>"` string literals and verifies every emitted backend error key exists in `en.json`.
- **Hard Unused-Key Error:** Any key in `en.json` not referenced in production TypeScript, Rust `"error.*"`, or `DYNAMIC_USED_{EXACT,PREFIXES}` fails the check with exit code `1`.
- **Mandatory Terminology Guardrail:** All user-facing copy MUST follow [`docs/UX_GLOSSARY.md`](../../../docs/UX_GLOSSARY.md) (no `"Wallet"`, `"DID"`, `"Hops"`, or `"bürgen"/"haften"` in WoT strings), enforced automatically by `glossaryLint.test.ts` and `uiModeLint.test.ts`.

## 4. Translation Workflow (`prep` -> `translate` -> `merge`)

1. Add or update English strings in `src/locales/en.json` (add `"ai_hint"` in `src/locales/i18n-lock.json` for ambiguous terms).
2. Generate tasks per target locale (`de`, `es`, `fr`, `it`):
   ```bash
   npm run i18n:prep -- <lang>    # or npm run i18n:status -- <lang>
   ```
3. Translate `i18n-task.json` into `i18n-result.json` (`{"key": "translated string"}`), preserving exact `{{placeholder}}` names, HTML tags, and `UX_GLOSSARY.md` terms.
4. Merge and validate all 5 locales:
   ```bash
   npm run i18n:merge -- <lang>
   npm run i18n:check -- --fix
   npx vitest run src/__tests__/glossaryLint.test.ts src/__tests__/uiModeLint.test.ts
   ```
