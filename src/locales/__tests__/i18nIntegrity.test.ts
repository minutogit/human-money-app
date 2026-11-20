import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, it, expect } from 'vitest';
import deJson from '../de.json';
import enJson from '../en.json';
import frJson from '../fr.json';
import esJson from '../es.json';
import itJson from '../it.json';

function extractPlaceholders(text: string): string[] {
    return Array.from(text.matchAll(/\{\{([^}]+)\}\}/g), (m) => m[1].trim()).sort();
}

describe('i18n Key & Placeholder Integrity', () => {
    const de = deJson as Record<string, string>;
    const en = enJson as Record<string, string>;
    const fr = frJson as Record<string, string>;
    const es = esJson as Record<string, string>;
    const itLocale = itJson as Record<string, string>;

    it('keeps 100% key parity and alphabetical key order across all 5 locales', () => {
        const allLocales: Array<[string, Record<string, string>]> = [
            ['en', en],
            ['de', de],
            ['fr', fr],
            ['es', es],
            ['it', itLocale],
        ];
        const enKeys = Object.keys(en);

        for (const [lang, obj] of allLocales) {
            const keys = Object.keys(obj);
            expect(keys, `${lang}.json keys must be alphabetically sorted`).toEqual([...keys].sort());

            const missing = enKeys.filter((k) => !(k in obj));
            const extra = keys.filter((k) => !(k in en));
            expect(missing, `${lang}.json missing keys vs en.json`).toEqual([]);
            expect(extra, `${lang}.json extra keys vs en.json`).toEqual([]);
        }
    });

    it('preserves identical {{placeholder}} variables across all 5 locales for shared keys', () => {
        const secondaryLocales: Array<[string, Record<string, string>]> = [
            ['de', de],
            ['fr', fr],
            ['es', es],
            ['it', itLocale],
        ];
        const mismatches: string[] = [];

        for (const [key, enVal] of Object.entries(en)) {
            if (typeof enVal !== 'string') continue;
            const enPlaceholders = extractPlaceholders(enVal).join(',');
            for (const [lang, localeObj] of secondaryLocales) {
                const val = localeObj[key];
                if (typeof val !== 'string') continue;
                const langPlaceholders = extractPlaceholders(val).join(',');
                if (langPlaceholders !== enPlaceholders) {
                    mismatches.push(
                        `[${lang}] ${key}: expected {{${enPlaceholders}}}, got {{${langPlaceholders}}}`
                    );
                }
            }
        }

        expect(mismatches).toEqual([]);
    });

    it('defines all dynamic voucher.txType.* and signatureHints profile.field.* keys across all locales', () => {
        const requiredKeys = [
            'voucher.txType.init',
            'voucher.txType.transfer',
            'voucher.txType.split',
            'voucher.txType.merge',
            'voucher.txType.redeem',
            'voucher.txType.claim',
            'voucher.txType.block',
            'profile.field.gender',
            'profile.field.coordinates',
            'profile.field.age',
            'profile.field.firstName',
            'profile.field.lastName',
            'profile.field.email',
            'profile.field.phone',
            'profile.field.organization',
            'profile.field.community',
            'profile.field.url',
            'profile.field.address',
            'profile.field.serviceOffer',
            'profile.field.needs',
        ];

        const locales: Array<[string, Record<string, string>]> = [
            ['en', en],
            ['de', de],
            ['fr', fr],
            ['es', es],
            ['it', itLocale],
        ];

        for (const [lang, obj] of locales) {
            for (const key of requiredKeys) {
                expect(obj[key], `Missing key ${key} in ${lang}.json`).toBeDefined();
                expect(obj[key].trim().length, `Empty key ${key} in ${lang}.json`).toBeGreaterThan(0);
            }
        }
    });

    it('F2: translates every backend GUARANTOR_ANCHOR_ISSUE_CODES constant in all 5 locales', () => {
        const CURRENT_DIR = path.dirname(fileURLToPath(import.meta.url));
        const vouchersRsPath = path.resolve(CURRENT_DIR, '../../../src-tauri/src/commands/vouchers.rs');
        const rustSrc = fs.readFileSync(vouchersRsPath, 'utf-8');

        const constValues = new Map<string, string>();
        for (const m of rustSrc.matchAll(
            /pub const (GUARANTOR_ISSUE_[A-Z0-9_]+):\s*&str\s*=\s*"([^"]+)";/g,
        )) {
            constValues.set(m[1], m[2]);
        }

        const listMatch = rustSrc.match(
            /pub const GUARANTOR_ANCHOR_ISSUE_CODES:\s*&\[&str\]\s*=\s*&\[([\s\S]*?)\];/,
        );
        expect(listMatch, 'GUARANTOR_ANCHOR_ISSUE_CODES missing in vouchers.rs').toBeTruthy();

        const issueCodes = listMatch![1]
            .split(',')
            .map((s) => s.trim())
            .filter(Boolean)
            .map((ident) => constValues.get(ident) ?? ident.replace(/^"|"$/g, ''));

        expect(issueCodes.length).toBeGreaterThanOrEqual(5);

        const locales: Array<[string, Record<string, string>]> = [
            ['en', en],
            ['de', de],
            ['fr', fr],
            ['es', es],
            ['it', itLocale],
        ];

        for (const code of issueCodes) {
            const key = `voucher.signRequest.anchorIssue.${code}`;
            for (const [lang, obj] of locales) {
                expect(obj[key], `Missing F2 anchor issue key ${key} in ${lang}.json`).toBeDefined();
                expect(obj[key].trim().length, `Empty key ${key} in ${lang}.json`).toBeGreaterThan(0);
            }
        }
    });
});
