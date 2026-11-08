/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { DropStatusBadge } from '../DropStatusBadge';
import { marketService } from '../../services/marketService';
import * as clipboardUtils from '../../utils/clipboard';

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

vi.mock('../../utils/clipboard', () => ({
    copyToClipboard: vi.fn(() => Promise.resolve(true)),
}));

describe('DropStatusBadge Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders not connected state (count = 0) and opens info modal with §7.6 text and invite button', async () => {
        vi.mocked(marketService.getDropOverviewStatus).mockResolvedValue({
            connectedCount: 0,
            friendDrops: 0,
            publicDrops: 0,
            hasOwnDrop: false,
            ownDropSoftWarning: false,
            lastSyncAt: null,
        });

        render(<DropStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('No Drop')).toBeInTheDocument();
        });

        // Click badge to open popover
        fireEvent.click(screen.getByRole('button', { name: /Ablage Status: No Drop/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText(/The Human Money App works completely without the internet/i)).toBeInTheDocument();
        });

        // Click copy invite button
        const inviteBtn = screen.getByRole('button', { name: /Ask a Friend/i });
        fireEvent.click(inviteBtn);

        await waitFor(() => {
            expect(clipboardUtils.copyToClipboard).toHaveBeenCalledWith(
                expect.stringContaining('Human Money App')
            );
        });

        // Click settings button
        const settingsBtn = screen.getByRole('button', { name: /Drop Settings/i });
        fireEvent.click(settingsBtn);

        expect(mockNavigate).toHaveBeenCalledWith({
            view: 'settings',
            openDropsModal: true,
            openDropsTab: undefined,
        });
    });

    it('renders connected state (count = 2) and displays breakdown in modal', async () => {
        vi.mocked(marketService.getDropOverviewStatus).mockResolvedValue({
            connectedCount: 2,
            friendDrops: 1,
            publicDrops: 1,
            hasOwnDrop: false,
            ownDropSoftWarning: false,
            lastSyncAt: 1730000000,
        });

        render(<DropStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('2 Drops Connected')).toBeInTheDocument();
        });

        // Click badge to open connected modal
        fireEvent.click(screen.getByRole('button', { name: /Ablage Status: 2 Drops Connected/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText('Drop Status')).toBeInTheDocument();
            expect(screen.getByText('Friends')).toBeInTheDocument();
            expect(screen.getByText('Public')).toBeInTheDocument();
            expect(screen.getByText('Own')).toBeInTheDocument();
        });

        // Click quick sync
        const syncBtn = screen.getByRole('button', { name: /Sync Now|Jetzt synchronisieren/i });
        fireEvent.click(syncBtn);

        await waitFor(() => {
            expect(marketService.syncMarketDrops).toHaveBeenCalledWith({ forceUpload: true });
        });
    });

    it('renders soft warning state when own drop storage is >= 80% full', async () => {
        vi.mocked(marketService.getDropOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            friendDrops: 0,
            publicDrops: 0,
            hasOwnDrop: true,
            ownDropSoftWarning: true,
            lastSyncAt: 1730000000,
        });

        render(<DropStatusBadge />);

        await waitFor(() => {
            expect(screen.getByText('1 Drop Connected')).toBeInTheDocument();
        });

        // Click badge to open modal
        fireEvent.click(screen.getByRole('button', { name: /Ablage Status: 1 Drop Connected/i }));

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByText(/Storage Limit Nearly Reached/i)).toBeInTheDocument();
            expect(screen.getByText(/Your own drop is 80% or more full/i)).toBeInTheDocument();
        });
    });
});
