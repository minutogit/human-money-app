import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { StatusChip } from '../StatusChip';
import { backupService } from '../../services/backupService';
import { marketService } from '../../services/marketService';
import { settingsService } from '../../services/settingsService';

vi.mock('../../services/backupService');
vi.mock('../../services/marketService');
vi.mock('../../services/postService', () => ({
    postService: {
        syncPostChannels: vi.fn().mockResolvedValue(undefined),
    },
}));
vi.mock('../../services/settingsService');

const mockNavigateToSettingsSection = vi.fn();

vi.mock('../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigateToSettingsSection: mockNavigateToSettingsSection,
        appState: { view: 'logged_in' },
    }),
}));

const mockTriggerSecurityLockdown = vi.fn();

vi.mock('../../context/SessionContext', () => ({
    useSession: () => ({
        isSessionActive: true,
        triggerSecurityLockdown: mockTriggerSecurityLockdown,
    }),
}));

vi.mock('react-i18next', () => ({
    useTranslation: () => ({
        t: (key: string, defaultOrOptions?: string | Record<string, unknown>, options?: Record<string, unknown>) => {
            if (typeof defaultOrOptions === 'string') return defaultOrOptions;
            if (defaultOrOptions && typeof defaultOrOptions === 'object' && 'count' in defaultOrOptions) {
                return `${key}:${String(defaultOrOptions.count)}`;
            }
            if (options && 'count' in options) {
                return `${key}:${String(options.count)}`;
            }
            return key;
        },
    }),
}));

describe('StatusChip Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders "Alles in Ordnung" when backup, sync, and protection are healthy', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'synced',
            lastBackupAt: new Date().toISOString(),
            backupDirty: false,
            syncFolder: '/path/backup',
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: new Date().toISOString(),
            hasAuthorizedSyncBackupTarget: true,
        });
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 2,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 1,
            hasOwnSync: true,
            ownSyncSoftWarning: false,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });
        vi.mocked(settingsService.checkAnchorSync).mockResolvedValue('Synchronized');

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Alles in Ordnung')).toBeInTheDocument();
        });
    });

    it('renders recommendations when sync service is not configured', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'synced',
            lastBackupAt: new Date().toISOString(),
            backupDirty: false,
            syncFolder: '/path/backup',
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: new Date().toISOString(),
            hasAuthorizedSyncBackupTarget: true,
        });
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 0,
            pendingCount: 0,
            friendSyncs: 0,
            publicSyncs: 0,
            hasOwnSync: false,
            ownSyncSoftWarning: false,
            lastSyncAt: null,
        });
        vi.mocked(settingsService.checkAnchorSync).mockResolvedValue('Synchronized');

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('1 Empfehlung')).toBeInTheDocument();
        });
    });

    it('renders "Handlung nötig" when no backup exists at all', async () => {
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
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: true,
            ownSyncSoftWarning: false,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });
        vi.mocked(settingsService.checkAnchorSync).mockResolvedValue('Synchronized');

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Handlung nötig')).toBeInTheDocument();
        });
    });

    it('opens popover on click and deep-links to settings sections', async () => {
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
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 0,
            pendingCount: 0,
            friendSyncs: 0,
            publicSyncs: 0,
            hasOwnSync: false,
            ownSyncSoftWarning: false,
            lastSyncAt: null,
        });
        vi.mocked(settingsService.checkAnchorSync).mockResolvedValue('Synchronized');

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByTestId('status-chip-button')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('status-chip-button'));

        await waitFor(() => {
            expect(screen.getByTestId('status-chip-popover')).toBeInTheDocument();
        });

        // Check rows
        expect(screen.getByTestId('status-chip-row-backup')).toBeInTheDocument();
        expect(screen.getByTestId('status-chip-row-sync')).toBeInTheDocument();
        expect(screen.getByTestId('status-chip-row-protection')).toBeInTheDocument();

        // Click backup row setup button
        const backupRow = screen.getByTestId('status-chip-row-backup');
        const backupBtn = backupRow.querySelector('button');
        expect(backupBtn).not.toBeNull();
        if (backupBtn) {
            fireEvent.click(backupBtn);
            expect(mockNavigateToSettingsSection).toHaveBeenCalledWith('backup');
        }
    });

    it('triggers lockdown when critical L2 fork conflict is detected', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'synced',
            lastBackupAt: new Date().toISOString(),
            backupDirty: false,
            syncFolder: '/path',
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: new Date().toISOString(),
            hasAuthorizedSyncBackupTarget: true,
        });
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: true,
            ownSyncSoftWarning: false,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });
        vi.mocked(settingsService.checkAnchorSync).mockResolvedValue('ForkConflict');

        render(<StatusChip />);

        await waitFor(() => {
            expect(mockTriggerSecurityLockdown).toHaveBeenCalledWith('fork');
            expect(screen.getByText('Handlung nötig')).toBeInTheDocument();
        });
    });

    it('renders recommendation when active ads are expiring soon', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            status: 'synced',
            lastBackupAt: new Date().toISOString(),
            backupDirty: false,
            syncFolder: '/path',
            changedBlobs: 0,
            syncBackupEnabled: true,
            lastSyncBackupAt: new Date().toISOString(),
            hasAuthorizedSyncBackupTarget: true,
        });
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: true,
            ownSyncSoftWarning: false,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });
        vi.mocked(settingsService.checkAnchorSync).mockResolvedValue('Synchronized');
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValue({
            profile: { id: 'did:key:test' },
            visibility: {} as any,
            skills: [],
            ads: [
                {
                    id: 'ad1',
                    kind: 'offer',
                    title: 'Honey',
                    publishedAt: 100,
                    expiresAt: 200,
                    allowInquiries: true,
                    status: 'active',
                    daysLeft: 7,
                },
            ],
            maxHops: 2,
            allowInquiries: true,
            published: true,
            qrIsPartial: false,
        });

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('1 Empfehlung')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('status-chip-button'));
        expect(screen.getByTestId('status-chip-row-ads')).toBeInTheDocument();
        expect(screen.getByText('1 Inserat läuft bald ab.')).toBeInTheDocument();
    });
});
