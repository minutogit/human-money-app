import { describe, it, expect } from 'vitest';
import {
    classifyHumocoFile,
    isSupportedHumocoFileName,
    classifyExtension,
    isReceiveImportKind,
} from '../humocoFileClassifier';

describe('humocoFileClassifier', () => {
    it('classifies wot-provider-filter+v1 JSON payloads as wot packages', () => {
        const payload = JSON.stringify({
            typ: 'wot-provider-filter+v1',
            issuer: 'did:key:zProvider1',
            entries: [],
        });
        const bytes = Array.from(new TextEncoder().encode(payload));

        const res = classifyHumocoFile({ fileName: 'alliance.json', bytes });
        expect(res.kind).toBe('wot');
    });

    it('rejects non-standard .toml config files instead of routing them as voucher standards', () => {
        const nonStandardToml = '[package]\nname = "Cargo"\nversion = "0.1.0"\n';
        const bytes = Array.from(new TextEncoder().encode(nonStandardToml));

        const res = classifyHumocoFile({ fileName: 'Cargo.toml', bytes });
        expect(res.kind).toBe('unknown');
    });

    it('accepts genuine voucher standard .toml files with [immutable] and [blueprint] markers', () => {
        const standardToml = '[immutable.identity]\nuuid = "std-1"\n[immutable.blueprint]\nunit = "Min"\n';
        const bytes = Array.from(new TextEncoder().encode(standardToml));

        const res = classifyHumocoFile({ fileName: 'minuto.toml', bytes });
        expect(res.kind).toBe('standard');
    });

    it('F6: classifies canonical snake_case proof JSON and rejects camelCase legacy proof JSON', () => {
        const canonicalProof = JSON.stringify({
            proof_id: 'proof-1',
            conflicting_transactions: [],
        });
        const camelProof = JSON.stringify({
            proofId: 'proof-1',
            conflictingTransactions: [],
        });

        expect(
            classifyHumocoFile({
                fileName: 'conflict.proof',
                bytes: Array.from(new TextEncoder().encode(canonicalProof)),
            }).kind,
        ).toBe('proof');

        expect(
            classifyHumocoFile({
                fileName: 'legacy.json',
                bytes: Array.from(new TextEncoder().encode(camelProof)),
            }).kind,
        ).toBe('unknown');
    });

    it('F6: classifies .sig detached signature containers by filename/payload', () => {
        const toB64Url = (obj: unknown) => {
            const bytes = new TextEncoder().encode(JSON.stringify(obj));
            const bin = Array.from(bytes, (b) => String.fromCharCode(b)).join('');
            return window.btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
        };
        const canonicalContainer = JSON.stringify({
            ciphertext: toB64Url({ Signature: { voucher_id: 'v-123' } }),
        });
        const canonicalBytes = Array.from(new TextEncoder().encode(canonicalContainer));

        expect(classifyHumocoFile({ fileName: 'reply.sig', bytes: canonicalBytes }).kind).toBe(
            'sig',
        );
    });

    it('detects JSON container magic types and embedded humoco:// deeplinks', () => {
        const toBytes = (str: string) => Array.from(new TextEncoder().encode(str));

        expect(
            classifyHumocoFile({
                fileName: 'data.json',
                bytes: toBytes(JSON.stringify({ format: 'hmc-backup-manifest-v1' })),
            }),
        ).toEqual({
            kind: 'backup',
            source: 'magic',
            containerType: 'hmc-backup-manifest-v1',
        });

        expect(
            classifyHumocoFile({
                fileName: 'bundle.dat',
                bytes: toBytes(JSON.stringify({ c: 'TransactionBundle' })),
            }),
        ).toEqual({
            kind: 'transfer',
            source: 'magic',
            containerType: 'TransactionBundle',
        });

        expect(
            classifyHumocoFile({
                fileName: 'card.json',
                bytes: toBytes(JSON.stringify({ card_wire: 'abc' })),
            }),
        ).toEqual({
            kind: 'card',
            source: 'magic',
            containerType: 'CardEnvelope',
        });

        expect(
            classifyHumocoFile({
                fileName: 'wot.json',
                bytes: toBytes(JSON.stringify({ lists: ['did:key:z1'] })),
            }),
        ).toEqual({
            kind: 'wot',
            source: 'magic',
            containerType: 'WotSyncPackage',
        });

        expect(
            classifyHumocoFile({
                fileName: 'note.txt',
                bytes: toBytes('Visit humoco://pay?to=did:key:z6MkNewerSigner111 today'),
            }),
        ).toEqual({
            kind: 'deeplink',
            source: 'magic',
            deepLink: 'humoco://pay?to=did:key:z6MkNewerSigner111',
        });
    });

    it('prioritizes content magic over misleading extensions and falls back to extension on empty bytes', () => {
        const transferBytes = Array.from(
            new TextEncoder().encode(JSON.stringify({ c: 'TransactionBundle' })),
        );
        expect(classifyHumocoFile({ fileName: 'fake.hmcbackup', bytes: transferBytes })).toEqual({
            kind: 'transfer',
            source: 'magic',
            containerType: 'TransactionBundle',
        });

        expect(classifyHumocoFile({ fileName: 'voucher.ask', bytes: new Uint8Array([]) })).toEqual({
            kind: 'ask',
            source: 'extension',
        });
    });

    it('checks extension filters and receive import eligibility accurately', () => {
        expect(isSupportedHumocoFileName('backup.hmcbackup', true)).toBe(true);
        expect(isSupportedHumocoFileName('backup.hmcbackup', false)).toBe(false);
        expect(isSupportedHumocoFileName('  PAYMENT.TRANSFER  ', false)).toBe(true);
        expect(classifyExtension('NOTES.TXT')).toBe('unknown');

        expect(isReceiveImportKind('transfer')).toBe(true);
        expect(isReceiveImportKind('ask')).toBe(true);
        expect(isReceiveImportKind('sig')).toBe(true);
        expect(isReceiveImportKind('wot')).toBe(true);
        expect(isReceiveImportKind('proof')).toBe(true);
        expect(isReceiveImportKind('card')).toBe(true);

        expect(isReceiveImportKind('standard')).toBe(false);
        expect(isReceiveImportKind('backup')).toBe(false);
        expect(isReceiveImportKind('deeplink')).toBe(false);
        expect(isReceiveImportKind('unknown')).toBe(false);
    });
});
