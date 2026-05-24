import fs from 'fs';
import path from 'path';

// Usage: node scripts/i18n-prep.js <lang> [--dry-run]
const args = process.argv.slice(2);
const langArgIndex = args.findIndex(arg => !arg.startsWith('--'));
const isDryRun = args.includes('--dry-run');

if (langArgIndex === -1) {
  console.error('❌ Error: Language code is required. Usage: npm run i18n:prep -- <lang> [--dry-run]');
  process.exit(1);
}

const targetLang = args[langArgIndex];

const localesDir = path.resolve('src/locales');
const enPath = path.join(localesDir, 'en.json');
const targetLangPath = path.join(localesDir, `${targetLang}.json`);
const lockPath = path.join(localesDir, 'i18n-lock.json');
const taskPath = path.resolve('i18n-task.json');

// Helper to read JSON safely
function readJson(filePath, required = false) {
  if (!fs.existsSync(filePath)) {
    if (required) {
      console.error(`❌ Error: Required file '${filePath}' is missing.`);
      process.exit(1);
    }
    return {};
  }
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (err) {
    console.error(`❌ Error parsing JSON from '${filePath}':`, err.message);
    process.exit(1);
  }
}

// 2. Load files
const en = readJson(enPath, true);
const targetJson = readJson(targetLangPath, false);
const lock = readJson(lockPath, false);

// 3. PRE-FLIGHT: Empty Source Detection
for (const [key, value] of Object.entries(en)) {
  if (value === "") {
    console.error(`❌ Error: Key '${key}' has an empty source string in en.json. Translation aborted.`);
    process.exit(1);
  }
}

// Keep track of modifications to lock and targetJson
let lockModified = false;
let targetModified = false;

// 4. PRUNING: Obsolete keys and orphaned hints
for (const key of Object.keys(targetJson)) {
  if (!(key in en)) {
    delete targetJson[key];
    targetModified = true;
    
    // We will delete it from lock next, so we can clean it up here
    if (key in lock) {
      const entry = lock[key];
      if (entry && entry.ai_hint) {
        console.log(`🧹 Cleanup: Context hint for deleted key '${key}' removed from lockfile.`);
      }
      delete lock[key];
      lockModified = true;
    }
    console.log(`🧹 Cleanup: Removed obsolete key '${key}' from ${targetLang}.json and lockfile.`);
  }
}

for (const key of Object.keys(lock)) {
  if (!(key in en)) {
    const entry = lock[key];
    if (entry && entry.ai_hint) {
      console.log(`🧹 Cleanup: Context hint for deleted key '${key}' removed from lockfile.`);
    }
    delete lock[key];
    lockModified = true;
    console.log(`🧹 Cleanup: Removed obsolete key '${key}' from lockfile.`);
  }
}

// 5. CHANGE DETECTION: Invalidating changed source texts
for (const [key, value] of Object.entries(en)) {
  if (!(key in lock)) {
    // If it exists in targetJson, assume it is translated, otherwise needs-translation
    const initialStatus = (key in targetJson) ? "ai-translated" : "needs-translation";
    lock[key] = {
      source_text: value,
      status: {
        [targetLang]: initialStatus
      }
    };
    lockModified = true;
    console.log(`🆕 New key '${key}' added to lockfile.`);
  } else {
    const entry = lock[key];
    if (!entry.status) {
      entry.status = {};
    }
    if (entry.source_text !== value) {
      entry.source_text = value;
      entry.status[targetLang] = "needs-translation";
      lockModified = true;
      console.log(`🔄 Source changed for '${key}'. Status reset to needs-translation.`);
    } else if (!entry.status[targetLang]) {
      // If it exists in lock, source is unchanged, but status for targetLang is missing
      entry.status[targetLang] = (key in targetJson) ? "ai-translated" : "needs-translation";
      lockModified = true;
    }
  }
}

// 6. TASK-QUEUE: Build the tasks to translate
const keysToTranslate = {};
for (const [key, value] of Object.entries(en)) {
  const entry = lock[key];
  const status = entry?.status?.[targetLang];
  const existsInTarget = key in targetJson;

  if (status === "needs-translation" || !existsInTarget) {
    if (!entry.status) {
      entry.status = {};
    }
    if (entry.status[targetLang] !== "needs-translation") {
      entry.status[targetLang] = "needs-translation";
      lockModified = true;
    }
    keysToTranslate[key] = {
      text: value,
      context_hint: entry.ai_hint || null
    };
  }
}

const keysToTranslateCount = Object.keys(keysToTranslate).length;

// Helper to save JSON sorted alphabetically
function saveJsonAlphabetically(filePath, obj) {
  const sorted = {};
  const keys = Object.keys(obj).sort();
  for (const key of keys) {
    sorted[key] = obj[key];
  }
  fs.writeFileSync(filePath, JSON.stringify(sorted, null, 2) + '\n', 'utf8');
}

// 7. EDGE CASE: Empty task queue
if (keysToTranslateCount === 0) {
  console.log(`✅ Nothing to translate for '${targetLang}'. All keys are up to date.`);
  if (!isDryRun) {
    if (lockModified) {
      saveJsonAlphabetically(lockPath, lock);
    }
    if (targetModified) {
      saveJsonAlphabetically(targetLangPath, targetJson);
    }
  }
  process.exit(0);
}

// 8. Output / Write files
if (isDryRun) {
  console.log(`📋 Dry run: ${keysToTranslateCount} keys would be queued for translation.`);
  console.log('Keys to translate:');
  for (const key of Object.keys(keysToTranslate)) {
    console.log(`  - ${key}`);
  }
} else {
  // Write i18n-task.json
  const taskContent = {
    target_language: targetLang,
    keys_to_translate: keysToTranslate
  };
  fs.writeFileSync(taskPath, JSON.stringify(taskContent, null, 2) + '\n', 'utf8');
  console.log(`📝 Task file written: i18n-task.json (${keysToTranslateCount} keys to translate)`);
  console.log(`Hand this file to the AI agent. Run 'npm run i18n:merge -- ${targetLang}' after.`);

  // Write files
  saveJsonAlphabetically(lockPath, lock);
  if (targetModified || !fs.existsSync(targetLangPath)) {
    saveJsonAlphabetically(targetLangPath, targetJson);
  }
}
