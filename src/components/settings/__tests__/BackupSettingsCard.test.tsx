/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { BackupSettingsCard } from '../BackupSettingsCard';
import { UiModeProvider } from '../../../context/UiModeContext';
import { backupService } from '../../../services/backupService';
import { settingsService } from '../../../services/settingsService';
import { save, open } from '@tauri-apps/plugin-dialog';
import { AppConfigDto, UiMode } from '../../../types';

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
    save: vi.fn(),
    open: vi.fn(),
}));

vi.mock('../../../services/backupService', () => ({
    backupService: {
        exportBackupFile: vi.fn(),
        syncBackupFolder: vi.fn(),
        syncBackup: vi.fn(),
        getBackupStatus: vi.fn().mockResolvedValue({
            status: 'ok',
            lastBackupAt: null,
            backupDirty: false,
            syncFolder: null,
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: true,
        }),
    },
}));

vi.mock('../../../services/settingsService', () => ({
    settingsService: {
        getConfig: vi.fn(),
        patchSecurityPolicy: vi.fn(),
    },
}));

const mockProtectAction = vi.fn(async (cb: (pw: string) => Promise<any>) => cb('mock-wallet-pw'));

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        protectAction: mockProtectAction,
    }),
    useOptionalSession: () => null,
}));

vi.mock('../../../utils/log', () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn(),
        debug: vi.fn(),
    },
}));

describe('BackupSettingsCard', () => {
    const createDefaultConfig = (): AppConfigDto => ({
        uiPreferences: {
            uiMode: 'expert',
            lastUsedDirectory: null,
            balanceViewMode: 'spendable',
            marketShowUnknown: true,
            maxStoredCards: 2000,
            marketSendCardOnPayment: false,
            marketSyncEnabled: true,
            shareMinLevel: 1,
            marketPollWifiOnly: false,
            wotShareViaSync: true,
            postAcceptFromContacts: true,
            mediaAutoLoadMaxHops: 2,
        },
        securityPolicy: {
            sessionTimeoutSeconds: 300,
            privacyDefault: 'public',
            devModeUnlocked: false,
            autoBackupEnabled: true,
            syncBackupEnabled: true,
            backupSyncFolder: null,
            backupProtectionMode: 'mnemonic_only',
            marketSelectedSyncs: [],
            publicSyncs: [],
        },
        lifecycle: {
            pendingRestoreL2Reconciliation: false,
            backupDirty: false,
            lastBackupAt: null,
            lastSyncBackupAt: null,
        },
    });

    const renderCard = (
        config: AppConfigDto,
        onConfigUpdated?: (next: AppConfigDto) => void,
        uiMode: UiMode = 'expert',
    ) =>
        render(
            <UiModeProvider initialMode={uiMode}>
                <BackupSettingsCard config={config} onConfigUpdated={onConfigUpdated} />
            </UiModeProvider>
        );

    beforeEach(() => {
        vi.clearAllMocks();
        mockProtectAction.mockImplementation(async (cb: (pw: string) => Promise<any>) => cb('mock-wallet-pw'));
        const cfg = createDefaultConfig();
        vi.mocked(settingsService.getConfig).mockResolvedValue(cfg);
        vi.mocked(settingsService.patchSecurityPolicy).mockImplementation(async (patch) => ({
            ...cfg.securityPolicy,
            ...patch,
        }));
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'ok',
            lastBackupAt: null,
            backupDirty: false,
            syncFolder: null,
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: true,
        });
    });

    it('renders card title, subtitle, and protection mode', () => {
        renderCard(createDefaultConfig());
        expect(screen.getByText(/Encrypted (Profile|Wallet) Backups/i)).toBeInTheDocument();
        expect(screen.getByText('Protection Mode')).toBeInTheDocument();
        expect(screen.getByText(/Local Backup Folder/i)).toBeInTheDocument();
        expect(screen.getByText(/Export Single File/i)).toBeInTheDocument();
        expect(screen.getByText(/No backup created yet/i)).toBeInTheDocument();
    });

    it('displays formatted timestamp if lastBackupAt is set', () => {
        const cfg = createDefaultConfig();
        cfg.lifecycle.lastBackupAt = '2026-10-30T12:00:00Z';
        renderCard(cfg);
        expect(screen.queryByText(/No backup created yet/i)).not.toBeInTheDocument();
    });

    it('switches between mnemonic_only and custom_password protection modes', () => {
        renderCard(createDefaultConfig());
        const customModeRadio = screen.getByDisplayValue('custom_password');
        fireEvent.click(customModeRadio);

        expect(screen.getAllByPlaceholderText('Min. 8 characters')).toHaveLength(2);
    });

    it('toggles auto-backup checkbox and patches SecurityPolicy', async () => {
        const onConfigUpdated = vi.fn();
        renderCard(createDefaultConfig(), onConfigUpdated);

        const checkboxes = screen.getAllByRole('checkbox');
        const folderAutoBackupCheckbox = checkboxes[0];
        expect(folderAutoBackupCheckbox).toBeChecked();

        fireEvent.click(folderAutoBackupCheckbox);
        expect(folderAutoBackupCheckbox).not.toBeChecked();

        await waitFor(() => {
            expect(settingsService.patchSecurityPolicy).toHaveBeenCalledWith(
                { autoBackupEnabled: false },
                'mock-wallet-pw'
            );
            expect(mockProtectAction).toHaveBeenCalledWith(expect.any(Function), {
                requireFullAccess: true,
            });
            expect(onConfigUpdated).toHaveBeenCalledWith(
                expect.objectContaining({
                    securityPolicy: expect.objectContaining({ autoBackupEnabled: false }),
                })
            );
        });
    });

    it('toggles sync-backup checkbox and patches SecurityPolicy', async () => {
        const onConfigUpdated = vi.fn();
        renderCard(createDefaultConfig(), onConfigUpdated);

        const checkboxes = screen.getAllByRole('checkbox');
        const syncAutoBackupCheckbox = checkboxes[1];
        expect(syncAutoBackupCheckbox).toBeChecked();

        fireEvent.click(syncAutoBackupCheckbox);
        expect(syncAutoBackupCheckbox).not.toBeChecked();

        await waitFor(() => {
            expect(settingsService.patchSecurityPolicy).toHaveBeenCalledWith(
                { syncBackupEnabled: false },
                'mock-wallet-pw'
            );
            expect(onConfigUpdated).toHaveBeenCalledWith(
                expect.objectContaining({
                    securityPolicy: expect.objectContaining({ syncBackupEnabled: false }),
                })
            );
        });
    });

    it('validates short and mismatched passwords in custom_password mode before export', async () => {
        renderCard(createDefaultConfig());
        fireEvent.click(screen.getByDisplayValue('custom_password'));

        const inputs = screen.getAllByPlaceholderText('Min. 8 characters');
        fireEvent.change(inputs[0], { target: { value: 'short1!' } });
        fireEvent.change(inputs[1], { target: { value: 'short1!' } });

        const exportBtn = screen.getByRole('button', { name: /Export Backup File/i });
        fireEvent.click(exportBtn);

        await waitFor(() => {
            expect(
                screen.getByText('Backup password must be at least 8 characters.')
            ).toBeInTheDocument();
        });
        expect(save).not.toHaveBeenCalled();
        expect(backupService.exportBackupFile).not.toHaveBeenCalled();

        fireEvent.change(inputs[0], { target: { value: 'password123' } });
        fireEvent.change(inputs[1], { target: { value: 'mismatch456' } });
        fireEvent.click(exportBtn);

        await waitFor(() => {
            expect(screen.getByText('Backup passwords do not match.')).toBeInTheDocument();
        });
        expect(save).not.toHaveBeenCalled();
        expect(backupService.exportBackupFile).not.toHaveBeenCalled();
    });

    it('exports backup file and invokes onConfigUpdated upon success', async () => {
        const onConfigUpdated = vi.fn();
        vi.mocked(save).mockResolvedValue('/tmp/backup.hmcbackup');
        vi.mocked(backupService.exportBackupFile).mockResolvedValue({
            totalBlobs: 42,
            changedBlobs: 42,
            deletedBlobs: 0,
            uploadBytes: 1024,
            timestamp: '2026-10-30T14:00:00Z',
            destinationPath: '/tmp/backup.hmcbackup',
            customStandardCount: 1,
        });
        const refreshed = createDefaultConfig();
        refreshed.lifecycle.lastBackupAt = '2026-10-30T14:00:00Z';
        refreshed.securityPolicy.backupProtectionMode = 'mnemonic_only';
        vi.mocked(settingsService.getConfig).mockResolvedValue(refreshed);

        renderCard(createDefaultConfig(), onConfigUpdated);
        const exportBtn = screen.getByRole('button', { name: /Export Backup File/i });
        fireEvent.click(exportBtn);

        await waitFor(() => {
            expect(save).toHaveBeenCalled();
            expect(backupService.exportBackupFile).toHaveBeenCalledWith({
                destinationPath: '/tmp/backup.hmcbackup',
                protectionMode: 'mnemonic_only',
                backupPassword: null,
                password: 'mock-wallet-pw',
            });
            expect(onConfigUpdated).toHaveBeenCalledWith(
                expect.objectContaining({
                    lifecycle: expect.objectContaining({ lastBackupAt: '2026-10-30T14:00:00Z' }),
                    securityPolicy: expect.objectContaining({ backupProtectionMode: 'mnemonic_only' }),
                })
            );
            expect(screen.getByText(/Backup exported successfully/i)).toBeInTheDocument();
        });
    });

    it('allows folder selection and syncs differential backup with exact 1 selection button', async () => {
        const onConfigUpdated = vi.fn();
        vi.mocked(open).mockResolvedValue('/media/usb/backups');
        vi.mocked(backupService.syncBackupFolder).mockResolvedValue({
            totalBlobs: 10,
            changedBlobs: 2,
            deletedBlobs: 0,
            uploadBytes: 512,
            timestamp: '2026-10-30T14:10:00Z',
            destinationPath: '/media/usb/backups',
            customStandardCount: 0,
        });

        renderCard(createDefaultConfig(), onConfigUpdated);

        const chooseFolderButtons = screen.getAllByRole('button', { name: /Choose Folder/i });
        expect(chooseFolderButtons).toHaveLength(1);
        fireEvent.click(chooseFolderButtons[0]);

        await waitFor(() => {
            expect(open).toHaveBeenCalledWith({ directory: true, multiple: false, defaultPath: undefined });
            expect(screen.getByText('/media/usb/backups')).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /Change folder/i })).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /Unlink folder/i })).toBeInTheDocument();
        });

        const syncNowBtn = screen.getByRole('button', { name: /Sync Folder Now/i });
        fireEvent.click(syncNowBtn);

        await waitFor(() => {
            expect(backupService.syncBackupFolder).toHaveBeenCalledWith({
                folderPath: '/media/usb/backups',
                protectionMode: 'mnemonic_only',
                backupPassword: null,
                password: 'mock-wallet-pw',
                forceFull: false,
            });
            expect(screen.getByText(/Sync completed: 2 updated, 8 unchanged/i)).toBeInTheDocument();
        });
    });

    it('allows clearing/unlinking configured backup folder', async () => {
        const onConfigUpdated = vi.fn();
        const cfg = createDefaultConfig();
        cfg.securityPolicy.backupSyncFolder = '/media/usb/backups';
        renderCard(cfg, onConfigUpdated);

        expect(screen.getByText('/media/usb/backups')).toBeInTheDocument();
        const unlinkBtn = screen.getByRole('button', { name: /Unlink folder/i });
        fireEvent.click(unlinkBtn);

        await waitFor(() => {
            expect(settingsService.patchSecurityPolicy).toHaveBeenCalledWith(
                { backupSyncFolder: null },
                'mock-wallet-pw'
            );
            expect(onConfigUpdated).toHaveBeenCalledWith(
                expect.objectContaining({
                    securityPolicy: expect.objectContaining({ backupSyncFolder: null }),
                })
            );
            expect(screen.getByText('No sync folder configured.')).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /Choose Folder/i })).toBeInTheDocument();
        });
    });

    it('dispatches APP_EVENTS.BACKUP_UPDATED when folder backup is synced manually', async () => {
        const eventSpy = vi.fn();
        window.addEventListener('humoco:backup-updated', eventSpy);

        try {
            vi.mocked(open).mockResolvedValue('/media/usb/backups');
            vi.mocked(backupService.syncBackupFolder).mockResolvedValue({
                totalBlobs: 10,
                changedBlobs: 2,
                deletedBlobs: 0,
                uploadBytes: 512,
                timestamp: new Date().toISOString(),
                destinationPath: '/media/usb/backups',
                customStandardCount: 0,
            });

            const cfg = createDefaultConfig();
            cfg.securityPolicy.backupSyncFolder = '/media/usb/backups';
            renderCard(cfg, vi.fn());

            const syncNowBtn = screen.getByRole('button', { name: /Sync Folder Now/i });
            fireEvent.click(syncNowBtn);

            await waitFor(() => {
                expect(backupService.syncBackupFolder).toHaveBeenCalled();
                expect(eventSpy).toHaveBeenCalled();
            });
        } finally {
            window.removeEventListener('humoco:backup-updated', eventSpy);
        }
    });

    it('hides Protection Mode behind more-options disclosure in simple mode while keeping backup button and hint visible', async () => {
        renderCard(createDefaultConfig(), vi.fn(), 'simple');

        expect(screen.getByTestId('backup-now-button')).toBeInTheDocument();
        expect(screen.getByTestId('backup-which-help-hint')).toBeInTheDocument();
        expect(screen.queryByText(/Protection Mode/i)).not.toBeInTheDocument();

        fireEvent.click(screen.getByTestId('backup-more-options-toggle'));

        await waitFor(() => {
            expect(screen.getByTestId('backup-more-options-content')).toBeInTheDocument();
            expect(screen.getByText(/Protection Mode/i)).toBeInTheDocument();
        });
    });

    it('synchronizes syncFolder and protectionMode with updated config props and refreshes backupStatus on BACKUP_UPDATED_EVENT', async () => {
        const { BACKUP_UPDATED_EVENT } = await import('../../../sync/syncBridge');
        vi.mocked(backupService.getBackupStatus)
            .mockResolvedValueOnce({
                status: 'ok',
                lastBackupAt: null,
                backupDirty: false,
                syncFolder: null,
                changedBlobs: 0,
                syncBackupEnabled: true,
                lastSyncBackupAt: null,
                hasAuthorizedSyncBackupTarget: false,
            })
            .mockResolvedValueOnce({
                status: 'ok',
                lastBackupAt: null,
                backupDirty: false,
                syncFolder: null,
                changedBlobs: 0,
                syncBackupEnabled: true,
                lastSyncBackupAt: null,
                hasAuthorizedSyncBackupTarget: true,
            });

        const cfg = createDefaultConfig();
        const { rerender } = renderCard(cfg);

        expect(screen.getByText('No sync folder configured.')).toBeInTheDocument();
        expect((screen.getByDisplayValue('mnemonic_only') as HTMLInputElement).checked).toBe(true);
        await waitFor(() => {
            expect(screen.getByText(/No Sync Service authorized for backups/i)).toBeInTheDocument();
        });

        // Update config props externally -> syncFolder & protectionMode must sync
        const updatedCfg: AppConfigDto = {
            ...cfg,
            securityPolicy: {
                ...cfg.securityPolicy,
                backupSyncFolder: '/srv/updated-backups',
                backupProtectionMode: 'custom_password',
            },
        };
        rerender(
            <UiModeProvider initialMode="expert">
                <BackupSettingsCard config={updatedCfg} onConfigUpdated={vi.fn()} />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(screen.getByText('/srv/updated-backups')).toBeInTheDocument();
            expect((screen.getByDisplayValue('custom_password') as HTMLInputElement).checked).toBe(true);
        });

        // Dispatch BACKUP_UPDATED_EVENT -> backupStatus refreshes
        window.dispatchEvent(new CustomEvent(BACKUP_UPDATED_EVENT));
        await waitFor(() => {
            expect(backupService.getBackupStatus).toHaveBeenCalledTimes(2);
            expect(screen.getByText(/Active & ready/i)).toBeInTheDocument();
        });
    });

    it('rolls back auto-backup toggle and skips onConfigUpdated when protectAction is cancelled or patch rejects', async () => {
        const onConfigUpdated = vi.fn();
        mockProtectAction.mockResolvedValueOnce(undefined);

        renderCard(createDefaultConfig(), onConfigUpdated);

        const folderAutoBackupCheckbox = screen.getAllByRole('checkbox')[0];
        expect(folderAutoBackupCheckbox).toBeChecked();

        // Cancelled authorization rolls back checkbox without calling backend
        fireEvent.click(folderAutoBackupCheckbox);
        await waitFor(() => {
            expect(folderAutoBackupCheckbox).toBeChecked();
        });
        expect(settingsService.patchSecurityPolicy).not.toHaveBeenCalled();
        expect(onConfigUpdated).not.toHaveBeenCalled();

        // Rejected policy update rolls back checkbox and shows error
        vi.mocked(settingsService.patchSecurityPolicy).mockRejectedValueOnce(
            new Error('Policy save failed')
        );
        fireEvent.click(folderAutoBackupCheckbox);
        await waitFor(() => {
            expect(screen.getByText('Policy save failed')).toBeInTheDocument();
            expect(folderAutoBackupCheckbox).toBeChecked();
        });
        expect(onConfigUpdated).not.toHaveBeenCalled();
    });

    it('rolls back folder unlink and displays error when patchSecurityPolicy rejects', async () => {
        const onConfigUpdated = vi.fn();
        const cfg = createDefaultConfig();
        cfg.securityPolicy.backupSyncFolder = '/media/usb/backups';
        vi.mocked(settingsService.patchSecurityPolicy).mockRejectedValueOnce(
            new Error('Disk busy')
        );

        renderCard(cfg, onConfigUpdated);

        fireEvent.click(screen.getByRole('button', { name: /Unlink folder/i }));

        await waitFor(() => {
            expect(screen.getByText('Disk busy')).toBeInTheDocument();
            expect(screen.getByText('/media/usb/backups')).toBeInTheDocument();
        });
        expect(onConfigUpdated).not.toHaveBeenCalled();
    });

    it('validates custom password before syncing backup folder', async () => {
        const cfg = createDefaultConfig();
        cfg.securityPolicy.backupSyncFolder = '/media/usb/backups';
        cfg.securityPolicy.backupProtectionMode = 'custom_password';

        renderCard(cfg);

        const inputs = screen.getAllByPlaceholderText('Min. 8 characters');
        fireEvent.change(inputs[0], { target: { value: 'short1!' } });
        fireEvent.change(inputs[1], { target: { value: 'short1!' } });

        fireEvent.click(screen.getByRole('button', { name: /Sync Folder Now/i }));

        await waitFor(() => {
            expect(
                screen.getByText('Backup password must be at least 8 characters.')
            ).toBeInTheDocument();
        });
        expect(backupService.syncBackupFolder).not.toHaveBeenCalled();
    });

    it('runs manual online sync backup and reports success with config refresh', async () => {
        const onConfigUpdated = vi.fn();
        vi.mocked(backupService.syncBackup).mockResolvedValue({
            uploadedTo: 2,
            paddedBytes: 4096,
            authorizedTargets: 2,
            errors: [],
            timestamp: '2026-10-30T15:00:00Z',
        });

        renderCard(createDefaultConfig(), onConfigUpdated);

        fireEvent.click(screen.getByRole('button', { name: 'Back up to sync service now' }));

        await waitFor(() => {
            expect(backupService.syncBackup).toHaveBeenCalledWith('mock-wallet-pw');
            expect(
                screen.getByText('Backup successfully updated in 2 sync service(s).')
            ).toBeInTheDocument();
            expect(onConfigUpdated).toHaveBeenCalled();
        });
    });

    it('surfaces sync backup errors for zero authorized targets and failed uploads without updating config', async () => {
        const onConfigUpdated = vi.fn();
        vi.mocked(backupService.syncBackup)
            .mockResolvedValueOnce({
                uploadedTo: 0,
                paddedBytes: 0,
                authorizedTargets: 0,
                errors: [],
                timestamp: null,
            })
            .mockResolvedValueOnce({
                uploadedTo: 0,
                paddedBytes: 0,
                authorizedTargets: 1,
                errors: ['HTTP 503'],
                timestamp: null,
            });

        renderCard(createDefaultConfig(), onConfigUpdated);

        const syncBtn = screen.getByRole('button', { name: 'Back up to sync service now' });
        fireEvent.click(syncBtn);

        await waitFor(() => {
            expect(
                screen.getByText(
                    'No sync service authorized for backup channel. Please authorize in sync service settings.'
                )
            ).toBeInTheDocument();
        });

        fireEvent.click(syncBtn);

        await waitFor(() => {
            expect(
                screen.getByText('Failed to back up to sync service: HTTP 503')
            ).toBeInTheDocument();
        });
        expect(onConfigUpdated).not.toHaveBeenCalled();
    });

    it('routes backup-now-button to folder sync, online sync, or file export based on active configuration', async () => {
        // Case A: Configured sync folder -> routes to syncBackupFolder
        const cfgFolder = createDefaultConfig();
        cfgFolder.securityPolicy.backupSyncFolder = '/media/usb';
        vi.mocked(backupService.syncBackupFolder).mockResolvedValue({
            totalBlobs: 4,
            changedBlobs: 1,
            deletedBlobs: 0,
            uploadBytes: 128,
            timestamp: '2026-10-30T15:10:00Z',
            destinationPath: '/media/usb',
            customStandardCount: 0,
        });

        const { unmount: unmountA } = renderCard(cfgFolder, vi.fn());
        fireEvent.click(screen.getByTestId('backup-now-button'));
        await waitFor(() => {
            expect(backupService.syncBackupFolder).toHaveBeenCalledTimes(1);
        });
        expect(backupService.syncBackup).not.toHaveBeenCalled();
        expect(save).not.toHaveBeenCalled();
        unmountA();

        vi.clearAllMocks();
        mockProtectAction.mockImplementation(async (cb: (pw: string) => Promise<any>) => cb('mock-wallet-pw'));
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'ok',
            lastBackupAt: null,
            backupDirty: false,
            syncFolder: null,
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: true,
        });
        vi.mocked(backupService.syncBackup).mockResolvedValue({
            uploadedTo: 1,
            paddedBytes: 2048,
            authorizedTargets: 1,
            errors: [],
            timestamp: '2026-10-30T15:15:00Z',
        });

        // Case B: No folder, syncBackupEnabled & authorized target -> routes to syncBackup
        const cfgSync = createDefaultConfig();
        const { unmount: unmountB } = renderCard(cfgSync, vi.fn());
        await waitFor(() => {
            expect(screen.getByText(/Active & ready/i)).toBeInTheDocument();
        });
        fireEvent.click(screen.getByTestId('backup-now-button'));
        await waitFor(() => {
            expect(backupService.syncBackup).toHaveBeenCalledTimes(1);
        });
        expect(save).not.toHaveBeenCalled();
        unmountB();

        vi.clearAllMocks();
        mockProtectAction.mockImplementation(async (cb: (pw: string) => Promise<any>) => cb('mock-wallet-pw'));
        vi.mocked(save).mockResolvedValue('/tmp/manual-fallback.hmcbackup');
        vi.mocked(backupService.exportBackupFile).mockResolvedValue({
            totalBlobs: 4,
            changedBlobs: 4,
            deletedBlobs: 0,
            uploadBytes: 256,
            timestamp: '2026-10-30T15:20:00Z',
            destinationPath: '/tmp/manual-fallback.hmcbackup',
            customStandardCount: 0,
        });

        // Case C: No folder & sync backup disabled -> routes to single file export
        const cfgFile = createDefaultConfig();
        cfgFile.securityPolicy.syncBackupEnabled = false;
        renderCard(cfgFile, vi.fn());
        fireEvent.click(screen.getByTestId('backup-now-button'));
        await waitFor(() => {
            expect(save).toHaveBeenCalledTimes(1);
            expect(backupService.exportBackupFile).toHaveBeenCalledTimes(1);
        });
        expect(backupService.syncBackup).not.toHaveBeenCalled();
    });
});
