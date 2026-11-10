/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { BackupStatusBadge } from '../BackupStatusBadge';
import { backupService } from '../../services/backupService';
import * as autoBackupUtils from '../../utils/autoBackup';

const mockNavigateToSettingsSection = vi.fn();
vi.mock('../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigateToSettingsSection: mockNavigateToSettingsSection,
    }),
}));

vi.mock('../../context/SessionContext', () => ({
    useSession: () => ({
        isSessionActive: true,
    }),
}));

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

    const stableT = (key: string, defaultValueOrOptions?: any, maybeOptions?: any) => {
        let val = (enTranslations as Record<string, string>)[key] || key;
        const options = (typeof defaultValueOrOptions === 'object' && defaultValueOrOptions !== null)
            ? defaultValueOrOptions
            : (typeof maybeOptions === 'object' && maybeOptions !== null)
                ? maybeOptions
                : undefined;

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

vi.mock('../../services/backupService', () => ({
    backupService: {
        getBackupStatus: vi.fn(),
        syncBackup: vi.fn().mockResolvedValue({ uploadedTo: 1, authorizedTargets: 1 }),
    },
}));

describe('BackupStatusBadge', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders notConfigured status and opens popover on click', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'notConfigured',
            lastBackupAt: null,
            backupDirty: false,
            syncFolder: null,
            changedBlobs: null,
            syncBackupEnabled: false,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: false,
        });

        render(<BackupStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('No backup')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Backup Status: No backup/i }));
        expect(screen.getByRole('dialog')).toBeInTheDocument();

        const settingsBtn = screen.getByRole('button', { name: /Backup settings/i });
        fireEvent.click(settingsBtn);
        expect(mockNavigateToSettingsSection).toHaveBeenCalledWith('backup');
    });

    it('closes popover on Escape key', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'notConfigured',
            lastBackupAt: null,
            backupDirty: false,
            syncFolder: null,
            changedBlobs: null,
            syncBackupEnabled: false,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: false,
        });

        render(<BackupStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('No backup')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Backup Status: No backup/i }));
        expect(screen.getByRole('dialog')).toBeInTheDocument();

        fireEvent.keyDown(document, { key: 'Escape' });
        await waitFor(() => {
            expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
        });
    });

    it('renders ok / synced status and shows cloud and local details in popover', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'ok',
            lastBackupAt: '2026-10-31T12:00:00Z',
            backupDirty: false,
            syncFolder: '/media/usb/backups',
            changedBlobs: 2,
            syncBackupEnabled: true,
            lastSyncBackupAt: '2026-10-31T12:05:00Z',
            hasAuthorizedSyncBackupTarget: true,
        });

        render(<BackupStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('Backed up')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Backup Status: Backed up/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText('/media/usb/backups')).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /Back up now/i })).toBeInTheDocument();
        });
    });

    it('renders folderDisconnected status with warning description in popover', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'disconnected',
            lastBackupAt: '2026-10-30T10:00:00Z',
            backupDirty: true,
            syncFolder: '/media/usb/backups',
            changedBlobs: null,
            syncBackupEnabled: false,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: false,
        });

        render(<BackupStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('Drive disconnected')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Backup Status: Drive disconnected/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText(/Backup folder \(e\.g\. USB drive\) not connected/i)).toBeInTheDocument();
        });
    });

    it('invokes runAutoBackupPass and syncBackup when Back up now button is clicked', async () => {
        const spyRunPass = vi.spyOn(autoBackupUtils, 'runAutoBackupPass').mockResolvedValue({
            status: 'ok',
            lastBackupAt: '2026-10-31T16:00:00Z',
            backupDirty: false,
            syncFolder: '/media/usb/backups',
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: true,
        });

        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'pending',
            lastBackupAt: '2026-10-01T10:00:00Z',
            backupDirty: true,
            syncFolder: '/media/usb/backups',
            changedBlobs: null,
            syncBackupEnabled: true,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: true,
        });

        render(<BackupStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('Backup pending')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Backup Status: Backup pending/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
        });

        const syncNowBtn = screen.getByRole('button', { name: /Back up now/i });
        fireEvent.click(syncNowBtn);

        await waitFor(() => {
            expect(spyRunPass).toHaveBeenCalled();
            expect(backupService.syncBackup).toHaveBeenCalled();
        });
    });
});
