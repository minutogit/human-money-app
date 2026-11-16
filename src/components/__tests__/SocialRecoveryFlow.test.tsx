import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SocialRecoveryFlow } from '../SocialRecoveryFlow';
import { recoveryService } from '../../services/recoveryService';
import { authService } from '../../services/authService';
import { ActiveRecoverySessionDto } from '../../types';

vi.mock('../../services/recoveryService', () => ({
    recoveryService: {
        getActiveRecoverySession: vi.fn(),
        startSocialRecoverySession: vi.fn(),
        cancelSocialRecoverySession: vi.fn(),
        importGuardianRecoveryReply: vi.fn(),
        completeSocialRecovery: vi.fn(),
    },
}));

vi.mock('../../services/authService', () => ({
    authService: {
        getLocalInstanceId: vi.fn(),
    },
}));

const SESSION_WITH_SHARES: ActiveRecoverySessionDto = {
    sasCode: '482-910',
    createdAt: 1700000000,
    nameHint: 'Alice',
    requestUri: 'humoco://recovery-req?v=1&pk=abc',
    sessionMode: 'restore_new_device',
    targetFolderName: null,
    ownerDid: 'did:key:z6MkAlice',
    ownerDisplayName: 'Alice',
    rotationEpoch: 1,
    thresholdM: 2,
    collectedCount: 2,
    questionIds: [1, 2, 3],
    hasEnoughShares: true,
    discoveredSyncUrls: ['https://sync.humoco.org'],
};

describe('SocialRecoveryFlow Component', () => {
    const onSuccess = vi.fn();
    const onBack = vi.fn();

    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(null);
        vi.mocked(authService.getLocalInstanceId).mockResolvedValue('inst-1');
    });

    it('starts a recovery session with sessionMode and imports replies', async () => {
        vi.mocked(recoveryService.startSocialRecoverySession).mockResolvedValue({
            ...SESSION_WITH_SHARES,
            collectedCount: 0,
            hasEnoughShares: false,
            questionIds: null,
            ownerDid: null,
            ownerDisplayName: null,
            rotationEpoch: null,
            thresholdM: null,
        });
        vi.mocked(recoveryService.importGuardianRecoveryReply).mockResolvedValue(
            SESSION_WITH_SHARES
        );

        render(
            <SocialRecoveryFlow
                mode="reset_local_password"
                targetFolderName="alice_folder"
                onSuccess={onSuccess}
                onBack={onBack}
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('start-social-recovery-btn')).not.toBeDisabled();
        });
        expect(screen.queryByTestId('social-name-hint-input')).not.toBeInTheDocument();

        fireEvent.click(screen.getByTestId('start-social-recovery-btn'));

        await waitFor(() => {
            expect(recoveryService.startSocialRecoverySession).toHaveBeenCalledWith(
                undefined,
                'reset_local_password',
                'alice_folder'
            );
            expect(screen.getByTestId('social-sas-code')).toHaveTextContent('482-910');
        });

        fireEvent.change(screen.getByTestId('social-reply-input'), {
            target: { value: 'humoco://recovery-reply?v=1&data=xyz' },
        });
        fireEvent.click(screen.getByTestId('social-add-reply-btn'));

        await waitFor(() => {
            expect(recoveryService.importGuardianRecoveryReply).toHaveBeenCalledWith(
                'humoco://recovery-reply?v=1&data=xyz'
            );
            expect(screen.getByTestId('social-answer-0')).toBeInTheDocument();
        });
    });

    it('handles sync_empty_requires_confirm and explicit empty fallback confirmation', async () => {
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(
            SESSION_WITH_SHARES
        );
        vi.mocked(recoveryService.completeSocialRecovery)
            .mockResolvedValueOnce({
                folderName: '',
                profileName: 'Alice',
                mode: 'restore_new_device',
                outcome: 'sync_empty_requires_confirm',
                backupEpoch: null,
                backupTxNonce: null,
                activeVoucherQueriesCount: 0,
                reconciledSpentCount: 0,
                restoredStandardsCount: 0,
                l2ReconciliationStatus: 'no_servers',
                pendingRestoreL2Reconciliation: false,
                localCollision: null,
                resolvedSyncTargets: [
                    {
                        writeUrl: 'https://sync.humoco.org/sync.php',
                        readUrl: 'https://sync.humoco.org/data',
                        dropPub: 'dp1',
                        registerUrl: null,
                    },
                ],
            })
            .mockResolvedValueOnce({
                folderName: 'alice_new',
                profileName: 'Alice',
                mode: 'restore_new_device',
                outcome: 'restored_seed_only',
                backupEpoch: null,
                backupTxNonce: null,
                activeVoucherQueriesCount: 0,
                reconciledSpentCount: 0,
                restoredStandardsCount: 0,
                l2ReconciliationStatus: 'no_servers',
                pendingRestoreL2Reconciliation: false,
                localCollision: null,
                resolvedSyncTargets: [],
            });

        render(<SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} onBack={onBack} />);

        await waitFor(() => expect(screen.getByTestId('social-answer-0')).toBeInTheDocument());

        fireEvent.change(screen.getByTestId('social-answer-0'), { target: { value: 'Berlin' } });
        fireEvent.change(screen.getByTestId('social-answer-1'), { target: { value: 'Oma' } });
        fireEvent.change(screen.getByTestId('social-answer-2'), { target: { value: 'Bello' } });
        fireEvent.change(screen.getByTestId('social-profile-name-input'), {
            target: { value: 'Alice' },
        });
        fireEvent.change(screen.getByTestId('social-new-password-input'), {
            target: { value: 'NewSecretPass123' },
        });
        fireEvent.change(screen.getByTestId('social-confirm-password-input'), {
            target: { value: 'NewSecretPass123' },
        });

        fireEvent.click(screen.getByTestId('social-complete-submit-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('social-sync-empty-confirm')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('social-confirm-empty-btn'));

        await waitFor(() => {
            expect(recoveryService.completeSocialRecovery).toHaveBeenLastCalledWith(
                expect.objectContaining({
                    allowEmptyFallback: true,
                })
            );
            expect(onSuccess).toHaveBeenCalledWith(
                expect.objectContaining({ outcome: 'restored_seed_only' })
            );
        });
    });

    it('handles collision_requires_choice with replace option', async () => {
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(
            SESSION_WITH_SHARES
        );
        vi.mocked(recoveryService.completeSocialRecovery)
            .mockResolvedValueOnce({
                folderName: '',
                profileName: 'Alice',
                mode: 'restore_new_device',
                outcome: 'collision_requires_choice',
                backupEpoch: 3,
                backupTxNonce: 10,
                activeVoucherQueriesCount: 0,
                reconciledSpentCount: 0,
                restoredStandardsCount: 0,
                l2ReconciliationStatus: 'no_servers',
                pendingRestoreL2Reconciliation: false,
                localCollision: {
                    folderName: 'alice_old',
                    profileName: 'Alice',
                    userId: 'did:key:z6MkAlice',
                    localEpoch: 1,
                    localTxNonce: 2,
                    isLockedOrConflicted: false,
                },
                resolvedSyncTargets: [],
            })
            .mockResolvedValueOnce({
                folderName: 'alice_new',
                profileName: 'Alice',
                mode: 'restore_new_device',
                outcome: 'restored_from_sync',
                backupEpoch: 3,
                backupTxNonce: 10,
                activeVoucherQueriesCount: 2,
                reconciledSpentCount: 0,
                restoredStandardsCount: 1,
                l2ReconciliationStatus: 'reconciled',
                pendingRestoreL2Reconciliation: false,
                localCollision: null,
                resolvedSyncTargets: [],
            });

        render(<SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} />);

        await waitFor(() => expect(screen.getByTestId('social-answer-0')).toBeInTheDocument());

        fireEvent.change(screen.getByTestId('social-answer-0'), { target: { value: 'Berlin' } });
        fireEvent.change(screen.getByTestId('social-answer-1'), { target: { value: 'Oma' } });
        fireEvent.change(screen.getByTestId('social-answer-2'), { target: { value: 'Bello' } });
        fireEvent.change(screen.getByTestId('social-profile-name-input'), {
            target: { value: 'Alice' },
        });
        fireEvent.change(screen.getByTestId('social-new-password-input'), {
            target: { value: 'NewSecretPass123' },
        });
        fireEvent.change(screen.getByTestId('social-confirm-password-input'), {
            target: { value: 'NewSecretPass123' },
        });

        fireEvent.click(screen.getByTestId('social-complete-submit-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('social-collision-card')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('social-collision-replace-btn'));

        await waitFor(() => {
            expect(recoveryService.completeSocialRecovery).toHaveBeenLastCalledWith(
                expect.objectContaining({
                    replaceFolderName: 'alice_old',
                })
            );
            expect(onSuccess).toHaveBeenCalledWith(
                expect.objectContaining({ outcome: 'restored_from_sync' })
            );
        });
    });
});
