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
});
