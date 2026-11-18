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
});
