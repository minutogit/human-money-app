import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SocialRecoveryFlow } from '../SocialRecoveryFlow';
import { recoveryService } from '../../../services/recoveryService';
import { profileService } from '../../../services/profileService';
import { authService } from '../../../services/authService';
import { copyToClipboard } from '../../../utils/clipboard';
import { ActiveRecoverySessionDto } from '../../../types';

vi.mock('../../../utils/clipboard', () => ({
    copyToClipboard: vi.fn().mockResolvedValue(true),
}));

vi.mock('../../../services/recoveryService', () => ({
    recoveryService: {
        getActiveRecoverySession: vi.fn(),
        startSocialRecoverySession: vi.fn(),
        cancelSocialRecoverySession: vi.fn(),
        importGuardianRecoveryReply: vi.fn(),
        completeSocialRecovery: vi.fn(),
    },
}));

vi.mock('../../../services/profileService', () => ({
    profileService: {
        encodeTextQrSvg: vi.fn(),
    },
}));

vi.mock('../../../services/authService', () => ({
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
        vi.resetAllMocks();
        vi.mocked(copyToClipboard).mockResolvedValue(true);
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
        expect(onSuccess).not.toHaveBeenCalled();

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
        expect(onSuccess).not.toHaveBeenCalled();

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

    it('supports chat message copy, pause across days, duplicate share feedback, and confirmed discard', async () => {
        const activeSession: ActiveRecoverySessionDto = {
            ...SESSION_WITH_SHARES,
            thresholdM: 2,
            totalN: 3,
            collectedCount: 1,
            collectedShareIndices: [2],
            hasEnoughShares: false,
            questionIds: null,
        };
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(activeSession);
        vi.mocked(recoveryService.importGuardianRecoveryReply).mockResolvedValue({
            ...activeSession,
            lastImportStatus: 'duplicate',
        });
        vi.mocked(recoveryService.cancelSocialRecoverySession).mockResolvedValue(undefined);

        render(<SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} onBack={onBack} />);

        await waitFor(() => {
            expect(screen.getByTestId('collected-share-badge-2')).toBeInTheDocument();
            expect(screen.getByTestId('social-discovered-sync-hint')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('copy-social-chat-msg-btn'));
        await waitFor(() => {
            expect(copyToClipboard).toHaveBeenCalledWith(
                expect.stringContaining('humoco://recovery-req?v=1&pk=abc')
            );
            expect(copyToClipboard).toHaveBeenCalledWith(expect.stringContaining('482-910'));
        });

        // Duplicate import shows notice instead of crashing
        fireEvent.change(screen.getByTestId('social-reply-input'), {
            target: { value: 'humoco://recovery-reply?v=1&data=dup' },
        });
        fireEvent.click(screen.getByTestId('social-add-reply-btn'));
        await waitFor(() => {
            expect(screen.getByText(/already been imported/i)).toBeInTheDocument();
        });

        // Pause button calls onBack without canceling session
        fireEvent.click(screen.getByTestId('pause-social-session-btn'));
        expect(onBack).toHaveBeenCalled();
        expect(recoveryService.cancelSocialRecoverySession).not.toHaveBeenCalled();

        // Two-step cancel confirmation
        fireEvent.click(screen.getByTestId('request-cancel-session-btn'));
        fireEvent.click(screen.getByTestId('confirm-cancel-session-btn'));
        await waitFor(() => {
            expect(recoveryService.cancelSocialRecoverySession).toHaveBeenCalledTimes(1);
        });
    });

    it('treats superseded_epoch as informational newer-epoch replacement instead of an error', async () => {
        const activeSession: ActiveRecoverySessionDto = {
            ...SESSION_WITH_SHARES,
            thresholdM: 2,
            totalN: 3,
            collectedCount: 1,
            collectedShareIndices: [1],
            hasEnoughShares: false,
            questionIds: null,
        };
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(activeSession);
        vi.mocked(recoveryService.importGuardianRecoveryReply).mockResolvedValue({
            ...activeSession,
            lastImportStatus: 'superseded_epoch',
        });

        render(<SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} onBack={onBack} />);

        await waitFor(() => {
            expect(screen.getByTestId('social-reply-input')).toBeInTheDocument();
        });

        fireEvent.change(screen.getByTestId('social-reply-input'), {
            target: { value: 'humoco://recovery-reply?v=1&data=newerEpoch' },
        });
        fireEvent.click(screen.getByTestId('social-add-reply-btn'));

        await waitFor(() => {
            expect(screen.getByText(/newer recovery contact epoch|neuerer Vertrauenskontakt-Stand/i)).toBeInTheDocument();
        });
    });

    it('rejects recovery-req and kit links pasted into reply input without calling backend', async () => {
        const waitingSession: ActiveRecoverySessionDto = {
            ...SESSION_WITH_SHARES,
            collectedCount: 0,
            hasEnoughShares: false,
            questionIds: null,
        };
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(waitingSession);

        render(<SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} />);

        await waitFor(() => {
            expect(screen.getByTestId('social-reply-input')).toBeInTheDocument();
        });

        fireEvent.change(screen.getByTestId('social-reply-input'), {
            target: { value: 'humoco://recovery-req?v=1&pk=abc' },
        });
        fireEvent.click(screen.getByTestId('social-add-reply-btn'));
        expect(
            screen.getByText(
                'This is your own request link (recovery-req…), not a reply from a recovery contact.'
            )
        ).toBeInTheDocument();
        expect(recoveryService.importGuardianRecoveryReply).not.toHaveBeenCalled();

        fireEvent.change(screen.getByTestId('social-reply-input'), {
            target: { value: 'humoco://kit?v=1&ent=abc' },
        });
        fireEvent.click(screen.getByTestId('social-add-reply-btn'));
        expect(
            screen.getByText(
                'This is an emergency sheet link (humoco://kit…), not a recovery contact reply.'
            )
        ).toBeInTheDocument();
        expect(recoveryService.importGuardianRecoveryReply).not.toHaveBeenCalled();
    });

    it('disables completion submit until all 3 answers, profile name, and valid matching passwords are provided', async () => {
        const sessionWithoutName: ActiveRecoverySessionDto = {
            ...SESSION_WITH_SHARES,
            ownerDisplayName: null,
        };
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(sessionWithoutName);

        render(<SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} />);

        await waitFor(() => {
            expect(screen.getByTestId('social-complete-submit-btn')).toBeInTheDocument();
        });

        const submitBtn = screen.getByTestId('social-complete-submit-btn');

        // Only 2 answers + profile name + valid matching password -> disabled
        fireEvent.change(screen.getByTestId('social-answer-0'), { target: { value: 'Berlin' } });
        fireEvent.change(screen.getByTestId('social-answer-1'), { target: { value: 'Oma' } });
        fireEvent.change(screen.getByTestId('social-profile-name-input'), {
            target: { value: 'Alice' },
        });
        fireEvent.change(screen.getByTestId('social-new-password-input'), {
            target: { value: 'StrongPass123' },
        });
        fireEvent.change(screen.getByTestId('social-confirm-password-input'), {
            target: { value: 'StrongPass123' },
        });
        expect(submitBtn).toBeDisabled();

        // Fill 3rd answer, clear profile name -> disabled
        fireEvent.change(screen.getByTestId('social-answer-2'), { target: { value: 'Bello' } });
        fireEvent.change(screen.getByTestId('social-profile-name-input'), { target: { value: '' } });
        expect(submitBtn).toBeDisabled();

        // Restore profile name, short password -> disabled
        fireEvent.change(screen.getByTestId('social-profile-name-input'), {
            target: { value: 'Alice' },
        });
        fireEvent.change(screen.getByTestId('social-new-password-input'), {
            target: { value: 'Short1!' },
        });
        fireEvent.change(screen.getByTestId('social-confirm-password-input'), {
            target: { value: 'Short1!' },
        });
        expect(submitBtn).toBeDisabled();

        // Mismatched passwords -> disabled
        fireEvent.change(screen.getByTestId('social-new-password-input'), {
            target: { value: 'StrongPass123' },
        });
        fireEvent.change(screen.getByTestId('social-confirm-password-input'), {
            target: { value: 'Different123' },
        });
        expect(submitBtn).toBeDisabled();

        // Positive baseline: matching valid passwords -> enabled
        fireEvent.change(screen.getByTestId('social-confirm-password-input'), {
            target: { value: 'StrongPass123' },
        });
        expect(submitBtn).not.toBeDisabled();
        expect(recoveryService.completeSocialRecovery).not.toHaveBeenCalled();
    });

    it('reveals BIP-39 passphrase input on bip39PassphraseRequired error and displays wrongAnswersRetry without calling onSuccess', async () => {
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(
            SESSION_WITH_SHARES
        );
        vi.mocked(recoveryService.completeSocialRecovery)
            .mockRejectedValueOnce({
                code: 'error.recovery.bip39PassphraseRequired',
                message: 'Passphrase required',
            })
            .mockRejectedValueOnce({
                code: 'error.recovery.wrongAnswers',
                message: 'Wrong answers',
            });

        render(<SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} />);

        await waitFor(() => expect(screen.getByTestId('social-answer-0')).toBeInTheDocument());

        fireEvent.change(screen.getByTestId('social-answer-0'), { target: { value: 'Berlin' } });
        fireEvent.change(screen.getByTestId('social-answer-1'), { target: { value: 'Oma' } });
        fireEvent.change(screen.getByTestId('social-answer-2'), { target: { value: 'Bello' } });
        fireEvent.change(screen.getByTestId('social-new-password-input'), {
            target: { value: 'StrongPass123' },
        });
        fireEvent.change(screen.getByTestId('social-confirm-password-input'), {
            target: { value: 'StrongPass123' },
        });
        fireEvent.click(screen.getByTestId('social-complete-submit-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('social-passphrase-input')).toBeInTheDocument();
            expect(
                screen.getAllByText(
                    'An additional passphrase (25th word) was set when creating this profile. Please enter it here:'
                )
            ).toHaveLength(2);
        });
        expect(onSuccess).not.toHaveBeenCalled();

        fireEvent.change(screen.getByTestId('social-passphrase-input'), {
            target: { value: 'my25thword' },
        });
        fireEvent.click(screen.getByTestId('social-complete-submit-btn'));

        await waitFor(() => {
            expect(recoveryService.completeSocialRecovery).toHaveBeenLastCalledWith(
                expect.objectContaining({
                    bip39Passphrase: 'my25thword',
                })
            );
            expect(
                screen.getByText(
                    'Your answers do not match yet – please check carefully and try again.'
                )
            ).toBeInTheDocument();
        });
        expect(onSuccess).not.toHaveBeenCalled();
    });

    it('handles syncUnreachable error by showing warning and completing only after explicit empty fallback', async () => {
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(
            SESSION_WITH_SHARES
        );
        vi.mocked(recoveryService.completeSocialRecovery)
            .mockRejectedValueOnce({
                code: 'error.recovery.syncUnreachable',
                message: 'Sync offline',
            })
            .mockResolvedValueOnce({
                folderName: 'alice_empty',
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

        render(<SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} />);

        await waitFor(() => expect(screen.getByTestId('social-answer-0')).toBeInTheDocument());

        fireEvent.change(screen.getByTestId('social-answer-0'), { target: { value: 'Berlin' } });
        fireEvent.change(screen.getByTestId('social-answer-1'), { target: { value: 'Oma' } });
        fireEvent.change(screen.getByTestId('social-answer-2'), { target: { value: 'Bello' } });
        fireEvent.change(screen.getByTestId('social-new-password-input'), {
            target: { value: 'StrongPass123' },
        });
        fireEvent.change(screen.getByTestId('social-confirm-password-input'), {
            target: { value: 'StrongPass123' },
        });
        fireEvent.click(screen.getByTestId('social-complete-submit-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('social-unreachable-warning')).toBeInTheDocument();
        });
        expect(onSuccess).not.toHaveBeenCalled();

        fireEvent.click(screen.getByTestId('social-unreachable-empty-btn'));

        await waitFor(() => {
            expect(recoveryService.completeSocialRecovery).toHaveBeenLastCalledWith(
                expect.objectContaining({
                    allowEmptyFallback: true,
                })
            );
            expect(onSuccess).toHaveBeenCalledTimes(1);
        });
    });

    it('renders sanitized request QR SVG when toggling show QR code', async () => {
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(
            SESSION_WITH_SHARES
        );
        vi.mocked(profileService.encodeTextQrSvg).mockResolvedValue(
            '<svg viewBox="0 0 10 10"><rect width="99" height="99"/></svg>'
        );

        render(<SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} />);

        await waitFor(() => {
            expect(screen.getByTestId('social-toggle-request-qr-btn')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('social-toggle-request-qr-btn'));

        await waitFor(() => {
            expect(profileService.encodeTextQrSvg).toHaveBeenCalledWith(
                'humoco://recovery-req?v=1&pk=abc'
            );
            expect(
                screen.getByTestId('social-request-qr').querySelector('svg rect[width="99"]')
            ).not.toBeNull();
        });
    });

    it('sanitizes/rejects unsafe requestQrSvg markup instead of injecting raw HTML', async () => {
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(
            SESSION_WITH_SHARES
        );
        vi.mocked(profileService.encodeTextQrSvg).mockResolvedValue(
            '<svg onload="alert(1)"><script>alert(1)</script><rect width="99" height="99"/></svg>'
        );

        const { container } = render(
            <SocialRecoveryFlow mode="restore_new_device" onSuccess={onSuccess} />
        );

        await waitFor(() => {
            expect(screen.getByTestId('social-toggle-request-qr-btn')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('social-toggle-request-qr-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('social-toggle-request-qr-btn')).toHaveTextContent(
                /Hide QR|QR-Code verbergen/i
            );
        });

        expect(container.querySelector('script')).toBeNull();
        expect(container.innerHTML).not.toContain('onload=');
        expect(screen.getByTestId('social-request-qr')).toBeInTheDocument();
        expect(screen.getByTestId('social-request-qr').querySelector('rect[width="99"]')).toBeNull();
    });
});
