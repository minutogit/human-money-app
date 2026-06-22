import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { VoucherBasicsForm } from '../VoucherBasicsForm';

describe('VoucherBasicsForm', () => {
    it('renders human-readable standard names in the dropdown', () => {
        const mockStandardsWithNames = [
            { id: 'freetaler_v1', displayName: 'FreeTaler', content: '...' },
            { id: 'minuto_v1', displayName: 'Minuto', content: '...' }
        ];

        render(
            <VoucherBasicsForm 
                standards={mockStandardsWithNames}
                selectedStandardId=""
                onStandardChange={() => {}}
                amount=""
                onAmountChange={() => {}}
                validityValue={1}
                onValidityValueChange={() => {}}
                validityUnit="Y"
                onValidityUnitChange={() => {}}
                nonRedeemable={false}
                onNonRedeemableChange={() => {}}
                isLoading={false}
                errors={{}}
                standardRef={{ current: null }}
                amountRef={{ current: null }}
            />
        );

        const option = screen.getByText('FreeTaler');
        expect(option).toBeDefined();
        
        // Also verify the raw ID is NOT shown as text
        const rawOption = screen.queryByText('freetaler_v1');
        expect(rawOption).toBeNull();
    });
});
