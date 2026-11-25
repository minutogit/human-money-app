import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { TimelineSection } from '../TimelineSection';
import { VoucherSignature, TransactionHistoryEntry } from '../../../types';
import { UiModeProvider } from '../../../context/UiModeContext';

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

    it('protects both creator and issuer roles from signature removal while displaying signer names', () => {
        const onRemoveSignature = vi.fn();
        const signaturesWithIssuer: VoucherSignature[] = [
            ...mockSignatures,
            {
                voucherId: 'voucher-123',
                signatureId: 'sig-issuer-3',
                signerId: 'did:key:issuer',
                signature: 'sig3',
                signatureTime: '2024-01-01T02:00:00Z',
                role: 'issuer',
                details: {
                    id: 'did:key:issuer',
                    firstName: 'Clara',
                    lastName: 'Issuer',
                },
            },
        ];

        render(
            <TimelineSection
                signatures={signaturesWithIssuer}
                transactions={mockTransactions}
                displayCurrency="Minutos"
                onRemoveSignature={onRemoveSignature}
                voucherStatus="incomplete"
            />,
        );

        expect(screen.getByText('Alice Creator')).toBeInTheDocument();
        expect(screen.getByText('Bob Guarantor')).toBeInTheDocument();
        expect(screen.getByText('Clara Issuer')).toBeInTheDocument();

        const buttons = screen.getAllByTestId('remove-signature');
        expect(buttons).toHaveLength(1);
        fireEvent.click(buttons[0]);
        expect(onRemoveSignature).toHaveBeenCalledTimes(1);
        expect(onRemoveSignature).toHaveBeenCalledWith('sig-guarantor-2');
    });

    it('hides transaction history in simple mode and renders reversed history with SYSTEM fallback in expert mode', () => {
        const txs: TransactionHistoryEntry[] = [
            {
                tId: 'tx-init',
                tType: 'init',
                amount: '100',
                tTime: '2024-01-01T00:00:00Z',
                recipientId: 'did:key:creator',
            },
            {
                tId: 'tx-transfer',
                tType: 'transfer',
                amount: '25',
                senderId: 'did:key:z6MkNewerSigner111',
                recipientId: 'did:key:z6MkRecipient111',
                tTime: '2024-01-02T00:00:00Z',
            },
        ];

        const { unmount } = render(
            <UiModeProvider initialMode="simple">
                <TimelineSection
                    signatures={mockSignatures}
                    transactions={txs}
                    displayCurrency="Minutos"
                    onRemoveSignature={vi.fn()}
                    voucherStatus="active"
                />
            </UiModeProvider>,
        );
        expect(screen.queryByText('25 Minutos')).toBeNull();
        expect(screen.queryByText('SYSTEM')).toBeNull();
        unmount();

        render(
            <UiModeProvider initialMode="expert">
                <TimelineSection
                    signatures={mockSignatures}
                    transactions={txs}
                    displayCurrency="Minutos"
                    onRemoveSignature={vi.fn()}
                    voucherStatus="active"
                />
            </UiModeProvider>,
        );
        const latestTx = screen.getByText('25 Minutos');
        const initialTx = screen.getByText('100 Minutos');
        expect(latestTx.compareDocumentPosition(initialTx) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(
            Node.DOCUMENT_POSITION_FOLLOWING,
        );
        expect(screen.getByText('SYSTEM')).toBeInTheDocument();
    });
});
