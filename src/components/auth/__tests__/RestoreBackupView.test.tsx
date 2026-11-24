import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { RestoreBackupView } from '../RestoreBackupView';
import { backupService } from '../../../services/backupService';
import { recoveryService } from '../../../services/recoveryService';
import { profileService } from '../../../services/profileService';
import { authService } from '../../../services/authService';
import { open } from '@tauri-apps/plugin-dialog';

vi.mock('../../../services/backupService', () => ({
    backupService: {
        probeBackup: vi.fn(),
        inspectBackup: vi.fn(),
        restoreBackup: vi.fn(),
        resetLocalPasswordViaBackup: vi.fn(),
        previewMnemonicIdentity: vi.fn(),
        stageDroppedBackupBytes: vi.fn(),
        probeSyncBackup: vi.fn(),
        reconcileRestoredWalletL2: vi.fn(),
    },
}));

vi.mock('../../../services/recoveryService', () => ({
    recoveryService: {
        getActiveRecoverySession: vi.fn(),
        inspectEmergencyKitUri: vi.fn(),
        startSocialRecoverySession: vi.fn(),
        cancelSocialRecoverySession: vi.fn(),
        importGuardianRecoveryReply: vi.fn(),
        completeSocialRecovery: vi.fn(),
    },
}));

vi.mock('../../../services/profileService', () => ({
    profileService: {
        getWordlist: vi.fn(),
        validateMnemonic: vi.fn(),
        createProfile: vi.fn(),
        recoverWallet: vi.fn(),
    },
}));

vi.mock('../../../services/authService', () => ({
    authService: {
        getLocalInstanceId: vi.fn(),
        listProfiles: vi.fn().mockResolvedValue([]),
    },
}));

vi.mock('../../../utils/log', () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn(),
        debug: vi.fn(),
    },
}));

const VALID_12 =
    'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';

describe('RestoreBackupView', () => {
    const onRestoreSuccess = vi.fn();
    const onBack = vi.fn();

    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue(null);
        vi.mocked(profileService.getWordlist).mockResolvedValue(['abandon', 'about']);
        vi.mocked(profileService.validateMnemonic).mockResolvedValue(undefined);
        vi.mocked(authService.getLocalInstanceId).mockResolvedValue('device-uuid-123');
    });

    it('renders 3 equal method cards and active social session banner when present', async () => {
        vi.mocked(recoveryService.getActiveRecoverySession).mockResolvedValue({
            sasCode: '123-456',
            createdAt: 1700000000,
            nameHint: 'Alice',
            requestUri: 'humoco://recovery-req?v=1&pk=abc',
            sessionMode: 'restore_new_device',
            targetFolderName: null,
            ownerDid: null,
            ownerDisplayName: null,
            rotationEpoch: null,
            thresholdM: 2,
            collectedCount: 1,
            questionIds: null,
            hasEnoughShares: false,
            discoveredSyncUrls: [],
        });

        render(<RestoreBackupView onRestoreSuccess={onRestoreSuccess} onBack={onBack} />);

        await waitFor(() => {
            expect(screen.getByTestId('resume-social-session-banner')).toBeInTheDocument();
        });

        expect(screen.getByTestId('choose-social-recovery-btn')).toBeInTheDocument();
        expect(screen.getByTestId('choose-words-or-sheet-btn')).toBeInTheDocument();
        expect(screen.getByTestId('choose-file-or-folder-btn')).toBeInTheDocument();

        fireEvent.click(screen.getByTestId('restore-back-btn').querySelector('button')!);
        expect(onBack).toHaveBeenCalled();
    });

    it('opens a profile directly from words without sync address (no backup path)', async () => {
        vi.mocked(backupService.previewMnemonicIdentity).mockResolvedValue({
            userId: 'did:key:z6MkAlice',
            localCollision: null,
        });
        vi.mocked(profileService.createProfile).mockResolvedValue(undefined);

        render(<RestoreBackupView onRestoreSuccess={onRestoreSuccess} onBack={onBack} />);

        fireEvent.click(screen.getByTestId('choose-words-or-sheet-btn'));

        fireEvent.change(screen.getByTestId('restore-words-word-0'), {
            target: { value: VALID_12 },
        });

        await waitFor(() => {
            expect(screen.getByTestId('words-continue-btn')).not.toBeDisabled();
        });

        fireEvent.click(screen.getByTestId('words-continue-btn'));

        await waitFor(() => {
            expect(backupService.previewMnemonicIdentity).toHaveBeenCalled();
            expect(screen.getByTestId('direct-profile-name-input')).toBeInTheDocument();
        });

        fireEvent.change(screen.getByTestId('direct-profile-name-input'), {
            target: { value: 'Alice Ohne Backup' },
        });
        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'StrongPass123!' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'StrongPass123!' },
        });

        fireEvent.click(screen.getByTestId('direct-open-profile-submit-btn'));

        await waitFor(() => {
            expect(profileService.createProfile).toHaveBeenCalledWith(
                expect.objectContaining({
                    profileName: 'Alice Ohne Backup',
                    mnemonic: VALID_12,
                    initialReachPreset: 'medium',
                    initialSyncTargets: undefined,
                })
            );
            expect(onRestoreSuccess).toHaveBeenCalled();
        });
    });

    it('handles subaccount creation with userPrefix and sync not_found adopting initialSyncTargets', async () => {
        vi.mocked(backupService.probeSyncBackup).mockResolvedValue({
            status: 'not_found',
            stagedPath: null,
            matchedKeyKind: null,
            sourceUrl: null,
            inspection: null,
            userId: 'mobil:did:key:z6MkAlice',
            localCollision: null,
            resolvedSyncTargets: [
                {
                    writeUrl: 'https://sync.humoco.org/sync.php',
                    readUrl: 'https://sync.humoco.org/data',
                    dropPub: 'dp_1',
                    registerUrl: null,
                },
            ],
            error: null,
        });
        vi.mocked(profileService.createProfile).mockResolvedValue(undefined);

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onBack={onBack}
                initialMethod="words_or_sheet"
            />
        );

        fireEvent.change(screen.getByTestId('restore-words-word-0'), {
            target: { value: VALID_12 },
        });

        fireEvent.click(screen.getByTestId('seed-advanced-toggle'));
        fireEvent.change(screen.getByTestId('seed-user-prefix-input'), {
            target: { value: 'mobil' },
        });
        fireEvent.change(screen.getByTestId('restore-sync-input'), {
            target: { value: 'https://sync.humoco.org/sync.php' },
        });

        await waitFor(() => {
            expect(screen.getByTestId('words-continue-btn')).not.toBeDisabled();
        });
        fireEvent.click(screen.getByTestId('words-continue-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('sync-not-found-banner')).toBeInTheDocument();
        });

        fireEvent.change(screen.getByTestId('direct-profile-name-input'), {
            target: { value: 'Alice Mobil' },
        });
        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'StrongPass123!' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'StrongPass123!' },
        });
        fireEvent.click(screen.getByTestId('direct-open-profile-submit-btn'));

        await waitFor(() => {
            expect(profileService.createProfile).toHaveBeenCalledWith(
                expect.objectContaining({
                    profileName: 'Alice Mobil',
                    userPrefix: 'mobil',
                    initialSyncTargets: [
                        expect.objectContaining({ writeUrl: 'https://sync.humoco.org/sync.php' }),
                    ],
                })
            );
            expect(onRestoreSuccess).toHaveBeenCalled();
        });
    });

    it('restores from sync service when backup is found and reconciles L2', async () => {
        vi.mocked(backupService.probeSyncBackup).mockResolvedValue({
            status: 'found',
            stagedPath: '/tmp/sync_restore.hmcbackup',
            matchedKeyKind: 'stealth',
            sourceUrl: 'https://drop.example.org/data',
            inspection: {
                profileName: 'Bob',
                userId: 'did:key:z6MkBob',
                epoch: 2,
                txNonce: 9,
                sealTimestamp: '2026-11-01T10:00:00Z',
                blobCount: 10,
                customStandardCount: 1,
                protectionMode: 'mnemonic_only',
                localCollision: null,
            },
            userId: 'did:key:z6MkBob',
            localCollision: null,
            resolvedSyncTargets: [],
            error: null,
        });
        vi.mocked(backupService.restoreBackup).mockResolvedValue({
            folderName: 'profile_bob',
            profileName: 'Bob',
            activeVoucherQueriesCount: 2,
            reconciledSpentCount: 0,
            restoredStandardsCount: 1,
            l2ReconciliationStatus: 'offline',
        });
        vi.mocked(backupService.reconcileRestoredWalletL2).mockResolvedValue({
            activeVoucherQueriesCount: 2,
            reconciledSpentCount: 0,
            l2ReconciliationStatus: 'reconciled',
            pendingRestoreL2Reconciliation: false,
        });

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onBack={onBack}
                initialMethod="words_or_sheet"
            />
        );

        fireEvent.change(screen.getByTestId('restore-words-word-0'), {
            target: { value: VALID_12 },
        });
        fireEvent.change(screen.getByTestId('restore-sync-input'), {
            target: { value: 'https://drop.example.org/sync.php' },
        });

        await waitFor(() => expect(screen.getByTestId('words-continue-btn')).not.toBeDisabled());
        fireEvent.click(screen.getByTestId('words-continue-btn'));

        await waitFor(() => {
            expect(screen.getByText('Verified Backup Manifest')).toBeInTheDocument();
            expect(screen.getByText('Bob')).toBeInTheDocument();
        });

        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'DevicePass456!' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'DevicePass456!' },
        });
        fireEvent.click(screen.getByTestId('restore-submit-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('restore-reconcile-l2-btn')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('restore-reconcile-l2-btn'));

        await waitFor(() => {
            expect(backupService.reconcileRestoredWalletL2).toHaveBeenCalled();
            expect(
                screen.getByText(/Online security reconciliation completed successfully/i)
            ).toBeInTheDocument();
        });
    });

    it('blocks silently creating an empty profile on unreachable sync and offers retry or explicit bypass', async () => {
        vi.mocked(backupService.probeSyncBackup).mockResolvedValue({
            status: 'unreachable',
            stagedPath: null,
            matchedKeyKind: null,
            sourceUrl: null,
            inspection: null,
            userId: 'did:key:z6MkAlice',
            localCollision: null,
            resolvedSyncTargets: [],
            error: 'Connection refused',
        });

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onBack={onBack}
                initialMethod="words_or_sheet"
            />
        );

        fireEvent.change(screen.getByTestId('restore-words-word-0'), {
            target: { value: VALID_12 },
        });
        fireEvent.change(screen.getByTestId('restore-sync-input'), {
            target: { value: 'https://down.example.org/sync.php' },
        });

        await waitFor(() => expect(screen.getByTestId('words-continue-btn')).not.toBeDisabled());
        fireEvent.click(screen.getByTestId('words-continue-btn'));

        await waitFor(() => {
            expect(backupService.probeSyncBackup).toHaveBeenCalledTimes(1);
            expect(screen.getByTestId('sync-unreachable-card')).toBeInTheDocument();
            expect(screen.getByTestId('retry-sync-probe-btn')).toBeInTheDocument();
            expect(screen.getByTestId('proceed-without-backup-btn')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('retry-sync-probe-btn'));
        await waitFor(() => {
            expect(backupService.probeSyncBackup).toHaveBeenCalledTimes(2);
            expect(backupService.probeSyncBackup).toHaveBeenLastCalledWith(
                expect.objectContaining({
                    syncInputs: ['https://down.example.org/sync.php'],
                })
            );
        });
        expect(profileService.createProfile).not.toHaveBeenCalled();

        fireEvent.click(screen.getByTestId('proceed-without-backup-btn'));
        expect(screen.getByTestId('direct-profile-name-input')).toBeInTheDocument();
    });

    it('shows Smart Local Collision Card when profile exists locally and defaults to reset_pw when local state is newer', async () => {
        vi.mocked(open).mockResolvedValue('/tmp/my-backup.hmcbackup');
        vi.mocked(backupService.probeBackup).mockResolvedValue({
            sourceKind: 'file',
            protectionMode: 'custom_password',
            supportsPasswordRestore: true,
            supportsMnemonicRestore: true,
            blobCount: 15,
            customStandardCount: 1,
        });
        vi.mocked(backupService.inspectBackup).mockResolvedValue({
            profileName: 'Alice',
            userId: 'did:key:z6MkAlice',
            epoch: 2,
            txNonce: 10,
            sealTimestamp: '2026-10-30T10:00:00Z',
            blobCount: 15,
            customStandardCount: 1,
            protectionMode: 'custom_password',
            localCollision: {
                folderName: 'alice_local',
                profileName: 'Alice',
                userId: 'did:key:z6MkAlice',
                localEpoch: 3,
                localTxNonce: 12,
                isLockedOrConflicted: false,
            },
        });
        vi.mocked(backupService.resetLocalPasswordViaBackup).mockResolvedValue(undefined);

        render(<RestoreBackupView onRestoreSuccess={onRestoreSuccess} onBack={onBack} />);

        fireEvent.click(screen.getByTestId('choose-file-or-folder-btn'));
        fireEvent.click(screen.getByTestId('select-backup-file-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('backup-password-input')).toBeInTheDocument();
        });

        fireEvent.change(screen.getByTestId('backup-password-input'), {
            target: { value: 'backupSecret123' },
        });
        fireEvent.click(screen.getByTestId('inspect-file-backup-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('local-collision-card')).toBeInTheDocument();
            expect(screen.getByTestId('collision-choice-reset-pw')).toBeInTheDocument();
            expect(screen.getByTestId('collision-choice-replace')).toBeInTheDocument();
        });

        // Default is reset_pw because localEpoch (3) > backupEpoch (2)
        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'NewLocalPass123' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'NewLocalPass123' },
        });
        fireEvent.click(screen.getByTestId('restore-submit-btn'));

        await waitFor(() => {
            expect(backupService.resetLocalPasswordViaBackup).toHaveBeenCalledWith(
                expect.objectContaining({
                    targetFolderName: 'alice_local',
                    sourcePath: '/tmp/my-backup.hmcbackup',
                    credentialMode: 'password',
                    backupPassword: 'backupSecret123',
                    newPassword: 'NewLocalPass123',
                })
            );
            expect(backupService.restoreBackup).not.toHaveBeenCalled();
            expect(onRestoreSuccess).toHaveBeenCalled();
        });
    });

    it('allows switching collision without backup to subaccount setup', async () => {
        vi.mocked(backupService.previewMnemonicIdentity).mockResolvedValue({
            userId: 'did:key:z6MkAlice',
            localCollision: {
                folderName: 'alice_local',
                profileName: 'Alice',
                userId: 'did:key:z6MkAlice',
                localEpoch: 0,
                localTxNonce: 1,
                isLockedOrConflicted: false,
            },
        });

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onBack={onBack}
                initialMethod="words_or_sheet"
            />
        );

        fireEvent.change(screen.getByTestId('restore-words-word-0'), {
            target: { value: VALID_12 },
        });
        await waitFor(() => expect(screen.getByTestId('words-continue-btn')).not.toBeDisabled());
        fireEvent.click(screen.getByTestId('words-continue-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('local-collision-card')).toBeInTheDocument();
            expect(screen.getByTestId('collision-create-subaccount-btn')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('collision-create-subaccount-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('seed-user-prefix-input')).toBeInTheDocument();
        });
    });

    it('shows missing sync hint in direct words setup and allows switching to contact recovery', async () => {
        vi.mocked(backupService.previewMnemonicIdentity).mockResolvedValue({
            userId: 'did:key:z6MkAlice',
            localCollision: null,
        });

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onBack={onBack}
                initialMethod="words_or_sheet"
            />
        );

        fireEvent.change(screen.getByTestId('restore-words-word-0'), {
            target: { value: VALID_12 },
        });
        await waitFor(() => expect(screen.getByTestId('words-continue-btn')).not.toBeDisabled());
        fireEvent.click(screen.getByTestId('words-continue-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('direct-open-no-sync-hint')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('switch-to-social-from-words-btn'));
        await waitFor(() => {
            expect(screen.getByTestId('start-social-recovery-btn')).toBeInTheDocument();
        });
    });

    it('handles Smart Local Collision replace choice by calling restoreBackup with replaceFolderName', async () => {
        vi.mocked(open).mockResolvedValue('/tmp/my-backup.hmcbackup');
        vi.mocked(backupService.probeBackup).mockResolvedValue({
            sourceKind: 'file',
            protectionMode: 'custom_password',
            supportsPasswordRestore: true,
            supportsMnemonicRestore: true,
            blobCount: 15,
            customStandardCount: 1,
        });
        vi.mocked(backupService.inspectBackup).mockResolvedValue({
            profileName: 'Alice',
            userId: 'did:key:z6MkAlice',
            epoch: 2,
            txNonce: 10,
            sealTimestamp: '2026-10-30T10:00:00Z',
            blobCount: 15,
            customStandardCount: 1,
            protectionMode: 'custom_password',
            localCollision: {
                folderName: 'alice_local',
                profileName: 'Alice',
                userId: 'did:key:z6MkAlice',
                localEpoch: 3,
                localTxNonce: 12,
                isLockedOrConflicted: false,
            },
        });
        vi.mocked(backupService.restoreBackup).mockResolvedValue({
            folderName: 'alice_local',
            profileName: 'Alice',
            activeVoucherQueriesCount: 1,
            reconciledSpentCount: 0,
            restoredStandardsCount: 1,
            l2ReconciliationStatus: 'reconciled',
        });

        render(<RestoreBackupView onRestoreSuccess={onRestoreSuccess} onBack={onBack} />);

        fireEvent.click(screen.getByTestId('choose-file-or-folder-btn'));
        fireEvent.click(screen.getByTestId('select-backup-file-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('backup-password-input')).toBeInTheDocument();
        });

        fireEvent.change(screen.getByTestId('backup-password-input'), {
            target: { value: 'backupSecret123' },
        });
        fireEvent.click(screen.getByTestId('inspect-file-backup-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('collision-choice-replace')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('collision-choice-replace'));
        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'NewLocalPass123' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'NewLocalPass123' },
        });
        fireEvent.click(screen.getByTestId('restore-submit-btn'));

        await waitFor(() => {
            expect(backupService.restoreBackup).toHaveBeenCalledWith(
                expect.objectContaining({
                    sourcePath: '/tmp/my-backup.hmcbackup',
                    replaceFolderName: 'alice_local',
                    credentialMode: 'password',
                    backupPassword: 'backupSecret123',
                    newPassword: 'NewLocalPass123',
                })
            );
            expect(screen.getByText('Profile Successfully Restored!')).toBeInTheDocument();
            expect(screen.getByRole('button', { name: 'Go to My Vouchers' })).toBeInTheDocument();
        });
        expect(backupService.resetLocalPasswordViaBackup).not.toHaveBeenCalled();
    });

    it('validates missing profile name, short password, and mismatched passwords before calling createProfile in direct open flow', async () => {
        vi.mocked(backupService.previewMnemonicIdentity).mockResolvedValue({
            userId: 'did:key:z6MkAlice',
            localCollision: null,
        });

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onBack={onBack}
                initialMethod="words_or_sheet"
            />
        );

        fireEvent.change(screen.getByTestId('restore-words-word-0'), {
            target: { value: VALID_12 },
        });
        await waitFor(() => expect(screen.getByTestId('words-continue-btn')).not.toBeDisabled());
        fireEvent.click(screen.getByTestId('words-continue-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('direct-profile-name-input')).toBeInTheDocument();
        });

        // Empty profile name
        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'StrongPass123!' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'StrongPass123!' },
        });
        fireEvent.click(screen.getByTestId('direct-open-profile-submit-btn'));
        expect(screen.getByText('Please enter a profile name.')).toBeInTheDocument();
        expect(profileService.createProfile).not.toHaveBeenCalled();

        // Short password
        fireEvent.change(screen.getByTestId('direct-profile-name-input'), {
            target: { value: 'Alice' },
        });
        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'Short1!' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'Short1!' },
        });
        fireEvent.click(screen.getByTestId('direct-open-profile-submit-btn'));
        expect(screen.getByText('Error: Password must be at least 8 characters')).toBeInTheDocument();
        expect(profileService.createProfile).not.toHaveBeenCalled();

        // Mismatched passwords
        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'StrongPass123!' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'Different123!' },
        });
        fireEvent.click(screen.getByTestId('direct-open-profile-submit-btn'));
        expect(screen.getByText('Error: The passwords do not match')).toBeInTheDocument();
        expect(profileService.createProfile).not.toHaveBeenCalled();
        expect(onRestoreSuccess).not.toHaveBeenCalled();
    });

    it('validates short and mismatched device passwords before calling restoreBackup on an inspected backup', async () => {
        vi.mocked(backupService.probeSyncBackup).mockResolvedValue({
            status: 'found',
            stagedPath: '/tmp/sync_restore.hmcbackup',
            matchedKeyKind: 'stealth',
            sourceUrl: 'https://drop.example.org/data',
            inspection: {
                profileName: 'Bob',
                userId: 'did:key:z6MkBob',
                epoch: 2,
                txNonce: 9,
                sealTimestamp: '2026-11-01T10:00:00Z',
                blobCount: 10,
                customStandardCount: 1,
                protectionMode: 'mnemonic_only',
                localCollision: null,
            },
            userId: 'did:key:z6MkBob',
            localCollision: null,
            resolvedSyncTargets: [],
            error: null,
        });

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onBack={onBack}
                initialMethod="words_or_sheet"
            />
        );

        fireEvent.change(screen.getByTestId('restore-words-word-0'), {
            target: { value: VALID_12 },
        });
        fireEvent.change(screen.getByTestId('restore-sync-input'), {
            target: { value: 'https://drop.example.org/sync.php' },
        });
        await waitFor(() => expect(screen.getByTestId('words-continue-btn')).not.toBeDisabled());
        fireEvent.click(screen.getByTestId('words-continue-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('restore-submit-btn')).toBeInTheDocument();
        });

        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'Short1!' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'Short1!' },
        });
        fireEvent.click(screen.getByTestId('restore-submit-btn'));
        expect(screen.getByText('Error: Password must be at least 8 characters')).toBeInTheDocument();
        expect(backupService.restoreBackup).not.toHaveBeenCalled();

        fireEvent.change(screen.getByTestId('new-device-password-input'), {
            target: { value: 'DevicePass456!' },
        });
        fireEvent.change(screen.getByTestId('confirm-device-password-input'), {
            target: { value: 'OtherPass456!' },
        });
        fireEvent.click(screen.getByTestId('restore-submit-btn'));
        expect(screen.getByText('Error: The passwords do not match')).toBeInTheDocument();
        expect(backupService.restoreBackup).not.toHaveBeenCalled();
        expect(onRestoreSuccess).not.toHaveBeenCalled();
    });

    it('blocks empty backup password and surfaces inspectBackup error without advancing to restore form', async () => {
        vi.mocked(open).mockResolvedValue('/tmp/my-backup.hmcbackup');
        vi.mocked(backupService.probeBackup).mockResolvedValue({
            sourceKind: 'file',
            protectionMode: 'custom_password',
            supportsPasswordRestore: true,
            supportsMnemonicRestore: true,
            blobCount: 15,
            customStandardCount: 1,
        });
        vi.mocked(backupService.inspectBackup).mockRejectedValue(
            new Error('Incorrect backup password')
        );

        render(<RestoreBackupView onRestoreSuccess={onRestoreSuccess} onBack={onBack} />);

        fireEvent.click(screen.getByTestId('choose-file-or-folder-btn'));
        fireEvent.click(screen.getByTestId('select-backup-file-btn'));

        await waitFor(() => {
            expect(screen.getByTestId('backup-password-input')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('inspect-file-backup-btn'));
        expect(
            screen.getByText('Backup password must be at least 8 characters.')
        ).toBeInTheDocument();
        expect(backupService.inspectBackup).not.toHaveBeenCalled();

        fireEvent.change(screen.getByTestId('backup-password-input'), {
            target: { value: 'wrongpass123' },
        });
        fireEvent.click(screen.getByTestId('inspect-file-backup-btn'));

        await waitFor(() => {
            expect(screen.getByText('Incorrect backup password')).toBeInTheDocument();
        });
        expect(screen.queryByTestId('restore-submit-btn')).toBeNull();
        expect(onRestoreSuccess).not.toHaveBeenCalled();
    });
});
