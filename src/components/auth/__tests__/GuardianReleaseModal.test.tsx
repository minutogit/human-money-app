import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { GuardianReleaseModal } from '../GuardianReleaseModal';
import { recoveryService } from '../../../services/recoveryService';
import { postService } from '../../../services/postService';

vi.mock('../../../services/recoveryService', () => ({
    recoveryService: {
        inspectGuardianRecoveryRequest: vi.fn(),
        approveGuardianRecoveryRequest: vi.fn(),
    },
}));

vi.mock('../../../services/postService', () => ({
    postService: {
        syncPostChannels: vi.fn(),
    },
}));

vi.mock('../../../utils/clipboard', () => ({
    copyToClipboard: vi.fn().mockResolvedValue(true),
}));

describe('GuardianReleaseModal Component', () => {
    const onClose = vi.fn();

    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('requires explicit selection among multiple wards and suppresses stale warning when fresh', async () => {
        vi.mocked(recoveryService.inspectGuardianRecoveryRequest).mockResolvedValue({
            sasCode: '482-910',
            requestedAt: 1700000000,
            isStale48h: false,
            nameHint: null,
            wards: [
                {
                    ownerDid: 'did:key:z6MkHans',
                    ownerDisplayName: 'Hans Müller',
                    rotationEpoch: 1,
                    thresholdM: 2,
                    totalN: 3,
                    updatedAt: 1700000000,
                },
                {
                    ownerDid: 'did:key:z6MkMaria',
                    ownerDisplayName: 'Maria Schmidt',
                    rotationEpoch: 1,
                    thresholdM: 2,
                    totalN: 3,
                    updatedAt: 1700000000,
                },
            ],
        });

        render(
            <GuardianReleaseModal
                isOpen
                onClose={onClose}
                initialRequestUri="humoco://recovery-req?v=1&pk=abc&ts=1700000000"
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('guardian-ward-select')).toBeInTheDocument();
        });

        // Fresh request must NOT show 48h stale warning
        expect(screen.queryByText(/older than 48 hours/i)).not.toBeInTheDocument();

        // Approve button stays disabled until user selects Hans explicitly
        expect(screen.getByTestId('guardian-ward-select')).toHaveValue('');
        const approveBtn = screen.getByTestId('guardian-release-approve-btn');
        expect(approveBtn).toBeDisabled();

        fireEvent.change(screen.getByTestId('guardian-ward-select'), {
            target: { value: 'did:key:z6MkHans' },
        });
        expect(screen.getByTestId('guardian-ward-select')).toHaveValue('did:key:z6MkHans');
    });

    it('shows stale warning only when isStale48h is true and allows inbox sync when vault is empty', async () => {
        vi.mocked(recoveryService.inspectGuardianRecoveryRequest)
            .mockResolvedValueOnce({
                sasCode: '999-111',
                requestedAt: 1600000000,
                isStale48h: true,
                nameHint: null,
                wards: [],
            })
            .mockResolvedValueOnce({
                sasCode: '999-111',
                requestedAt: 1600000000,
                isStale48h: true,
                nameHint: null,
                wards: [
                    {
                        ownerDid: 'did:key:z6MkHans',
                        ownerDisplayName: 'Hans Müller',
                        rotationEpoch: 1,
                        thresholdM: 2,
                        totalN: 3,
                        updatedAt: 1700000000,
                    },
                ],
            });
        vi.mocked(postService.syncPostChannels).mockResolvedValue({
            importedPayments: 0,
            importedSignatures: 0,
            importedContactCards: 0,
            importedWotUpdates: 0,
            importedRecoveryAssignments: 1,
            errors: [],
        } as never);

        render(<GuardianReleaseModal isOpen onClose={onClose} />);

        fireEvent.change(screen.getByTestId('guardian-request-uri-input'), {
            target: { value: 'Hallo Bernd!\nhumoco://recovery-req?v=1&pk=abc&ts=1600000000' },
        });
        fireEvent.click(screen.getByTestId('guardian-inspect-request-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('guardian-no-wards-banner')).toBeInTheDocument();
            expect(screen.getByText(/older than 48 hours/i)).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('sync-inbox-for-ward-btn'));

        await waitFor(() => {
            expect(postService.syncPostChannels).toHaveBeenCalledTimes(1);
            expect(screen.getByText('Hans Müller')).toBeInTheDocument();
        });
    });
});
