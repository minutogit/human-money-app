import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { VoucherBasicsForm } from '../VoucherBasicsForm';
import { VoucherStandardDefinition, UiMode } from '../../../types';
import { UiModeProvider } from '../../../context/UiModeContext';

const mockStandardsWithNames = [
    { id: 'freetaler_v1', displayName: 'FreeTaler', content: '...' },
    { id: 'minuto_v1', displayName: 'Minuto', content: '...' },
];

const mockParsedStandard: VoucherStandardDefinition = {
    immutable: {
        identity: { uuid: 'freetaler-uuid', name: 'FreeTaler', abbreviation: 'Taler', issuerId: '0:is@did:key:z6Mkexample' },
        blueprint: { unit: 'Taler', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
        features: {
            allowPartialTransfers: true,
            balancesAreSummable: true,
            amountDecimalPlaces: 4,
            privacyMode: 'public',
            allowedTTypes: [],
        },
        issuance: {
            validityDurationRange: [],
            issuanceMinimumValidityDuration: 'P1Y',
            additionalSignaturesRange: [],
            allowedSignatureRoles: [],
        },
        customRules: {},
    },
    mutable: {
        metadata: { issuerName: 'Human Money Project', keywords: [] },
        appConfig: { defaultValidityDuration: 'P1Y' },
        i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
    },
};

function renderForm(
    overrides?: Partial<React.ComponentProps<typeof VoucherBasicsForm>>,
    uiMode: UiMode = 'simple',
) {
    return render(
        <UiModeProvider initialMode={uiMode}>
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
                {...overrides}
            />
        </UiModeProvider>
    );
}

describe('VoucherBasicsForm', () => {
    beforeEach(() => {
        localStorage.clear();
    });

    it('renders human-readable standard names in the dropdown', () => {
        renderForm();

        const option = screen.getByText('FreeTaler');
        expect(option).toBeInTheDocument();
        expect(option.tagName).toBe('OPTION');

        // Also verify the raw ID is NOT shown as text
        const rawOption = screen.queryByText('freetaler_v1');
        expect(rawOption).toBeNull();
    });

    it('hides manage-standards and validity disclosure in simple mode and shows them in expert mode', async () => {
        const onManageStandards = vi.fn();

        const { unmount } = renderForm({ onManageStandards });
        expect(screen.queryByTestId('manage-standards-button')).not.toBeInTheDocument();
        expect(screen.queryByTestId('voucher-validity-toggle')).not.toBeInTheDocument();
        unmount();

        renderForm({ onManageStandards }, 'expert');

        const manageBtn = screen.getByTestId('manage-standards-button');
        expect(manageBtn).toBeInTheDocument();
        expect(screen.getByTestId('voucher-validity-toggle')).toBeInTheDocument();

        await userEvent.click(manageBtn);
        expect(onManageStandards).toHaveBeenCalledTimes(1);
    });

    it('renders short standard description when parsedStandard is provided', () => {
        renderForm({
            selectedStandardId: 'freetaler_v1',
            parsedStandard: mockParsedStandard,
        });

        const summary = screen.getByTestId('standard-short-description');
        expect(summary).toHaveTextContent('FreeTaler — Voucher denominated in Taler.');
    });

    it('propagates standard, amount, and test/real mode callbacks and styles errors', () => {
        const onStandardChange = vi.fn();
        const onAmountChange = vi.fn();
        const onNonRedeemableChange = vi.fn();

        const { container } = renderForm({
            onStandardChange,
            onAmountChange,
            onNonRedeemableChange,
            nonRedeemable: true,
            errors: { standard: true, amount: true },
        });

        const typeSelect = container.querySelector('#voucher-type') as HTMLSelectElement;
        expect(typeSelect.className).toContain('border-rose-500');
        fireEvent.change(typeSelect, { target: { value: 'minuto_v1' } });
        expect(onStandardChange).toHaveBeenCalledWith('minuto_v1');

        const amountInput = container.querySelector('#voucher-amount') as HTMLInputElement;
        expect(amountInput.className).toContain('border-rose-500');
        fireEvent.change(amountInput, { target: { value: '12.50' } });
        expect(onAmountChange).toHaveBeenCalledWith('12.50');

        fireEvent.click(screen.getByTestId('voucher-mode-real'));
        expect(onNonRedeemableChange).toHaveBeenCalledWith(false);
    });

    it('expands validity disclosure in expert mode and propagates validity value and unit changes', () => {
        const onValidityValueChange = vi.fn();
        const onValidityUnitChange = vi.fn();

        renderForm(
            {
                onValidityValueChange,
                onValidityUnitChange,
            },
            'expert',
        );

        fireEvent.click(screen.getByTestId('voucher-validity-toggle'));
        const content = screen.getByTestId('voucher-validity-content');

        const valInput = content.querySelector('input[type="number"]') as HTMLInputElement;
        fireEvent.change(valInput, { target: { value: '3' } });
        expect(onValidityValueChange).toHaveBeenCalledWith(3);

        const unitSelect = content.querySelector('select') as HTMLSelectElement;
        fireEvent.change(unitSelect, { target: { value: 'M' } });
        expect(onValidityUnitChange).toHaveBeenCalledWith('M');
    });
});
