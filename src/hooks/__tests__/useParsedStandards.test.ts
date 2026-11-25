import { renderHook } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { useParsedStandards, parseStandardInfo } from '../useParsedStandards';
import { VoucherStandardInfo } from '../../types';

const minutoStandard: VoucherStandardInfo = {
    id: 'minuto_v1',
    displayName: 'Minuto',
    content: `
# Sample standard TOML
[immutable.identity]
uuid = "MINUTO-V1-2025-09" # inline comment
name = "Minuto"
abbreviation = "MIN"

[immutable.blueprint]
unit = "Minuto"

[immutable.features]
allow_partial_transfers = true
balances_are_summable = true
amount_decimal_places = 0
privacy_mode = "flexible"

[mutable.metadata]
issuer_name = "Human Money Project"

[mutable.app_config]
default_validity_duration = "P5Y"

[mutable.i18n.descriptions]
en = "Hashtag # inside string should not break parsing"
`,
};

const flatStandard: VoucherStandardInfo = {
    id: 'custom_std',
    displayName: 'Custom Display',
    content: `
name = "CustomToken"
uuid = "custom-uuid-123"
amountDecimalPlaces = 2
allowPartialTransfers = false
`,
};

describe('useParsedStandards', () => {
    it('parses sectioned TOML and flat/camelCase test fixtures accurately', () => {
        const parsedMinuto = parseStandardInfo(minutoStandard);
        expect(parsedMinuto.uuid).toBe('MINUTO-V1-2025-09');
        expect(parsedMinuto.name).toBe('Minuto');
        expect(parsedMinuto.abbreviation).toBe('MIN');
        expect(parsedMinuto.unit).toBe('Minuto');
        expect(parsedMinuto.decimalPlaces).toBe(0);
        expect(parsedMinuto.allowPartialTransfers).toBe(true);
        expect(parsedMinuto.balancesAreSummable).toBe(true);
        expect(parsedMinuto.privacyMode).toBe('flexible');
        expect(parsedMinuto.issuerName).toBe('Human Money Project');
        expect(parsedMinuto.defaultValidityDuration).toBe('P5Y');

        const parsedFlat = parseStandardInfo(flatStandard);
        expect(parsedFlat.uuid).toBe('custom-uuid-123');
        expect(parsedFlat.name).toBe('CustomToken');
        expect(parsedFlat.decimalPlaces).toBe(2);
        expect(parsedFlat.allowPartialTransfers).toBe(false);
    });

    it('exposes memoized parsedMap, findStandardByUuid, and getStandardName', () => {
        const standards = [minutoStandard, flatStandard];
        const { result, rerender } = renderHook(
            ({ items }) => useParsedStandards(items),
            { initialProps: { items: standards } }
        );

        const initialMap = result.current.parsedMap;
        expect(initialMap.size).toBe(2);
        expect(result.current.standards).toBe(standards);

        expect(result.current.findStandardByUuid('MINUTO-V1-2025-09')?.id).toBe('minuto_v1');
        expect(result.current.findStandardByUuid('minuto_v1')?.uuid).toBe('MINUTO-V1-2025-09');
        expect(result.current.findStandardByUuid('missing')).toBeUndefined();

        expect(result.current.getStandardName('custom-uuid-123')).toBe('CustomToken');
        expect(result.current.getStandardName('missing-uuid', 'Fallback Label')).toBe('Fallback Label');
        expect(result.current.getStandardName('missing-uuid')).toBe('missing-uuid');

        // Re-rendering with the same standards array keeps the same parsedMap instance
        rerender({ items: standards });
        expect(result.current.parsedMap).toBe(initialMap);
    });

    it('unescapes quoted TOML strings with hashtags and falls back safely on empty content', () => {
        const quotedStd: VoucherStandardInfo = {
            id: 'quoted_std',
            displayName: 'Quoted Fallback Display',
            content: [
                'name = "Token #1 \\"Pro\\" \\\\ v1"',
                "unit = 'Min#1'",
                'privacy_mode = "flexible"',
                'issuer_name = "Local Guild"',
                'default_validity_duration = "P2Y"',
                'malformed line without equals',
            ].join('\n'),
        };
        const emptyStd: VoucherStandardInfo = {
            id: 'empty_std',
            displayName: 'Empty Display Fallback',
            content: '',
        };

        const parsedQuoted = parseStandardInfo(quotedStd);
        expect(parsedQuoted.name).toBe('Token #1 "Pro" \\ v1');
        expect(parsedQuoted.unit).toBe('Min#1');
        expect(parsedQuoted.privacyMode).toBe('flexible');
        expect(parsedQuoted.issuerName).toBe('Local Guild');
        expect(parsedQuoted.defaultValidityDuration).toBe('P2Y');

        const parsedEmpty = parseStandardInfo(emptyStd);
        expect(parsedEmpty.decimalPlaces).toBe(4);
        expect(parsedEmpty.uuid).toBeNull();
        expect(parsedEmpty.name).toBeNull();
        expect(parsedEmpty.allowPartialTransfers).toBeNull();

        const { result } = renderHook(() => useParsedStandards([emptyStd]));
        expect(result.current.getStandardName('empty_std')).toBe('Empty Display Fallback');
    });

    it('isolates TOML sections so unrelated sections cannot override identity or features fields', () => {
        const sectionedStd: VoucherStandardInfo = {
            id: 'isolated_std',
            displayName: 'Isolated Standard',
            content: [
                '[immutable.identity]',
                'name = "RealName"',
                '',
                '[other.section]',
                'name = "SpoofedName"',
                'amount_decimal_places = 9',
            ].join('\n'),
        };

        const parsed = parseStandardInfo(sectionedStd);
        expect(parsed.name).toBe('RealName');
        expect(parsed.decimalPlaces).toBe(4);
    });
});
