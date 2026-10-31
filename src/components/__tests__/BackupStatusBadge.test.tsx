/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { BackupStatusBadge } from '../BackupStatusBadge';
import { backupService } from '../../services/backupService';
import * as autoBackupUtils from '../../utils/autoBackup';

const mockNavigate = vi.fn();
vi.mock('../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
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

vi.mock('../../services/backupService', () => ({
    backupService: {
        getBackupStatus: vi.fn(),
        runAutoBackupSync: vi.fn(),
    },
}));

describe('BackupStatusBadge', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders notConfigured status and navigates to settings on click', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'notConfigured',
            lastBackupAt: null,
            backupDirty: false,
            syncFolder: null,
            changedBlobs: null,
        });

        render(<BackupStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('No backup')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Backup Status: No backup/i }));
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'settings' });
    });

    it('renders synced status and opens details modal on click', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'synced',
            lastBackupAt: '2026-10-31T12:00:00Z',
            backupDirty: false,
            syncFolder: '/media/usb/backups',
            changedBlobs: 2,
        });

        render(<BackupStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('Backed up')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Backup Status: Backed up/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText('/media/usb/backups')).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /Sync Now/i })).toBeInTheDocument();
        });
    });

    it('renders folderDisconnected status with warning description in modal', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'folderDisconnected',
            lastBackupAt: '2026-10-30T10:00:00Z',
            backupDirty: true,
            syncFolder: '/media/usb/backups',
            changedBlobs: null,
        });

        render(<BackupStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('Drive disconnected')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Backup Status: Drive disconnected/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText(/target backup folder is currently disconnected/i)).toBeInTheDocument();
        });
    });

    it('invokes runAutoBackupPass when Sync Now button is clicked in modal', async () => {
        const spyRunPass = vi.spyOn(autoBackupUtils, 'runAutoBackupPass').mockResolvedValue({
            status: 'synced',
            lastBackupAt: '2026-10-31T16:00:00Z',
            backupDirty: false,
            syncFolder: '/media/usb/backups',
            changedBlobs: 0,
        });

        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'outdated',
            lastBackupAt: '2026-10-01T10:00:00Z',
            backupDirty: true,
            syncFolder: '/media/usb/backups',
            changedBlobs: null,
        });

        render(<BackupStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('Backup pending')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Backup Status: Backup pending/i }));

        const syncNowBtn = await screen.findByRole('button', { name: /Sync Now/i });
        fireEvent.click(syncNowBtn);

        expect(spyRunPass).toHaveBeenCalled();
    });
});
