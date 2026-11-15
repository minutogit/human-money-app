import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SocialAndSheetRecoveryFlow } from '../SocialAndSheetRecoveryFlow';
import { recoveryService } from '../../services/recoveryService';

vi.mock('../../services/recoveryService', () => ({
    recoveryService: {
        getActiveRecoverySession: vi.fn(),
        startSocialRecoverySession: vi.fn(),
        cancelSocialRecoverySession: vi.fn(),
        importGuardianRecoveryReply: vi.fn(),
        completeSocialRecovery: vi.fn(),
        restoreFromEmergencyKitUri: vi.fn(),
    },
}));

describe('SocialAndSheetRecoveryFlow Component', () => {
    const mockOnSuccess = vi.fn();
    const mockOnCancel = vi.fn();

    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(null);
    });

    it('renders primary choice tiles (Social vs Emergency Sheet)', async () => {
        render(
            <SocialAndSheetRecoveryFlow
                mode="restore_new_device"
                onSuccess={mockOnSuccess}
                onCancel={mockOnCancel}
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('choose-social-recovery-btn')).toBeInTheDocument();
            expect(screen.getByTestId('choose-sheet-recovery-btn')).toBeInTheDocument();
        });
    });

    it('navigates to Emergency Sheet mode and restores with valid kit URI', async () => {
        vi.mocked(recoveryService.restoreFromEmergencyKitUri).mockResolvedValue({
            profileName: 'Recovered User',
            mode: 'restore_new_device',
            outcome: 'success',
            backupEpoch: 1,
            backupTxNonce: 1,
            pendingRestoreL2Reconciliation: false,
        });

        render(
            <SocialAndSheetRecoveryFlow
                mode="restore_new_device"
                onSuccess={mockOnSuccess}
                onCancel={mockOnCancel}
            />
        );

        const sheetBtn = await screen.findByTestId('choose-sheet-recovery-btn');
        fireEvent.click(sheetBtn);

        const kitInput = await screen.findByTestId('emergency-sheet-uri-input');
        fireEvent.change(kitInput, { target: { value: 'humoco://kit?did=did:key:z6Mku&words=12' } });

        const profileNameInput = screen.getByTestId('sheet-profile-name-input');
        fireEvent.change(profileNameInput, { target: { value: 'Recovered Profile' } });

        const newPwdInput = screen.getByTestId('sheet-new-password-input');
        fireEvent.change(newPwdInput, { target: { value: 'ValidPassword123' } });

        const confirmPwdInput = screen.getByTestId('sheet-confirm-password-input');
        fireEvent.change(confirmPwdInput, { target: { value: 'ValidPassword123' } });

        const submitBtn = screen.getByTestId('sheet-restore-submit-btn');
        fireEvent.click(submitBtn);

        await waitFor(() => {
            expect(recoveryService.restoreFromEmergencyKitUri).toHaveBeenCalled();
            expect(mockOnSuccess).toHaveBeenCalled();
        });
    });

    it('resumes active social recovery session automatically when present', async () => {
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue({
            sasCode: '482-910',
            createdAt: Date.now() / 1000,
            nameHint: 'Alice',
            requestUri: 'humoco://recovery-req?did=did:key:z6MkuOwner',
            ownerDid: 'did:key:z6MkuOwner',
            ownerDisplayName: 'Alice',
            rotationEpoch: 1,
            thresholdM: 2,
            collectedCount: 2,
            questionIds: [1, 2, 3],
            hasEnoughShares: true,
            discoveredSyncUrls: [],
        });

        render(
            <SocialAndSheetRecoveryFlow
                mode="restore_new_device"
                onSuccess={mockOnSuccess}
                onCancel={mockOnCancel}
            />
        );

        await waitFor(() => {
            expect(screen.getByText('482-910')).toBeInTheDocument();
        });
    });
});
