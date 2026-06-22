import fs from 'fs';
import path from 'path';

// Usage: node scripts/i18n-merge.js <lang>
const args = process.argv.slice(2);
const langArgIndex = args.findIndex(arg => !arg.startsWith('--'));

if (langArgIndex === -1) {
  console.error('❌ Error: Language code is required. Usage: npm run i18n:merge -- <lang>');
  process.exit(1);
}

const targetLang = args[langArgIndex];

const localesDir = path.resolve('src/locales');
const enPath = path.join(localesDir, 'en.json');
const targetLangPath = path.join(localesDir, `${targetLang}.json`);
const lockPath = path.join(localesDir, 'i18n-lock.json');
const taskPath = path.resolve('i18n-task.json');
const resultPath = path.resolve('i18n-result.json');

// Helper to read JSON safely
function readJson(filePath, required = false) {
  if (!fs.existsSync(filePath)) {
    if (required) {
      console.error(`❌ Error: Required file '${filePath}' is missing.`);
      process.exit(1);
    }
    return null;
  }
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (err) {
    console.error(`❌ Error parsing JSON from '${filePath}':`, err.message);
    process.exit(1);
  }
}

// 2. Load i18n-result.json
if (!fs.existsSync(resultPath)) {
  console.error('❌ Error: i18n-result.json not found. Run the AI agent first.');
  process.exit(1);
}

let rawResult = fs.readFileSync(resultPath, 'utf8');

// 3. ROBUST JSON PARSING: Markdown-Stripping
const trimmedResult = rawResult.trim();
const markdownMatch = trimmedResult.match(/^```json\s*([\s\S]*?)\s*```$/) || trimmedResult.match(/^```\s*([\s\S]*?)\s*```$/);
let jsonString = trimmedResult;
if (markdownMatch) {
  jsonString = markdownMatch[1].trim();
  console.warn('⚠️ Warning: Stripped Markdown wrapper from i18n-result.json.');
}

let result;
try {
  result = JSON.parse(jsonString);
} catch (err) {
  console.error('❌ Error: i18n-result.json contains invalid JSON.', err.message);
  process.exit(1);
}

// 4. Load i18n-task.json
const task = readJson(taskPath, false);
if (!task) {
  console.error('❌ Error: i18n-task.json not found. Merge without prep is not supported.');
  process.exit(1);
}

// 5. Load en.json and i18n-lock.json
const en = readJson(enPath, true);
const lock = readJson(lockPath, true);
const targetJson = readJson(targetLangPath, false) || {};

// Helpers for validation
function arraysEqual(a, b) {
  if (a.length !== b.length) return false;
  const sortedA = [...a].sort();
  const sortedB = [...b].sort();
  for (let i = 0; i < sortedA.length; i++) {
    if (sortedA[i] !== sortedB[i]) return false;
  }
  return true;
}

// Abort function
function abort(message) {
  if (message) {
    console.error(message);
  }
  console.error(`⚠️ Process aborted. ${targetLang}.json and i18n-lock.json were NOT modified.`);
  process.exit(1);
}

// 6. VALIDATION PHASE
console.log('🔍 Validating i18n-result.json...');

const resultKeys = Object.keys(result);

// 6.1 Hallucinations protection
for (const key of resultKeys) {
  if (!(key in en)) {
    abort(`❌ Critical Error: AI hallucinated non-existent key '${key}' in i18n-result.json. Merge aborted.`);
  }
}

// 6.2 Value-type validation
for (const key of resultKeys) {
  if (typeof result[key] !== 'string') {
    abort(`❌ Critical Error: Value for key '${key}' is not a string. Merge aborted.`);
  }
}

// 6.3 Interpolation Guard ({{placeholder}}-Schutz)
for (const key of resultKeys) {
  const sourceText = lock[key]?.source_text || en[key];
  const translationText = result[key];

  const sourcePlaceholders = sourceText.match(/\{\{[^}]+\}\}/g) || [];
  const translationPlaceholders = translationText.match(/\{\{[^}]+\}\}/g) || [];

  if (!arraysEqual(sourcePlaceholders, translationPlaceholders)) {
    abort(
      `❌ Critical Error: Variable placeholder mismatch in key '${key}'.\n` +
      `   -> Expected: [${sourcePlaceholders.join(', ')}]\n` +
      `   -> Got:      [${translationPlaceholders.join(', ')}]\n` +
      `   Please fix and re-translate.`
    );
  }
}
console.log('✅ Interpolation variables verified.');

// 6.4 HTML Tag Guard
for (const key of resultKeys) {
  const sourceText = lock[key]?.source_text || en[key];
  const translationText = result[key];

  const sourceTags = sourceText.match(/<[^>]+>/g) || [];
  const translationTags = translationText.match(/<[^>]+>/g) || [];

  if (!arraysEqual(sourceTags, translationTags)) {
    abort(
      `❌ Critical Error: HTML tag count or format mismatch in key '${key}'.\n` +
      `   -> Source has [${sourceTags.length}] tags: [${sourceTags.join(', ')}]\n` +
      `   -> Translation has [${translationTags.length}] tags: [${translationTags.join(', ')}]\n` +
      `   Please fix and re-translate.`
    );
  }
}
console.log('✅ HTML tags verified.');

// 6.5 Key completeness check
const taskKeys = Object.keys(task.keys_to_translate || {});
const missingKeys = taskKeys.filter(key => !(key in result));
if (missingKeys.length > 0) {
  console.warn(`⚠️ Warning: ${missingKeys.length} keys from the task were not translated: [${missingKeys.join(', ')}]`);
  console.warn(`   These remain as 'needs-translation'.`);
}
console.log('✅ Key consistency verified.');

// Helper to save JSON sorted alphabetically
function saveJsonAlphabetically(filePath, obj) {
  const sorted = {};
  const keys = Object.keys(obj).sort();
  for (const key of keys) {
    sorted[key] = obj[key];
  }
  fs.writeFileSync(filePath, JSON.stringify(sorted, null, 2) + '\n', 'utf8');
}

// 7. MERGE (only if we reached here)
const mergedCount = resultKeys.length;
for (const key of resultKeys) {
  targetJson[key] = result[key];
}
saveJsonAlphabetically(targetLangPath, targetJson);

// 8. LOCK-FILE UPDATE
for (const key of resultKeys) {
  if (!lock[key]) {
    lock[key] = {};
  }
  lock[key].source_text = en[key];
  if (!lock[key].status) {
    lock[key].status = {};
  }
  lock[key].status[targetLang] = "ai-translated";
}
saveJsonAlphabetically(lockPath, lock);

// 9. CLEANUP
try {
  if (fs.existsSync(taskPath)) fs.unlinkSync(taskPath);
  if (fs.existsSync(resultPath)) fs.unlinkSync(resultPath);
} catch (err) {
  console.warn(`⚠️ Warning: Could not clean up temporary task/result files:`, err.message);
}

// 10. SUCCESS SUMMARY
console.log(`✨ Merged ${mergedCount} translations into ${targetLang}.json.`);
console.log(`🔒 i18n-lock.json updated with status 'ai-translated'.`);
console.log(`🧹 Temporary task files removed.`);
