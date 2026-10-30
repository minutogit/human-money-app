/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { BackupSettingsCard } from '../BackupSettingsCard';
import { backupService } from '../../services/backupService';
import { save, open } from '@tauri-apps/plugin-dialog';
import { AppSettings } from '../../types';

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

vi.mock('../../services/backupService', () => ({
    backupService: {
        exportBackupFile: vi.fn(),
        syncBackupFolder: vi.fn(),
    },
}));

const mockProtectAction = vi.fn(async (cb: (pw: string) => Promise<any>) => cb('mock-wallet-pw'));

vi.mock('../../context/SessionContext', () => ({
    useSession: () => ({
        protectAction: mockProtectAction,
    }),
}));

vi.mock('../../utils/log', () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn(),
        debug: vi.fn(),
    },
}));

describe('BackupSettingsCard', () => {
    const defaultSettings: AppSettings = {
        bundleRetentionDays: 30,
        sessionTimeoutSeconds: 300,
        privacyDefault: 'public',
        backupProtectionMode: 'mnemonic_only',
        lastBackupAt: null,
        backupSyncFolder: null,
    };

    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders card title, subtitle, and never-backed-up status', () => {
        render(<BackupSettingsCard settings={defaultSettings} />);
        expect(screen.getByText('Encrypted Wallet Backups')).toBeInTheDocument();
        expect(screen.getByText(/No backup created yet/i)).toBeInTheDocument();
    });

    it('displays formatted timestamp if lastBackupAt is set', () => {
        const settingsWithDate: AppSettings = {
            ...defaultSettings,
            lastBackupAt: '2026-10-30T12:00:00Z',
        };
        render(<BackupSettingsCard settings={settingsWithDate} />);
        expect(screen.queryByText(/No backup created yet/i)).not.toBeInTheDocument();
    });

    it('switches between mnemonic_only and custom_password protection modes', () => {
        render(<BackupSettingsCard settings={defaultSettings} />);
        const customModeRadio = screen.getByDisplayValue('custom_password');
        fireEvent.click(customModeRadio);

        expect(screen.getAllByPlaceholderText('Min. 8 characters')).toHaveLength(2);
    });

    it('validates password mismatch in custom_password mode on export', async () => {
        render(<BackupSettingsCard settings={defaultSettings} />);
        fireEvent.click(screen.getByDisplayValue('custom_password'));

        const inputs = screen.getAllByPlaceholderText('Min. 8 characters');
        fireEvent.change(inputs[0], { target: { value: 'password123' } });
        fireEvent.change(inputs[1], { target: { value: 'mismatch456' } });

        const exportBtn = screen.getByRole('button', { name: /Export Backup File/i });
        fireEvent.click(exportBtn);

        await waitFor(() => {
            expect(screen.getByText('Backup passwords do not match.')).toBeInTheDocument();
        });
        expect(save).not.toHaveBeenCalled();
    });

    it('exports backup file and invokes onSettingsUpdated upon success', async () => {
        const onSettingsUpdated = vi.fn();
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

        render(<BackupSettingsCard settings={defaultSettings} onSettingsUpdated={onSettingsUpdated} />);
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
            expect(onSettingsUpdated).toHaveBeenCalledWith(
                expect.objectContaining({
                    lastBackupAt: '2026-10-30T14:00:00Z',
                    backupProtectionMode: 'mnemonic_only',
                })
            );
            expect(screen.getByText(/Backup exported successfully/i)).toBeInTheDocument();
        });
    });

    it('allows folder selection and syncs differential backup', async () => {
        const onSettingsUpdated = vi.fn();
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

        render(<BackupSettingsCard settings={defaultSettings} onSettingsUpdated={onSettingsUpdated} />);
        
        // Click Choose Folder
        const chooseFolderBtn = screen.getByRole('button', { name: /Choose Folder/i });
        fireEvent.click(chooseFolderBtn);

        await waitFor(() => {
            expect(open).toHaveBeenCalledWith({ directory: true, multiple: false, defaultPath: undefined });
            expect(screen.getByText('/media/usb/backups')).toBeInTheDocument();
        });

        // Click Sync Folder Now
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
});
