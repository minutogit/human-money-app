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
});
