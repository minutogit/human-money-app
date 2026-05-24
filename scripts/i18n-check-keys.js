import fs from 'fs';
import path from 'path';

const srcDir = path.resolve('src');
const enPath = path.resolve('src/locales/en.json');

if (!fs.existsSync(enPath)) {
  console.error(`❌ Error: en.json not found at ${enPath}`);
  process.exit(1);
}

const VALID_DOMAINS = [
  'common',
  'auth',
  'profile',
  'dashboard',
  'wallet',
  'voucher',
  'contacts',
  'transfer',
  'history',
  'conflict',
  'integrity',
  'settings'
];

let enContent;
try {
  enContent = fs.readFileSync(enPath, 'utf8');
} catch (err) {
  console.error(`❌ Error reading en.json: ${err.message}`);
  process.exit(1);
}

let enData;
try {
  enData = JSON.parse(enContent);
} catch (err) {
  console.error(`❌ Error parsing en.json: ${err.message}`);
  process.exit(1);
}

const originalKeys = Object.keys(enData);
const sortedKeys = [...originalKeys].sort();
const isSorted = originalKeys.every((key, index) => key === sortedKeys[index]);

let enKeys = new Set(originalKeys);

let sortingFailed = false;
if (!isSorted) {
  console.log(`ℹ️ en.json is not alphabetically sorted. Sorting automatically...`);
  const sortedData = {};
  for (const key of sortedKeys) {
    sortedData[key] = enData[key];
  }
  try {
    fs.writeFileSync(enPath, JSON.stringify(sortedData, null, 2) + '\n', 'utf8');
    console.log(`✅ en.json sorted alphabetically.`);
  } catch (err) {
    console.error(`❌ Error writing sorted en.json: ${err.message}`);
    process.exit(1);
  }
  enKeys = new Set(sortedKeys);
  sortingFailed = true;
}

// Validate all keys in en.json for domain prefixes
let invalidEnKeysCount = 0;
for (const key of enKeys) {
  const dotIndex = key.indexOf('.');
  if (dotIndex === -1) {
    console.error(`❌ Key '${key}' in en.json is missing a domain prefix.`);
    invalidEnKeysCount++;
  } else {
    const domain = key.substring(0, dotIndex);
    if (!VALID_DOMAINS.includes(domain)) {
      console.error(`❌ Key '${key}' in en.json has an invalid domain '${domain}'.`);
      invalidEnKeysCount++;
    }
  }
}

// Recursively find TS/TSX files
function getFiles(dir) {
  let results = [];
  const list = fs.readdirSync(dir);
  for (const file of list) {
    const filePath = path.join(dir, file);
    const stat = fs.statSync(filePath);
    if (stat && stat.isDirectory()) {
      // Exclude directories we don't want to parse
      if (file !== 'locales' && file !== 'test' && file !== 'node_modules') {
        results = results.concat(getFiles(filePath));
      }
    } else if (filePath.endsWith('.ts') || filePath.endsWith('.tsx')) {
      results.push(filePath);
    }
  }
  return results;
}

const files = getFiles(srcDir);
const missingKeys = [];
const invalidKeys = [];

// Matches t('key') or t("key") or t('key.subkey')
const tRegex = /\bt\(\s*(['"`])([a-zA-Z0-9_.-]+)\1/g;

for (const file of files) {
  let content;
  try {
    content = fs.readFileSync(file, 'utf8');
  } catch (err) {
    console.error(`❌ Error reading file ${file}: ${err.message}`);
    continue;
  }

  let match;
  tRegex.lastIndex = 0;
  while ((match = tRegex.exec(content)) !== null) {
    const key = match[2];
    
    // Calculate the correct line number by counting newlines before match.index
    const linesBefore = (content.substring(0, match.index).match(/\n/g) || []).length;
    const lineNo = linesBefore + 1;

    // 1. Check if the key uses a valid domain
    const dotIndex = key.indexOf('.');
    if (dotIndex === -1) {
      invalidKeys.push({
        key,
        file: path.relative(process.cwd(), file),
        line: lineNo,
        reason: 'missing a domain prefix'
      });
    } else {
      const domain = key.substring(0, dotIndex);
      if (!VALID_DOMAINS.includes(domain)) {
        invalidKeys.push({
          key,
          file: path.relative(process.cwd(), file),
          line: lineNo,
          reason: `invalid domain '${domain}'`
        });
      }
    }

    // 2. Check if the key is missing in en.json
    if (!enKeys.has(key)) {
      missingKeys.push({
        key,
        file: path.relative(process.cwd(), file),
        line: lineNo,
      });
    }
  }
}

let hasErrors = false;

if (invalidEnKeysCount > 0) {
  console.error(`❌ Critical Error: Found ${invalidEnKeysCount} key(s) in en.json with invalid domain prefixes.`);
  hasErrors = true;
}

if (invalidKeys.length > 0) {
  console.error('❌ Critical Error: Found invalid i18n keys used in code:');
  for (const item of invalidKeys) {
    console.error(`   - Key '${item.key}' in ${item.file}:${item.line} is ${item.reason}`);
  }
  hasErrors = true;
}

if (missingKeys.length > 0) {
  console.error('❌ Critical Error: Found i18n keys used in code that are missing from en.json:');
  for (const item of missingKeys) {
    console.error(`   - Key '${item.key}' in ${item.file}:${item.line}`);
  }
  hasErrors = true;
}

if (sortingFailed) {
  console.error('❌ Critical Error: en.json was not sorted alphabetically. It has been automatically sorted.');
  hasErrors = true;
}

if (hasErrors) {
  process.exit(1);
}

console.log('✅ All i18n keys used in code are valid and present in en.json.');
process.exit(0);
