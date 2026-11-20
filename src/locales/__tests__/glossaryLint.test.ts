import { describe, it, expect } from 'vitest';
import deJson from '../de.json';
import enJson from '../en.json';
import esJson from '../es.json';
import frJson from '../fr.json';
import itJson from '../it.json';

/**
 * Glossary Lint Test Suite (enforces docs/UX_GLOSSARY.md).
 *
 * Verifies that:
 * 1. German translations (de.json) do not contain forbidden jargon or superseded terminology
 *    (Sync-Ablage, Kontaktkarte, Marktplatz-Karte, Vertrauensdatei, .wot, Geldbörse, Wallet,
 *     did:key, DID, Hop/Hops, Betrugsmeldung, Feed, Listing).
 *    Allowlisted strictly to help.*.long or settings.advanced* where technical concepts are explained.
 * 2. Web of Trust / Contact keys (addressBook.*, wot.*, help.wot*, help.trustLevels*)
 *    do not contain "bürgen", "haften", "Bürge", "Bürgschaft", ensuring guarantee terminology
 *    is strictly reserved for Layer-1 voucher guarantors.
 * 3. English translations (en.json) also respect the core glossary rules.
 */

describe('UX Glossary Linting (docs/UX_GLOSSARY.md)', () => {
    const de = deJson as Record<string, string>;
    const en = enJson as Record<string, string>;

    const stripPlaceholders = (text: string): string =>
        text.replace(/\{\{[^}]+\}\}/g, '').replace(/humoco-sync-server/g, '');

    const isAllowlisted = (key: string): boolean => {
        // Only help.*.long, help.*.deepDive, settings.advanced*, and *.expert.* keys may use technical terms
        if (/^help\.[^.]+\.(long|deepDive)$/.test(key)) return true;
        if (/^settings\.advanced/.test(key)) return true;
        if (/\.expert(\.|$)/.test(key)) return true;
        return false;
    };

    const isTechnicalLockServerKey = (key: string): boolean => {
        if (key === 'error.network.l2Offline' || key === 'profile.backupRestoreOfflineNotice') return true;
        if (key.startsWith('help.')) return true;
        if (
            key.startsWith('transfer.checkout.') ||
            key.startsWith('transfer.l2.') ||
            key === 'transfer.l2PreparedOfflineDesc'
        ) {
            return true;
        }
        if (key.startsWith('voucher.')) return true;
        if (
            /^settings\.(addServerButton|anchorTemplates|benchmarkButton|cachedLockProbes|clearOwnServerBtn|customServersTitle|l2Mode|lifecycle|lockProbe|lockServerUrlLabel|noCustomServers|ownLock|ownNominated|probeLockServerBtn|probeServersFirstHint|probingLockServer|saveOwnServerBtn|selectCommitteeServers|serverNamePlaceholder|serverUrlPlaceholder|useAsOwnServer)/.test(
                key,
            )
        ) {
            return true;
        }
        return false;
    };

    const FORBIDDEN_DE_PATTERNS: Array<{ name: string; pattern: RegExp }> = [
        { name: 'Sync-Ablage', pattern: /Sync-Ablage/i },
        { name: 'Kontaktkarte', pattern: /Kontaktkarte/i },
        { name: 'Marktplatz-Karte', pattern: /Marktplatz-Karte/i },
        { name: 'Vertrauensdatei', pattern: /Vertrauensdatei/i },
        { name: '.wot', pattern: /\.wot/i },
        { name: 'Geldbörse', pattern: /Geldbörse/i },
        { name: 'Wallet', pattern: /\bWallet\b/i },
        { name: 'did:key', pattern: /did:key/i },
        { name: 'DID', pattern: /\bDID\b/ },
        { name: 'Hop/Hops', pattern: /\bHops?\b/i },
        { name: 'Betrugsmeldung', pattern: /Betrugsmeldung/i },
        { name: 'Feed', pattern: /\bFeed\b/i },
        { name: 'Listing', pattern: /\bListing\b/i },
        { name: 'Server', pattern: /\bServer\w*/i },
    ];

    const WOT_KEY_PREFIXES = ['addressBook.', 'contacts.', 'wot.', 'help.wot', 'help.trustLevels'];
    const WOT_FORBIDDEN_DE_PATTERN = /\b(bürg\w*|buerg\w*|haft\w*)\b/i;

    it('ensures de.json contains no forbidden terms in standard keys', () => {
        const violations: Array<{ key: string; term: string; text: string }> = [];

        for (const [key, value] of Object.entries(de)) {
            if (typeof value !== 'string') continue;
            if (isAllowlisted(key)) continue;

            const cleanText = stripPlaceholders(value);
            for (const { name, pattern } of FORBIDDEN_DE_PATTERNS) {
                if (name === 'Server' && isTechnicalLockServerKey(key)) continue;
                if (pattern.test(cleanText)) {
                    violations.push({ key, term: name, text: value });
                }
            }
        }

        if (violations.length > 0) {
            const report = violations
                .map((v) => `  - [${v.term}] ${v.key}: "${v.text}"`)
                .join('\n');
            expect.fail(`Found ${violations.length} forbidden glossary term(s) in de.json:\n${report}`);
        }
    });

    it('ensures Web of Trust & Contact keys never use "bürgen" or "haften"', () => {
        const violations: Array<{ key: string; text: string }> = [];

        for (const [key, value] of Object.entries(de)) {
            if (typeof value !== 'string') continue;

            const isWotKey = WOT_KEY_PREFIXES.some((prefix) => key.startsWith(prefix));
            if (!isWotKey) continue;

            const cleanText = stripPlaceholders(value);
            if (WOT_FORBIDDEN_DE_PATTERN.test(cleanText)) {
                violations.push({ key, text: value });
            }
        }

        if (violations.length > 0) {
            const report = violations
                .map((v) => `  - ${v.key}: "${v.text}"`)
                .join('\n');
            expect.fail(
                `Found ${violations.length} WoT/Contact key(s) using guarantor terminology (bürgen/haften):\n${report}`
            );
        }
    });

    it('ensures en.json respects core terminology without forbidden jargon', () => {
        const violations: Array<{ key: string; term: string; text: string }> = [];

        const EN_FORBIDDEN_PATTERNS: Array<{ name: string; pattern: RegExp }> = [
            { name: 'did:key', pattern: /did:key/i },
            { name: 'DID', pattern: /\bDID\b/ },
            { name: 'Hop/Hops', pattern: /\bHops?\b/i },
            { name: 'Wallet', pattern: /\bWallet\b/i },
            { name: 'Listing', pattern: /\bListing\b/i },
            { name: '.wot', pattern: /\.wot/i },
        ];

        for (const [key, value] of Object.entries(en)) {
            if (typeof value !== 'string') continue;
            if (isAllowlisted(key)) continue;

            const cleanText = stripPlaceholders(value);
            for (const { name, pattern } of EN_FORBIDDEN_PATTERNS) {
                if (pattern.test(cleanText)) {
                    violations.push({ key, term: name, text: value });
                }
            }
        }

        if (violations.length > 0) {
            const report = violations
                .map((v) => `  - [${v.term}] ${v.key}: "${v.text}"`)
                .join('\n');
            expect.fail(`Found ${violations.length} forbidden term(s) in en.json:\n${report}`);
        }
    });

    it('ensures allowlist keys only apply to valid technical explanation paths', () => {
        for (const key of Object.keys(de)) {
            if (isAllowlisted(key)) {
                expect(
                    (key.startsWith('help.') && (key.endsWith('.long') || key.endsWith('.deepDive'))) ||
                        key.startsWith('settings.advanced') ||
                        /\.expert(\.|$)/.test(key),
                    `Unexpected allowlisted key pattern: ${key}`
                ).toBe(true);
            }
        }
    });

    it('detects forbidden terms and guarantor terms if injected into test fixtures', () => {
        const sampleViolations = [
            'Sync-Ablage einrichten',
            'Hier ist deine neue Kontaktkarte',
            'Importiere die Vertrauensdatei jetzt',
            'Öffne die .wot Datei',
            'Guthaben in der Geldbörse',
            'Dein Wallet ist gesperrt',
            'did:key:z6MkuSLm... anzeigen',
            'Nutzer DID kopieren',
            'Erreiche Kontakte über 2 Hops',
            'Neue Betrugsmeldung eingegangen',
            'Pinnwand Feed aktualisieren',
            'Neues Listing anlegen',
            'Sync-Server verbinden',
        ];

        for (const sample of sampleViolations) {
            const hasMatch = FORBIDDEN_DE_PATTERNS.some(({ pattern }) => pattern.test(sample));
            expect(hasMatch, `Sample should have been detected as forbidden: "${sample}"`).toBe(true);
        }

        const wotSamples = [
            'Ich bürge für diesen Kontakt',
            'Bürgschaft für Alice übernehmen',
            'Haftung im Vertrauensnetzwerk',
            'Du haftest bei Betrug',
        ];

        for (const sample of wotSamples) {
            expect(
                WOT_FORBIDDEN_DE_PATTERN.test(sample),
                `Sample should have triggered WoT guarantor prohibition: "${sample}"`
            ).toBe(true);
        }
    });

    it('ensures all 5 locales contain zero stale ReceiveOnly / Kassenmodus / 14-of-20 terminology (F1)', () => {
        const allLocales: Array<[string, Record<string, string>]> = [
            ['de', de],
            ['en', en],
            ['es', esJson as Record<string, string>],
            ['fr', frJson as Record<string, string>],
            ['it', itJson as Record<string, string>],
        ];
        const stalePatterns: Array<{ name: string; pattern: RegExp }> = [
            { name: 'Kassenmodus', pattern: /\bKassenmodus\b/i },
            { name: 'nur Empfang', pattern: /\bnur Empfang\b/i },
            { name: 'ReceiveOnly', pattern: /\bReceiveOnly\b/i },
            { name: 'Cashier Mode', pattern: /\bCashier\s+Mode\b/i },
            { name: 'receive only', pattern: /\breceive\s+only\b/i },
            { name: 'modo caja', pattern: /\bmodo\s+caja\b/i },
            { name: 'mode caisse', pattern: /\bmode\s+caisse\b/i },
            { name: 'modalità cassa', pattern: /\bmodalità\s+cassa\b/i },
            { name: '14/20', pattern: /14\/20/ },
            { name: '14 von 20', pattern: /14\s+von\s+20/i },
            { name: '14 of 20', pattern: /14\s+of\s+20/i },
        ];

        const violations: string[] = [];
        for (const [lang, obj] of allLocales) {
            for (const [key, value] of Object.entries(obj)) {
                if (typeof value !== 'string') continue;
                for (const { name, pattern } of stalePatterns) {
                    if (pattern.test(value)) {
                        violations.push(`[${lang}] (${name}) ${key}: "${value}"`);
                    }
                }
            }
        }

        expect(violations).toEqual([]);
    });

    it('F3: ensures es/fr/it contain zero untranslated English strings (>12 chars) outside explicit proper-noun allowlist', () => {
        const PROPER_NOUN_OR_COGNATE_ALLOWLIST = new Set([
            'common.appName',
            'common.noticeSignaturesPart',
            'common.protocolVersion',
            'common.supportTitle',
            'common.version',
            'conflict.errorPrefix',
            'marketplace.counterTotal',
            'profile.emailPlaceholder',
            'profile.phonePlaceholder',
            'profile.pictureUrlPlaceholder',
            'profile.websitePlaceholder',
            'settings.serverUrlPlaceholder',
            'transfer.pillars.l1FormatOnly',
            'transfer.receiveSuccess.countableItem',
        ]);

        const secondaryLocales: Array<[string, Record<string, string>]> = [
            ['es', esJson as Record<string, string>],
            ['fr', frJson as Record<string, string>],
            ['it', itJson as Record<string, string>],
        ];
        const untranslated: string[] = [];

        for (const [key, enVal] of Object.entries(en)) {
            if (typeof enVal !== 'string' || enVal.length <= 12) continue;
            if (PROPER_NOUN_OR_COGNATE_ALLOWLIST.has(key)) continue;
            for (const [lang, locObj] of secondaryLocales) {
                if (locObj[key] === enVal) {
                    untranslated.push(`[${lang}] ${key}: "${enVal}"`);
                }
            }
        }

        expect(untranslated).toEqual([]);
    });

    it('F3: ensures es/fr/it respect UX glossary without forbidden technical jargon in standard keys', () => {
        const FORBIDDEN_INTL_PATTERNS: Array<{ name: string; pattern: RegExp }> = [
            { name: 'did:key', pattern: /did:key/i },
            { name: 'DID', pattern: /\bDID\b/ },
            { name: 'Hop/Hops', pattern: /\bHops?\b/i },
            { name: 'Wallet', pattern: /\bWallet\b/i },
            { name: 'Listing', pattern: /\bListing\b/i },
            { name: '.wot', pattern: /\.wot/i },
        ];

        const secondaryLocales: Array<[string, Record<string, string>]> = [
            ['es', esJson as Record<string, string>],
            ['fr', frJson as Record<string, string>],
            ['it', itJson as Record<string, string>],
        ];
        const violations: string[] = [];

        for (const [lang, locObj] of secondaryLocales) {
            for (const [key, value] of Object.entries(locObj)) {
                if (typeof value !== 'string') continue;
                if (isAllowlisted(key)) continue;
                const cleanText = stripPlaceholders(value);
                for (const { name, pattern } of FORBIDDEN_INTL_PATTERNS) {
                    if (pattern.test(cleanText)) {
                        violations.push(`[${lang}] (${name}) ${key}: "${value}"`);
                    }
                }
            }
        }

        expect(violations).toEqual([]);
    });
});
