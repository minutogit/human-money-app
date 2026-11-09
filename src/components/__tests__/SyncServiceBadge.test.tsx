/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SyncServiceBadge } from '../SyncServiceBadge';
import { marketService } from '../../services/marketService';

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

vi.mock('../../services/marketService', () => ({
    marketService: {
        getDropOverviewStatus: vi.fn(),
        syncMarketDrops: vi.fn(),
    },
}));

vi.mock('../../services/postService', () => ({
    postService: {
        syncPostDrops: vi.fn().mockResolvedValue(null),
    },
}));

describe('SyncServiceBadge Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders not connected state (count = 0) and opens popover on click', async () => {
        vi.mocked(marketService.getDropOverviewStatus).mockResolvedValue({
            connectedCount: 0,
            friendDrops: 0,
            publicDrops: 0,
            hasOwnDrop: false,
            ownDropSoftWarning: false,
            lastSyncAt: null,
        });

        render(<SyncServiceBadge />);

        await waitFor(() => {
            expect(screen.getByText('No sync service')).toBeInTheDocument();
        });

        // Click badge to open popover
        fireEvent.click(screen.getByRole('button', { name: /Sync-Dienst Status: No sync service/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText(/Automated background exchange for payments/i)).toBeInTheDocument();
        });

        // Click settings button
        const settingsBtn = screen.getByRole('button', { name: /Manage sync service/i });
        fireEvent.click(settingsBtn);

        expect(mockNavigateToSettingsSection).toHaveBeenCalledWith('sync');
    });

    it('closes popover on Escape key', async () => {
        vi.mocked(marketService.getDropOverviewStatus).mockResolvedValue({
            connectedCount: 0,
            friendDrops: 0,
            publicDrops: 0,
            hasOwnDrop: false,
            ownDropSoftWarning: false,
            lastSyncAt: null,
        });

        render(<SyncServiceBadge />);

        await waitFor(() => {
            expect(screen.getByText('No sync service')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Sync-Dienst Status: No sync service/i }));
        expect(screen.getByRole('dialog')).toBeInTheDocument();

        fireEvent.keyDown(document, { key: 'Escape' });
        await waitFor(() => {
            expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
        });
    });

    it('renders connected state (count = 2) and displays breakdown in popover', async () => {
        vi.mocked(marketService.getDropOverviewStatus).mockResolvedValue({
            connectedCount: 2,
            friendDrops: 1,
            publicDrops: 1,
            hasOwnDrop: false,
            ownDropSoftWarning: false,
            lastSyncAt: 1730000000,
        });

        render(<SyncServiceBadge />);

        await waitFor(() => {
            expect(screen.getByText('2 sync services connected')).toBeInTheDocument();
        });

        // Click badge to open connected popover
        fireEvent.click(screen.getByRole('button', { name: /Sync-Dienst Status: 2 sync services connected/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText('1 Freunde')).toBeInTheDocument();
            expect(screen.getByText('1 Öffentlich')).toBeInTheDocument();
        });

        // Click quick sync
        const syncBtn = screen.getByRole('button', { name: /Sync now|Jetzt synchronisieren/i });
        fireEvent.click(syncBtn);

        await waitFor(() => {
            expect(marketService.syncMarketDrops).toHaveBeenCalledWith({ forceUpload: true });
        });
    });

    it('renders soft warning state when own sync service storage is >= 80% full', async () => {
        vi.mocked(marketService.getDropOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            friendDrops: 0,
            publicDrops: 0,
            hasOwnDrop: true,
            ownDropSoftWarning: true,
            lastSyncAt: 1730000000,
        });

        render(<SyncServiceBadge />);

        await waitFor(() => {
            expect(screen.getByText('1 sync service connected')).toBeInTheDocument();
        });

        // Click badge to open popover
        fireEvent.click(screen.getByRole('button', { name: /Sync-Dienst Status: 1 sync service connected/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText(/Your own .* is 80% or more full|Dein eigener/i)).toBeInTheDocument();
        });
    });
});
