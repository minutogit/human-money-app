import { describe, it, expect } from 'vitest';
import { classifyHumocoFile } from '../humocoFileClassifier';

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
});
