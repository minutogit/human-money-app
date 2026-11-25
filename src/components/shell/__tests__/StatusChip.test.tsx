import { render, screen, waitFor, fireEvent, act, within } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { StatusChip } from '../StatusChip';
import { backupService } from '../../../services/backupService';
import { marketService } from '../../../services/marketService';
import { integrityService } from '../../../services/integrityService';
import { postService } from '../../../services/postService';
import { APP_EVENTS, emitAppEvent } from '../../../utils/appEvents';
import { notifySyncStatusUpdated } from '../../../utils/syncEvents';

vi.mock('../../../services/backupService');
vi.mock('../../../services/marketService');
vi.mock('../../../services/integrityService');
vi.mock('../../../services/postService', () => ({
    postService: {
        syncPostChannels: vi.fn().mockResolvedValue(undefined),
    },
}));

const mockNavigateToSettingsSection = vi.fn();
const mockNavigate = vi.fn();

vi.mock('../../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigateToSettingsSection: mockNavigateToSettingsSection,
        navigate: mockNavigate,
        appState: { view: 'logged_in' },
    }),
}));

const mockTriggerSecurityLockdown = vi.fn();

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        isAuthenticated: true,
        hasCachedSession: true,
        triggerSecurityLockdown: mockTriggerSecurityLockdown,
    }),
}));

vi.mock('react-i18next', () => ({
    useTranslation: () => ({
        t: (key: string, defaultOrOptions?: string | Record<string, unknown>, options?: Record<string, unknown>) => {
            if (typeof defaultOrOptions === 'string') {
                if (options && 'count' in options) {
                    return defaultOrOptions.replace('{{count}}', String(options.count));
                }
                return defaultOrOptions;
            }
            if (defaultOrOptions && typeof defaultOrOptions === 'object') {
                if ('count' in defaultOrOptions) {
                    return `${key}:${String(defaultOrOptions.count)}`;
                }
                if ('days' in defaultOrOptions) {
                    return `${key}:${String(defaultOrOptions.days)}`;
                }
            }
            if (options && 'count' in options) {
                return `${key}:${String(options.count)}`;
            }
            return key;
        },
    }),
}));

describe('StatusChip Component', () => {
    const healthyBackup = {
        status: 'synced' as const,
        lastBackupAt: new Date().toISOString(),
        backupDirty: false,
        syncFolder: '/path/backup',
        changedBlobs: 0,
        syncBackupEnabled: true,
        lastSyncBackupAt: new Date().toISOString(),
        hasAuthorizedSyncBackupTarget: true,
    };

    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(integrityService.getDoubleSpendConflicts).mockResolvedValue([]);
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValue(undefined as never);
        vi.mocked(marketService.syncMarketChannels).mockReset();
        vi.mocked(postService.syncPostChannels).mockResolvedValue(undefined as never);
    });

    it('renders "Alles in Ordnung" when backup, sync, and voucher protection are healthy', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue(healthyBackup);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 2,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 1,
            hasOwnSync: true,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Alles in Ordnung')).toBeInTheDocument();
        });
    });

    it('stays healthy ("Alles in Ordnung") in local mode when sync service is simply optional', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue(healthyBackup);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 0,
            pendingCount: 0,
            pendingAuthCount: 0,
            failingCount: 0,
            friendSyncs: 0,
            publicSyncs: 0,
            hasOwnSync: false,
            lastSyncAt: null,
        });

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Alles in Ordnung')).toBeInTheDocument();
        });
    });

    it('renders "1 Empfehlung" when a configured sync service is failing', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue(healthyBackup);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            failingCount: 1,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: true,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });

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
            lastSyncAt: Math.floor(Date.now() / 1000),
        });

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Handlung nötig')).toBeInTheDocument();
        });
    });

    it('opens popover on click and deep-links to settings and wallet sections', async () => {
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
            lastSyncAt: null,
        });

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByTestId('status-chip-button')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('status-chip-button'));

        await waitFor(() => {
            expect(screen.getByTestId('status-chip-popover')).toBeInTheDocument();
        });

        expect(screen.getByTestId('status-chip-row-backup')).toBeInTheDocument();
        expect(screen.getByTestId('status-chip-row-sync')).toBeInTheDocument();
        expect(screen.getByTestId('status-chip-row-protection')).toBeInTheDocument();

        const backupRow = screen.getByTestId('status-chip-row-backup');
        const backupBtn = within(backupRow).getByRole('button', { name: 'Einrichten' });
        fireEvent.click(backupBtn);
        expect(mockNavigateToSettingsSection).toHaveBeenCalledWith('backup');
        expect(screen.queryByTestId('status-chip-popover')).toBeNull();
    });

    it('renders "Handlung nötig" and routes to conflict_list when double-spend conflicts exist', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue(healthyBackup);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: true,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });
        vi.mocked(integrityService.getDoubleSpendConflicts).mockResolvedValue([
            { proofId: 'proof-1' } as never,
        ]);

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Handlung nötig')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('status-chip-button'));
        const protRow = screen.getByTestId('status-chip-row-protection');
        const protBtn = within(protRow).getByRole('button', { name: 'Konflikte prüfen' });
        fireEvent.click(protBtn);
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'conflict_list' });
        expect(screen.queryByTestId('status-chip-popover')).toBeNull();
    });

    it('renders recommendation when active ads are expiring soon', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue(healthyBackup);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: true,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValue({
            profile: { id: 'did:key:test' },
            visibility: {} as never,
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
                    published: true,
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
        expect(screen.getByText('statusChip.oneAdExpiring:7')).toBeInTheDocument();
    });

    it('refreshes status via APP_EVENTS and triggers gentle daily sync at most once', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue(healthyBackup);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: true,
            lastSyncAt: null,
        });
        vi.mocked(marketService.syncMarketChannels).mockResolvedValue({
            status: 'completed',
            uploadedTo: 0,
            polledSyncs: 1,
            decryptedSlots: 0,
            importedCards: 0,
            importedWotLists: 0,
            errors: [],
        });
        vi.mocked(postService.syncPostChannels).mockResolvedValue({
            uploadedTo: 0,
            polledSyncs: 1,
            importedPayments: 0,
            importedReceipts: 0,
            importedSignatures: 0,
            newSignRequests: 0,
            newWotHandshakes: 0,
            newInquiries: 0,
            newInquiryMessages: 0,
            prunedBoxes: 0,
            errors: [],
        });

        render(<StatusChip />);

        await waitFor(() => {
            expect(marketService.syncMarketChannels).toHaveBeenCalledTimes(1);
            expect(postService.syncPostChannels).toHaveBeenCalledTimes(1);
        });

        const backupCallsBefore = vi.mocked(backupService.getBackupStatus).mock.calls.length;
        const syncCallsBefore = vi.mocked(marketService.getSyncOverviewStatus).mock.calls.length;

        await act(async () => {
            emitAppEvent(APP_EVENTS.SYNC_COMPLETED, { source: 'post' });
        });

        await waitFor(() => {
            expect(backupService.getBackupStatus).toHaveBeenCalledTimes(backupCallsBefore + 1);
            expect(marketService.getSyncOverviewStatus).toHaveBeenCalledTimes(syncCallsBefore + 1);
        });
        // Daily sync must not re-fire on subsequent refreshes
        expect(marketService.syncMarketChannels).toHaveBeenCalledTimes(1);
    });

    it('ignores witness and locally overridden conflicts and routes protection button to wallet', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue(healthyBackup);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: true,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });
        vi.mocked(integrityService.getDoubleSpendConflicts).mockResolvedValue([
            { proofId: 'witness-1', conflictRole: 'witness' } as never,
            { proofId: 'override-1', conflictRole: 'victim', localOverride: 'dismissed' } as never,
        ]);

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Alles in Ordnung')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('status-chip-button'));
        const protRow = screen.getByTestId('status-chip-row-protection');
        fireEvent.click(within(protRow).getByRole('button', { name: 'Meine Gutscheine' }));
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'wallet' });
    });

    it('keeps manual pull-sync disabled while in-flight and prevents duplicate triggers', async () => {
        let resolveMarket!: (value: Awaited<ReturnType<typeof marketService.syncMarketChannels>>) => void;
        let resolvePost!: (value: Awaited<ReturnType<typeof postService.syncPostChannels>>) => void;

        vi.mocked(backupService.getBackupStatus).mockResolvedValue(healthyBackup);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 2,
            pendingCount: 0,
            friendSyncs: 2,
            publicSyncs: 0,
            hasOwnSync: true,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });
        vi.mocked(marketService.syncMarketChannels).mockImplementation(
            () => new Promise((resolve) => { resolveMarket = resolve; })
        );
        vi.mocked(postService.syncPostChannels).mockImplementation(
            () => new Promise((resolve) => { resolvePost = resolve; })
        );

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Alles in Ordnung')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('status-chip-button'));
        const syncBtn = screen.getByTestId('status-chip-sync-now-btn');
        fireEvent.click(syncBtn);
        expect(syncBtn).toBeDisabled();
        fireEvent.click(syncBtn);
        expect(marketService.syncMarketChannels).toHaveBeenCalledTimes(1);
        expect(postService.syncPostChannels).toHaveBeenCalledTimes(1);

        await act(async () => {
            resolveMarket({
                status: 'completed',
                uploadedTo: 0,
                polledSyncs: 1,
                decryptedSlots: 0,
                importedCards: 0,
                importedWotLists: 0,
                errors: [],
            });
            resolvePost({
                uploadedTo: 0,
                polledSyncs: 1,
                importedPayments: 0,
                importedReceipts: 0,
                importedSignatures: 0,
                newSignRequests: 0,
                newWotHandshakes: 0,
                newInquiries: 0,
                newInquiryMessages: 0,
                prunedBoxes: 0,
                errors: [],
            });
        });

        await waitFor(() => {
            expect(syncBtn).not.toBeDisabled();
        });
    });

    it('renders critical folderDisconnected state and aggregated multi-warning recommendations with ad routing', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue({
            ...healthyBackup,
            status: 'folderDisconnected',
        });
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 0,
            pendingCount: 1,
            pendingAuthCount: 1,
            failingCount: 0,
            friendSyncs: 0,
            publicSyncs: 0,
            hasOwnSync: false,
            lastSyncAt: null,
        });
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValue({
            profile: { id: 'did:key:test' },
            visibility: {} as never,
            skills: [],
            ads: [
                {
                    id: 'ad1',
                    kind: 'offer',
                    title: 'Apples',
                    publishedAt: 100,
                    expiresAt: 200,
                    allowInquiries: true,
                    status: 'active',
                    daysLeft: 12,
                    published: true,
                },
                {
                    id: 'ad2',
                    kind: 'request',
                    title: 'Repair',
                    publishedAt: 100,
                    expiresAt: 200,
                    allowInquiries: true,
                    status: 'active',
                    daysLeft: 4,
                    published: true,
                },
            ],
            maxHops: 2,
            allowInquiries: true,
            published: true,
            qrIsPartial: false,
        });

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Handlung nötig')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('status-chip-button'));
        expect(screen.getByText('Backup-Ordner ist nicht erreichbar.')).toBeInTheDocument();
        expect(screen.getByText('Sync wartet auf Freigabe.')).toBeInTheDocument();
        expect(screen.getByText('statusChip.adsExpiring:2')).toBeInTheDocument();

        // Transition backup via BACKUP_UPDATED event into dirty warning state -> 3 warnings total
        await act(async () => {
            emitAppEvent(APP_EVENTS.BACKUP_UPDATED, {
                ...healthyBackup,
                status: 'synced',
                backupDirty: true,
            });
        });

        await waitFor(() => {
            expect(screen.getByText('3 Empfehlungen')).toBeInTheDocument();
        });

        const adsRow = screen.getByTestId('status-chip-row-ads');
        fireEvent.click(within(adsRow).getByRole('button', { name: 'Öffnen' }));
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'marketplace', initialTab: 'mine' });
    });

    it('closes popover on Escape or outside mousedown and updates via SYNC_STATUS_UPDATED_EVENT', async () => {
        vi.mocked(backupService.getBackupStatus).mockResolvedValue(healthyBackup);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: true,
            lastSyncAt: Math.floor(Date.now() / 1000),
        });

        render(<StatusChip />);

        await waitFor(() => {
            expect(screen.getByText('Alles in Ordnung')).toBeInTheDocument();
        });

        const trigger = screen.getByTestId('status-chip-button');
        fireEvent.click(trigger);
        expect(screen.getByTestId('status-chip-popover')).toBeInTheDocument();

        fireEvent.keyDown(document, { key: 'Escape' });
        expect(screen.queryByTestId('status-chip-popover')).toBeNull();
        expect(document.activeElement).toBe(trigger);

        fireEvent.click(trigger);
        expect(screen.getByTestId('status-chip-popover')).toBeInTheDocument();

        fireEvent.mouseDown(document.body);
        expect(screen.queryByTestId('status-chip-popover')).toBeNull();

        await act(async () => {
            notifySyncStatusUpdated({
                connectedCount: 1,
                pendingCount: 0,
                failingCount: 1,
                friendSyncs: 1,
                publicSyncs: 0,
                hasOwnSync: true,
                lastSyncAt: Math.floor(Date.now() / 1000),
            });
        });

        await waitFor(() => {
            expect(screen.getByText('1 Empfehlung')).toBeInTheDocument();
        });
    });
});
