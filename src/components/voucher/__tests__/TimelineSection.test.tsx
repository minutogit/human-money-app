import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { TimelineSection } from '../TimelineSection';
import { VoucherSignature, TransactionHistoryEntry } from '../../../types';

describe('TimelineSection', () => {
    const mockSignatures: VoucherSignature[] = [
        {
            voucherId: 'voucher-123',
            signatureId: 'sig-creator-1',
            signerId: 'did:key:creator',
            signature: 'sig1',
            signatureTime: '2024-01-01T00:00:00Z',
            role: 'creator',
            details: {
                id: 'did:key:creator',
                firstName: 'Alice',
                lastName: 'Creator'
            }
        },
        {
            voucherId: 'voucher-123',
            signatureId: 'sig-guarantor-2',
            signerId: 'did:key:guarantor',
            signature: 'sig2',
            signatureTime: '2024-01-01T01:00:00Z',
            role: 'guarantor',
            details: {
                id: 'did:key:guarantor',
                firstName: 'Bob',
                lastName: 'Guarantor'
            }
        }
    ];

    const mockTransactions: TransactionHistoryEntry[] = [];

    it('does not display the remove signature button for the creator signature, even if status is incomplete', () => {
        const onRemoveSignature = vi.fn();
        render(
            <TimelineSection 
                signatures={mockSignatures}
                transactions={mockTransactions}
                displayCurrency="Minutos"
                onRemoveSignature={onRemoveSignature}
                voucherStatus="incomplete"
            />
        );

        // Find all remove buttons by test ID.
        // There are 2 signatures: one creator, one guarantor.
        // The creator should NOT have a remove button. The guarantor SHOULD have one.
        // So we expect exactly 1 button to be rendered in total.
        const buttons = screen.getAllByTestId('remove-signature');
        expect(buttons).toHaveLength(1);

        // Click the single available button and ensure it triggers the callback for the guarantor's signatureId
        fireEvent.click(buttons[0]);
        expect(onRemoveSignature).toHaveBeenCalledWith('sig-guarantor-2');
        expect(onRemoveSignature).not.toHaveBeenCalledWith('sig-creator-1');
    });

    it('does not display any remove signature buttons if the voucher status is active', () => {
        const onRemoveSignature = vi.fn();
        render(
            <TimelineSection 
                signatures={mockSignatures}
                transactions={mockTransactions}
                displayCurrency="Minutos"
                onRemoveSignature={onRemoveSignature}
                voucherStatus="active"
            />
        );

        // No buttons should be rendered since status is active (i.e. not incomplete)
        const buttons = screen.queryAllByTestId('remove-signature');
        expect(buttons).toHaveLength(0);
    });

    it('does not display any remove signature buttons if the voucher status is endorsed', () => {
        const onRemoveSignature = vi.fn();
        render(
            <TimelineSection 
                signatures={mockSignatures}
                transactions={mockTransactions}
                displayCurrency="Minutos"
                onRemoveSignature={onRemoveSignature}
                voucherStatus="endorsed"
            />
        );

        // No buttons should be rendered since status is endorsed
        const buttons = screen.queryAllByTestId('remove-signature');
        expect(buttons).toHaveLength(0);
    });
});
