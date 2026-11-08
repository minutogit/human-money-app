/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { RestoreBackupView } from '../RestoreBackupView';
import { backupService } from '../../services/backupService';
import { authService } from '../../services/authService';
import { open } from '@tauri-apps/plugin-dialog';

vi.mock('react-i18next', () => {
    const fs = require('fs');
    const path = require('path');
    const enPath = path.resolve(process.cwd(), 'src/locales/en.json');
    let enTranslations = {};
    try {
        enTranslations = JSON.parse(fs.readFileSync(enPath, 'utf-8'));
    } catch (err) {
        console.error(err);
    }

    const stableT = (key: string, options?: any) => {
        let val = (enTranslations as Record<string, string>)[key] || key;
        if (options && typeof options === 'object') {
            for (const [k, v] of Object.entries(options)) {
                val = val.replace(new RegExp(`\\{\\{\\s*${k}\\s*\\}\\}`, 'g'), String(v));
            }
        }
        return val;
    };

    return {
        useTranslation: () => ({
            t: stableT,
            i18n: { language: 'en', changeLanguage: vi.fn() },
            ready: true,
        }),
    };
});

vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn(),
}));

vi.mock('../../services/backupService', () => ({
    backupService: {
        probeBackup: vi.fn(),
        inspectBackup: vi.fn(),
        restoreBackup: vi.fn(),
        fetchAndInspectDropBackup: vi.fn(),
        reconcileRestoredWalletL2: vi.fn(),
    },
}));

vi.mock('../../services/authService', () => ({
    authService: {
        getLocalInstanceId: vi.fn(),
    },
}));

vi.mock('../../utils/log', () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn(),
        debug: vi.fn(),
    },
}));

describe('RestoreBackupView', () => {
    const onRestoreSuccess = vi.fn();
    const onSwitchToRecreate = vi.fn();
    const onBack = vi.fn();

    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders initial source selection screen and links', () => {
        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onSwitchToRecreate={onSwitchToRecreate}
                onBack={onBack}
            />
        );

        expect(screen.getByText('Restore Wallet from Backup')).toBeInTheDocument();
        expect(screen.getByText('Select Backup Source')).toBeInTheDocument();
        expect(screen.getByText('Select .hmcbackup File')).toBeInTheDocument();
        expect(screen.getByText('Select Backup Sync Folder')).toBeInTheDocument();

        // Top choice link to recreate
        const recreateBtn = screen.getByRole('button', { name: /Recreate/i });
        fireEvent.click(recreateBtn);
        expect(onSwitchToRecreate).toHaveBeenCalled();

        // Back button
        const backBtn = screen.getByTestId('restore-back-btn');
        fireEvent.click(backBtn);
        expect(onBack).toHaveBeenCalled();
    });

    it('handles probing a backup file and transitions to unlock screen', async () => {
        vi.mocked(open).mockResolvedValue('/tmp/my-backup.hmcbackup');
        vi.mocked(backupService.probeBackup).mockResolvedValue({
            sourceKind: 'file',
            protectionMode: 'custom_password',
            supportsPasswordRestore: true,
            supportsMnemonicRestore: true,
            blobCount: 15,
            customStandardCount: 1,
        });

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onSwitchToRecreate={onSwitchToRecreate}
                onBack={onBack}
            />
        );

        fireEvent.click(screen.getByText('Select .hmcbackup File'));

        await waitFor(() => {
            expect(backupService.probeBackup).toHaveBeenCalledWith('/tmp/my-backup.hmcbackup');
            expect(screen.getByText('Unlock & Preview Backup')).toBeInTheDocument();
            expect(screen.getByText('/tmp/my-backup.hmcbackup')).toBeInTheDocument();
        });
    });

    it('allows inspecting backup with password and then restoring with new device password', async () => {
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
            userId: 'hmc1alice1234567890',
            epoch: 2,
            txNonce: 18,
            sealTimestamp: '2026-10-30T10:00:00Z',
            blobCount: 15,
            customStandardCount: 1,
            protectionMode: 'custom_password',
        });

        vi.mocked(authService.getLocalInstanceId).mockResolvedValue('device-uuid-123');
        vi.mocked(backupService.restoreBackup).mockResolvedValue({
            folderName: 'profile_alice',
            profileName: 'Alice',
            activeVoucherQueriesCount: 5,
            reconciledSpentCount: 2,
            restoredStandardsCount: 1,
            l2ReconciliationStatus: 'no_servers',
        });

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onSwitchToRecreate={onSwitchToRecreate}
                onBack={onBack}
            />
        );

        // 1. Select source
        fireEvent.click(screen.getByText('Select .hmcbackup File'));

        await waitFor(() => {
            expect(screen.getByText('Unlock & Preview Backup')).toBeInTheDocument();
        });

        // 2. Input backup password
        const pwInput = screen.getByTestId('backup-password-input');
        fireEvent.change(pwInput, { target: { value: 'backupSecret123' } });

        // Click Verify & Preview
        fireEvent.click(screen.getByRole('button', { name: /Verify & Preview Backup/i }));

        await waitFor(() => {
            expect(backupService.inspectBackup).toHaveBeenCalledWith({
                sourcePath: '/tmp/my-backup.hmcbackup',
                credentialMode: 'password',
                backupPassword: 'backupSecret123',
                mnemonic: null,
                passphrase: null,
                userPrefix: null,
                language: null,
            });
            expect(screen.getByText('Verified Backup Manifest')).toBeInTheDocument();
            expect(screen.getByText('Alice')).toBeInTheDocument();
            expect(screen.getByText(/User ID: hmc1alice1234567890/i)).toBeInTheDocument();
        });

        // 3. Step 3: Set local device password & restore
        expect(screen.getByText('Set Local Device Password')).toBeInTheDocument();
        const newPasswordInput = screen.getByTestId('new-device-password-input');
        const confirmNewPasswordInput = screen.getByTestId('confirm-device-password-input');

        fireEvent.change(newPasswordInput, { target: { value: 'devicePassword123' } });
        fireEvent.change(confirmNewPasswordInput, { target: { value: 'devicePassword123' } });

        fireEvent.click(screen.getByTestId('restore-submit-btn'));

        await waitFor(() => {
            expect(authService.getLocalInstanceId).toHaveBeenCalled();
            expect(backupService.restoreBackup).toHaveBeenCalledWith({
                sourcePath: '/tmp/my-backup.hmcbackup',
                credentialMode: 'password',
                backupPassword: 'backupSecret123',
                mnemonic: null,
                passphrase: null,
                userPrefix: null,
                language: null,
                newPassword: 'devicePassword123',
                localInstanceId: 'device-uuid-123',
            });
            expect(screen.getByText('Wallet Successfully Restored!')).toBeInTheDocument();
            expect(screen.getByText('Restored offline. No L2 reconciliation servers are configured yet.')).toBeInTheDocument();
        });

        // 4. Open Wallet button
        fireEvent.click(screen.getByRole('button', { name: /Open Wallet/i }));
        expect(onRestoreSuccess).toHaveBeenCalledWith(
            expect.objectContaining({
                folderName: 'profile_alice',
                activeVoucherQueriesCount: 5,
            })
        );
    });

    it('allows restoring from a drop using mnemonic and drop address', async () => {
        vi.mocked(backupService.fetchAndInspectDropBackup).mockResolvedValue({
            stagedPath: '/tmp/drop_restore_123.hmcbackup',
            inspection: {
                profileName: 'Bob',
                userId: 'hmc1bob1234567890',
                epoch: 1,
                txNonce: 5,
                sealTimestamp: '2026-11-01T10:00:00Z',
                blobCount: 10,
                customStandardCount: 0,
                protectionMode: 'mnemonic_only',
            },
            sourceUrl: 'https://drop.example.org',
            matchedKeyKind: 'stealth',
        });

        vi.mocked(authService.getLocalInstanceId).mockResolvedValue('device-uuid-456');
        vi.mocked(backupService.restoreBackup).mockResolvedValue({
            folderName: 'profile_bob',
            profileName: 'Bob',
            activeVoucherQueriesCount: 2,
            reconciledSpentCount: 0,
            restoredStandardsCount: 0,
            l2ReconciliationStatus: 'offline_or_failed',
        });

        vi.mocked(backupService.reconcileRestoredWalletL2).mockResolvedValue({
            l2ReconciliationStatus: 'reconciled',
            activeVoucherQueriesCount: 2,
            reconciledSpentCount: 0,
            pendingRestoreL2Reconciliation: false,
        });

        render(
            <RestoreBackupView
                onRestoreSuccess={onRestoreSuccess}
                onSwitchToRecreate={onSwitchToRecreate}
                onBack={onBack}
            />
        );

        // 1. Click "Restore from Drop"
        fireEvent.click(screen.getByText('Restore from Drop'));

        await waitFor(() => {
            expect(screen.getByText('Load Backup from Drop')).toBeInTheDocument();
            expect(screen.getByText('Find & Load Backup')).toBeInTheDocument();
        });

        // Enter mnemonic words (paste phrase into first word input)
        const firstWordInput = screen.getByTestId('restore-drop-word-0');
        fireEvent.change(firstWordInput, {
            target: {
                value: 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about',
            },
        });

        // Enter drop address
        const dropAddressInput = screen.getByTestId('restore-drop-input');
        fireEvent.change(dropAddressInput, { target: { value: 'https://drop.example.org' } });

        // Click Find & Load Backup
        fireEvent.click(screen.getByText('Find & Load Backup'));

        await waitFor(() => {
            expect(backupService.fetchAndInspectDropBackup).toHaveBeenCalledWith({
                dropInput: 'https://drop.example.org',
                mnemonic: 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about',
                passphrase: null,
                userPrefix: null,
                language: 'english',
            });
            expect(screen.getByText('Verified Backup Manifest')).toBeInTheDocument();
            expect(screen.getByText('Bob')).toBeInTheDocument();
        });

        // Set device password and restore
        const newPasswordInput = screen.getByTestId('new-device-password-input');
        const confirmNewPasswordInput = screen.getByTestId('confirm-device-password-input');

        fireEvent.change(newPasswordInput, { target: { value: 'devicePassword456' } });
        fireEvent.change(confirmNewPasswordInput, { target: { value: 'devicePassword456' } });

        fireEvent.click(screen.getByTestId('restore-submit-btn'));

        await waitFor(() => {
            expect(backupService.restoreBackup).toHaveBeenCalledWith(
                expect.objectContaining({
                    sourcePath: '/tmp/drop_restore_123.hmcbackup',
                    credentialMode: 'mnemonic',
                    newPassword: 'devicePassword456',
                })
            );
            expect(screen.getByText('Online Security Reconciliation Recommended')).toBeInTheDocument();
        });

        // Click Reconcile online now
        fireEvent.click(screen.getByRole('button', { name: /Reconcile online now/i }));

        await waitFor(() => {
            expect(backupService.reconcileRestoredWalletL2).toHaveBeenCalled();
            expect(screen.getByText(/Online security reconciliation completed successfully/i)).toBeInTheDocument();
        });
    });
});
