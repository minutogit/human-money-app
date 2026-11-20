import fs from 'fs';
import path from 'path';
import ts from 'typescript';

const srcDir = path.resolve('src');
const localesDir = path.resolve('src/locales');
const LOCALES = ['en', 'de', 'es', 'fr', 'it'];
const FIX_MODE = process.argv.includes('--fix');
const enPath = path.join(localesDir, 'en.json');

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
  'settings',
  'standards',
  'error',
  'gender',
  'marketplace',
  'help',
  'helpCenter',
  'statusChip',
  'sidebar',
  'backup'
];

function extractPlaceholders(text) {
  return Array.from(String(text).matchAll(/\{\{([^}]+)\}\}/g), (m) => m[1].trim()).sort().join(',');
}

const localeDataByLang = {};
let sortingFailed = false;
let invalidDomainKeysCount = 0;
const parityIssues = [];
const placeholderIssues = [];

for (const lang of LOCALES) {
  const filePath = path.join(localesDir, `${lang}.json`);
  if (!fs.existsSync(filePath)) {
    console.error(`❌ Error: ${lang}.json not found at ${filePath}`);
    process.exit(1);
  }

  let data;
  try {
    data = JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (err) {
    console.error(`❌ Error reading/parsing ${lang}.json: ${err.message}`);
    process.exit(1);
  }

  const originalKeys = Object.keys(data);
  const sortedKeys = [...originalKeys].sort();
  const isSorted = originalKeys.every((key, index) => key === sortedKeys[index]);

  if (!isSorted) {
    if (FIX_MODE) {
      console.log(`ℹ️ ${lang}.json is not alphabetically sorted. Sorting (--fix)...`);
      const sortedData = {};
      for (const key of sortedKeys) {
        sortedData[key] = data[key];
      }
      fs.writeFileSync(filePath, JSON.stringify(sortedData, null, 2) + '\n', 'utf8');
      data = sortedData;
    } else {
      console.error(`❌ Error: ${lang}.json is not alphabetically sorted (run with --fix to sort).`);
      sortingFailed = true;
    }
  }

  for (const key of Object.keys(data)) {
    const dotIndex = key.indexOf('.');
    if (dotIndex === -1) {
      console.error(`❌ Key '${key}' in ${lang}.json is missing a domain prefix.`);
      invalidDomainKeysCount++;
    } else {
      const domain = key.substring(0, dotIndex);
      if (!VALID_DOMAINS.includes(domain)) {
        console.error(`❌ Key '${key}' in ${lang}.json has an invalid domain '${domain}'.`);
        invalidDomainKeysCount++;
      }
    }
  }

  localeDataByLang[lang] = data;
}

const enData = localeDataByLang.en;
const enKeys = new Set(Object.keys(enData));

for (const lang of LOCALES.filter((l) => l !== 'en')) {
  const data = localeDataByLang[lang];
  const langKeys = new Set(Object.keys(data));
  const missing = [...enKeys].filter((k) => !langKeys.has(k));
  const extra = [...langKeys].filter((k) => !enKeys.has(k));
  if (missing.length > 0 || extra.length > 0) {
    parityIssues.push({ lang, missing, extra });
  }
  for (const key of enKeys) {
    if (!(key in data)) continue;
    const expectedVars = extractPlaceholders(enData[key]);
    const actualVars = extractPlaceholders(data[key]);
    if (expectedVars !== actualVars) {
      placeholderIssues.push(`[${lang}] ${key}: expected {{${expectedVars}}}, got {{${actualVars}}}`);
    }
  }
}

const DYNAMIC_USED_PREFIXES = [
  'profile.field.',
  'voucher.role.',
  'voucher.txType.',
  'voucher.signRequest.anchorIssue.',
];

const DYNAMIC_USED_EXACT = new Set([
  'profile.gender',
  'profile.coordinates',
  'profile.firstName',
  'profile.lastName',
  'profile.email',
  'profile.phone',
  'profile.organization',
  'profile.community',
  'profile.url',
]);

function isAllowlistedDynamicKey(key) {
  return (
    DYNAMIC_USED_EXACT.has(key) ||
    DYNAMIC_USED_PREFIXES.some((prefix) => key.startsWith(prefix))
  );
}

function getSourceFiles(dir, { includeTests = true, extensions = ['.ts', '.tsx'] } = {}) {
  let results = [];
  if (!fs.existsSync(dir)) return results;
  for (const entry of fs.readdirSync(dir)) {
    const fullPath = path.join(dir, entry);
    const stat = fs.statSync(fullPath);
    if (stat.isDirectory()) {
      if (
        entry === 'locales' ||
        entry === 'test' ||
        entry === 'node_modules' ||
        (!includeTests && entry === '__tests__')
      ) {
        continue;
      }
      results = results.concat(getSourceFiles(fullPath, { includeTests, extensions }));
    } else if (extensions.some((ext) => fullPath.endsWith(ext))) {
      if (!includeTests && /\.test\.(ts|tsx)$/.test(entry)) {
        continue;
      }
      results.push(fullPath);
    }
  }
  return results;
}

const files = getSourceFiles(srcDir, { includeTests: true, extensions: ['.ts', '.tsx'] });
const prodTsFiles = new Set(
  getSourceFiles(srcDir, { includeTests: false, extensions: ['.ts', '.tsx'] }),
);
const missingKeys = [];
const invalidKeys = [];
const callSitePlaceholderIssues = [];
const usedKeys = new Set();

const requiredVarsByKey = new Map();
for (const [k, v] of Object.entries(enData)) {
  const vars = [
    ...new Set(Array.from(String(v).matchAll(/\{\{([^}]+)\}\}/g), (m) => m[1].trim())),
  ];
  if (vars.length > 0) {
    requiredVarsByKey.set(k, vars);
  }
}

// Matches t('key') or t("key") or t(`key`)
const tRegex = /\bt\(\s*(['"`])([a-zA-Z0-9_.-]+)\1/g;
const anyStringLiteralRegex = /(['"`])([a-zA-Z0-9_.-]+)\1/g;

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

    const linesBefore = (content.substring(0, match.index).match(/\n/g) || []).length;
    const lineNo = linesBefore + 1;

    const dotIndex = key.indexOf('.');
    if (dotIndex === -1) {
      invalidKeys.push({
        key,
        file: path.relative(process.cwd(), file),
        line: lineNo,
        reason: 'missing a domain prefix',
      });
    } else {
      const domain = key.substring(0, dotIndex);
      if (!VALID_DOMAINS.includes(domain)) {
        invalidKeys.push({
          key,
          file: path.relative(process.cwd(), file),
          line: lineNo,
          reason: `invalid domain '${domain}'`,
        });
      }
    }

    if (!enKeys.has(key)) {
      missingKeys.push({
        key,
        file: path.relative(process.cwd(), file),
        line: lineNo,
      });
    }
  }

  if (prodTsFiles.has(file)) {
    anyStringLiteralRegex.lastIndex = 0;
    while ((match = anyStringLiteralRegex.exec(content)) !== null) {
      const literal = match[2];
      if (enKeys.has(literal)) {
        usedKeys.add(literal);
      }
    }

    const sf = ts.createSourceFile(file, content, ts.ScriptTarget.Latest, true);
    const visitAst = (node) => {
      if (
        ts.isCallExpression(node) &&
        ts.isIdentifier(node.expression) &&
        node.expression.text === 't' &&
        node.arguments.length >= 1
      ) {
        const a0 = node.arguments[0];
        if (ts.isStringLiteral(a0) || ts.isNoSubstitutionTemplateLiteral(a0)) {
          const key = a0.text;
          const req = requiredVarsByKey.get(key);
          if (req) {
            const optArg =
              node.arguments.length >= 3
                ? node.arguments[2]
                : node.arguments.length === 2 &&
                    !ts.isStringLiteral(node.arguments[1]) &&
                    !ts.isNoSubstitutionTemplateLiteral(node.arguments[1])
                  ? node.arguments[1]
                  : undefined;
            const line = sf.getLineAndCharacterOfPosition(node.getStart()).line + 1;
            const relFile = path.relative(process.cwd(), file);
            if (!optArg) {
              callSitePlaceholderIssues.push(
                `${relFile}:${line} t('${key}') missing options for {{${req.join('}}, {{')}}}`,
              );
            } else if (ts.isObjectLiteralExpression(optArg)) {
              const hasSpread = optArg.properties.some((p) => ts.isSpreadAssignment(p));
              if (!hasSpread) {
                const provided = new Set();
                for (const p of optArg.properties) {
                  if (
                    (ts.isPropertyAssignment(p) || ts.isShorthandPropertyAssignment(p)) &&
                    (ts.isIdentifier(p.name) || ts.isStringLiteral(p.name))
                  ) {
                    provided.add(p.name.text);
                  }
                }
                const missing = req.filter((r) => !provided.has(r));
                if (missing.length > 0) {
                  callSitePlaceholderIssues.push(
                    `${relFile}:${line} t('${key}') missing {{${missing.join('}}, {{')}}}`,
                  );
                }
              }
            }
          }
        }
      }
      ts.forEachChild(node, visitAst);
    };
    visitAst(sf);
  }
}

// --- Cross-language scan of Rust error mappings across all src-tauri/src/**/*.rs ---
const rustSrcDir = path.resolve('src-tauri/src');
const rustFiles = getSourceFiles(rustSrcDir, { includeTests: true, extensions: ['.rs'] });
const rustKeyRegex = /"(error\.[a-zA-Z0-9_.-]+)"/g;
for (const rustFile of rustFiles) {
  const rustContent = fs.readFileSync(rustFile, 'utf8');
  let rustMatch;
  rustKeyRegex.lastIndex = 0;
  while ((rustMatch = rustKeyRegex.exec(rustContent)) !== null) {
    const key = rustMatch[1];
    usedKeys.add(key);
    if (!enKeys.has(key)) {
      missingKeys.push({
        key,
        file: path.relative(process.cwd(), rustFile),
        line: (rustContent.substring(0, rustMatch.index).match(/\n/g) || []).length + 1,
      });
    }
  }
}

const unusedKeys = [...enKeys].filter(
  (key) => !usedKeys.has(key) && !isAllowlistedDynamicKey(key),
);

let hasErrors = false;

if (invalidDomainKeysCount > 0) {
  console.error(`❌ Critical Error: Found ${invalidDomainKeysCount} key(s) with invalid domain prefixes.`);
  hasErrors = true;
}

if (parityIssues.length > 0) {
  console.error('❌ Critical Error: Locale key parity mismatches detected against en.json:');
  for (const issue of parityIssues) {
    if (issue.missing.length > 0) {
      console.error(`   - [${issue.lang}] Missing ${issue.missing.length} key(s): ${issue.missing.slice(0, 10).join(', ')}${issue.missing.length > 10 ? ' ...' : ''}`);
    }
    if (issue.extra.length > 0) {
      console.error(`   - [${issue.lang}] Extra ${issue.extra.length} key(s): ${issue.extra.slice(0, 10).join(', ')}${issue.extra.length > 10 ? ' ...' : ''}`);
    }
  }
  hasErrors = true;
}

if (placeholderIssues.length > 0) {
  console.error('❌ Critical Error: Placeholder mismatches detected across locales:');
  for (const msg of placeholderIssues.slice(0, 20)) {
    console.error(`   - ${msg}`);
  }
  hasErrors = true;
}

if (callSitePlaceholderIssues.length > 0) {
  console.error('❌ Critical Error: Missing i18n placeholder arguments at call sites:');
  for (const msg of callSitePlaceholderIssues.slice(0, 20)) {
    console.error(`   - ${msg}`);
  }
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

if (unusedKeys.length > 0) {
  console.error(`❌ Critical Error: Found ${unusedKeys.length} unused i18n key(s) in en.json:`);
  for (const key of unusedKeys) {
    console.error(`   - ${key}`);
  }
  hasErrors = true;
}

if (sortingFailed) {
  console.error('❌ Critical Error: One or more locale files are not alphabetically sorted.');
  hasErrors = true;
}

if (hasErrors) {
  process.exit(1);
}

console.log(`✅ All i18n keys across ${LOCALES.length} locales (${enKeys.size} keys each) are sorted, complete, and valid.`);
process.exit(0);
