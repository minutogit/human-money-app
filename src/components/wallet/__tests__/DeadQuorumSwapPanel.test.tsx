import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { DeadQuorumSwapPanel } from '../DeadQuorumSwapPanel';
import { voucherService } from '../../../services/voucherService';
import { UiModeProvider, UI_MODE_STORAGE_KEY } from '../../../context/UiModeContext';

vi.mock('../../../services/voucherService', () => ({
    voucherService: {
        createDeadQuorumSwapOffer: vi.fn(),
        createDeadQuorumSwapClaim: vi.fn(),
        adjudicateDeadQuorumSwap: vi.fn(),
    },
}));

describe('DeadQuorumSwapPanel', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.setItem(UI_MODE_STORAGE_KEY, 'expert');
    });

    it('renders approved claims, rejected claims, and doubleSpendProofsCount without crashing', async () => {
        vi.mocked(voucherService.adjudicateDeadQuorumSwap).mockResolvedValue({
            approved: [
                {
                    claimantDid: 'did:key:zClaimantAlice',
                    voucherId: 'v-1',
                    tipTId: 't-1',
                    approvedAmount: '40.00',
                },
            ],
            rejected: [
                {
                    claimantDid: 'did:key:zClaimantBob',
                    voucherId: 'v-2',
                    tipTId: 't-2',
                    reason: 'Conflicting spend proof',
                },
            ],
            doubleSpendProofsCount: 2,
        });

        render(
            <UiModeProvider>
                <DeadQuorumSwapPanel
                    localInstanceId="inst-1"
                    isActiveVoucher={true}
                    protectAction={async (fn) => fn('test-pw')}
                    findMatchingStandardToml={async () => 'name = "Test"'}
                    onRefreshDetails={async () => undefined}
                />
            </UiModeProvider>
        );

        expect(screen.getByText(/^(Not-Umtausch|Emergency Exchange)$/)).toBeInTheDocument();

        const offerArea = screen.getByPlaceholderText(/Not-Umtausch-Angebot|emergency swap offer/i);
        fireEvent.change(offerArea, { target: { value: '{"offer":"ok"}' } });

        const subsArea = screen.getByPlaceholderText(/Einreichungen der Inhaber|holder claim submissions/i);
        fireEvent.change(subsArea, { target: { value: '{"claim":"ok"}' } });

        const adjBtn = screen.getByRole('button', { name: /Not-Umtausch entscheiden|Adjudicate Emergency Exchange/i });
        fireEvent.click(adjBtn);

        await waitFor(() => {
            expect(screen.getByText(/did:key:zClaimantAlice \(40\.00\)/)).toBeInTheDocument();
            expect(screen.getByText(/did:key:zClaimantBob: Conflicting spend proof/)).toBeInTheDocument();
            expect(screen.getByText(/Doppelausgabe-Beweise: 2|Double-spend proofs found: 2/i)).toBeInTheDocument();
        });
    });
});
