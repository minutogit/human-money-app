---
name: i18n-workflow
description: Instructions and guidelines for the i18n localization workflow including preparation, validation rules, lockfile state management, and merging of translation results.
---

# Human Money App — i18n Translation Workflow

This document describes the technical localization (i18n) infrastructure of the app and how the translation process is executed, validated, and managed using the integrated Node utility scripts.

---

## 1. Directory Structure & File Roles

```
src/
└── locales/
    ├── en.json              ← Source of Truth (English source strings)
    ├── de.json              ← Target language (e.g. German, alphabetized)
    └── i18n-lock.json       ← State-Registry (translation status & change detection)

scripts/
├── i18n-prep.js             ← Pre-translation manager + linter (npm run i18n:prep)
├── i18n-merge.js            ← Post-translation merger + gatekeeper (npm run i18n:merge)
└── i18n-check-keys.js       ← Linter for checking key validity and presence (npm run i18n:check)

# Temporary workfiles in the project root (ignored via .gitignore):
i18n-task.json               ← Created by the prep script for the translation agent
i18n-result.json             ← Provided by the translation agent as the result
```

---

## 2. Domain-Based Key Prefixing Schema

To prevent key collisions and ensure key searchability (e.g., using `grep`), every localization key must start with one of the following 12 functional domains followed by a dot (`.`):

### The 12 Standard Domains
1.  `common` — Shared UI text (e.g., "Save", "Cancel", "Error", generic actions)
2.  `auth` — Login, profile unlock, session timeout warnings
3.  `profile` — Profile settings, mnemonic/seed words, backup, restoration
4.  `dashboard` — Welcome screens, widget cards, dashboard status
5.  `wallet` — Balances, addresses, security seals, sync indicators
6.  `voucher` — Basics form, services form, signatures, verification rules, detail view
7.  `contacts` — Address book, trust levels, add/edit contacts
8.  `transfer` — Send/receive money, inputs, QR codes, success/error overlays
9.  `history` — Transaction list, activities log, filter/search views
10. `conflict` — Quarantine list, double-spend witnesses/victims, local overrides
11. `integrity` — Database health reports, app version checks, warning indicators
12. `settings` — App configuration, theme, language, network setup

### Rules & Linter Checks (`npm run i18n:check`)
- **Valid Keys:** Must start with a standard domain, followed by a dot, followed by the specific key identifier (e.g. `voucher.title`, `common.cancel`).
- **Invalid Keys:** Keys without a prefix (e.g. `title`) or with an unknown prefix (e.g. `invalid.title`) will trigger a critical linter failure.
- **Auto-Sorting of `en.json`:** To maintain clean git diffs, `en.json` is validated to be alphabetically sorted. If not, the linter automatically sorts it, rewrites the file, and exits with code `1` instructing the developer to commit the changes.

---

## 3. Status Management (`i18n-lock.json`)

The `i18n-lock.json` file controls which keys need translation. It stores the `source_text` (for change detection) and the translation status for each target language.

### Status Values
*   `needs-translation`: The key is new or the English source text has changed. **Must be translated.**
*   `ai-translated`: Translated by AI, but not yet manually reviewed.
*   `pending-review`: Marked for a later manual human review.
*   `human-reviewed`: Reviewed and approved by a human translator.

> [!IMPORTANT]
> If the source text in `en.json` changes, the prep script automatically resets the status for that target language back to `needs-translation` (even if it was marked as `human-reviewed`).

---

## 4. Translation Hints (`context_hint`)

Sometimes the context of a term is ambiguous (e.g., whether "Minuto" should be translated or if a variable represents a name or a number). For this, there is an integrated hint system:

1. **Adding Hints in the Lockfile (`src/locales/i18n-lock.json`):**
   Developers can manually add an `"ai_hint"` field to a key's entry:
   ```json
   "voucher.title": {
     "source_text": "Minuto Voucher",
     "ai_hint": "Do not translate 'Minuto'. It is a brand name.",
     "status": {
       "de": "needs-translation"
     }
   }
   ```

2. **Passing Hints to the Task:**
   When running the prep script (`npm run i18n:prep`), this hint is automatically read and placed as `"context_hint"` in `i18n-task.json`.

3. **Adherence by the Translation Agent:**
   **Every translation agent MUST inspect this field** and strictly follow its instructions when translating the string.

---

## 5. The Translation Workflow (Step-by-Step)

### Step 1: Manage Source Strings
Developers add or modify translation keys directly in `src/locales/en.json`.

### Step 2: Generate the Task Queue
Run the prep script for the target language (e.g., German):
```bash
npm run i18n:prep -- de
```
*   **What happens?**
    1. Pruning of obsolete keys from `de.json` and `i18n-lock.json`.
    2. Change detection (comparison with `en.json`).
    3. Creation of `i18n-task.json` in the project root.
*   *Tip for checking status (without modifying files):*
    ```bash
    npm run i18n:status -- de
    ```

### Step 3: Perform Translation (AI Agent)
The translation agent:
1. Reads the generated `i18n-task.json`.
2. Translates the texts into the target language (respecting the `context_hint` in the task).
3. Creates the `i18n-result.json` file as a flat JSON object in the project root:
   ```json
   {
     "voucher.title": "Minuto-Gutschein"
   }
   ```

### Step 4: Merge & Validate Results
Run the merge script:
```bash
npm run i18n:merge -- de
```
*   **What happens?**
    The script reads `i18n-result.json` and performs strict validations. Only if **all** checks pass are the changes written to `de.json` and `i18n-lock.json`.

---

## 6. Validation Rules (The Gatekeeper)

If any of these checks fail, the merge is aborted:

1.  **Hallucination Guard (Strict Key Validation):**
    Every key in `i18n-result.json` must exist in `en.json`.
2.  **Value Type Check:**
    All values in `i18n-result.json` must be strings (no arrays, objects, or `null`).
3.  **Interpolation Guard (`{{placeholder}}`):**
    Placeholders in the translated text must match those in the source text exactly (same count and exact name, e.g., `{{amount}}`).
4.  **HTML Tag Guard:**
    HTML tags (e.g., `<strong>`, `</strong>`) in the translated text must match those in the source text exactly (same count and exact tag names).
5.  **Key Completeness:**
    If keys from `i18n-task.json` are missing in the translation, the script prints a warning (merge succeeds, but the missing keys remain as `needs-translation`).

---

## 7. react-i18next Frontend Integration

Localization is initialized via `src/i18n.ts` and loaded in `src/main.tsx`.

```typescript
// src/i18n.ts
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from './locales/en.json';
import de from './locales/de.json';

i18n
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: en },
      de: { translation: de },
    },
    lng: 'de',
    fallbackLng: 'en', // Fallback language if a key is missing in the target language
    keySeparator: false, // Prevents i18next from parsing dots as nested objects
    interpolation: {
      escapeValue: false, // React already escapes values
    },
  });

export default i18n;
```
