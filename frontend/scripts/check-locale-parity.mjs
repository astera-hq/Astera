// Fails CI when a locale's key set drifts from en, so a PR that adds/removes
// keys in en without updating the other locales gets caught immediately
// instead of the gap silently growing (#1389).
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const localesDir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'locales');
const baseLocale = 'en';

function flattenKeys(node, prefix = '') {
  const keys = new Set();
  for (const [key, value] of Object.entries(node)) {
    const fullKey = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      for (const nested of flattenKeys(value, fullKey)) keys.add(nested);
    } else {
      keys.add(fullKey);
    }
  }
  return keys;
}

function loadKeys(locale) {
  const file = path.join(localesDir, locale, 'common.json');
  const data = JSON.parse(readFileSync(file, 'utf8'));
  return flattenKeys(data);
}

const locales = readdirSync(localesDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && entry.name !== baseLocale)
  .map((entry) => entry.name);

const baseKeys = loadKeys(baseLocale);
let hasMismatch = false;

for (const locale of locales) {
  const keys = loadKeys(locale);
  const missing = [...baseKeys].filter((key) => !keys.has(key)).sort();
  const extra = [...keys].filter((key) => !baseKeys.has(key)).sort();

  if (missing.length > 0 || extra.length > 0) {
    hasMismatch = true;
    console.error(`\nlocales/${locale}/common.json is out of sync with ${baseLocale}:`);
    if (missing.length > 0) {
      console.error(`  missing ${missing.length} key(s):`);
      for (const key of missing) console.error(`    - ${key}`);
    }
    if (extra.length > 0) {
      console.error(`  ${extra.length} extra key(s) not in ${baseLocale}:`);
      for (const key of extra) console.error(`    - ${key}`);
    }
  }
}

if (hasMismatch) {
  console.error('\nRun this check locally after editing any locales/*/common.json file.');
  process.exit(1);
}

console.log(`All locales (${locales.join(', ')}) match ${baseLocale}'s key set.`);
