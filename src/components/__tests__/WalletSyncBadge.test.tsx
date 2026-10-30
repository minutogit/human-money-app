/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { WalletSyncBadge } from '../WalletSyncBadge';
import { settingsService } from '../../services/settingsService';

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

    const stableT = (key: string, defaultOrOptions?: any, options?: any) => {
        const actualOptions = typeof defaultOrOptions === 'object' ? defaultOrOptions : options;
        const defaultVal = typeof defaultOrOptions === 'string' ? defaultOrOptions : key;
        let val = (enTranslations as Record<string, string>)[key] || defaultVal;
        if (actualOptions && typeof actualOptions === 'object') {
            for (const [k, v] of Object.entries(actualOptions)) {
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

vi.mock('../../services/settingsService', () => ({
    settingsService: {
        checkAnchorSync: vi.fn(),
        getSettings: vi.fn(),
        reconcileRollback: vi.fn(),
    },
}));

const mockProtectAction = vi.fn(async (cb: (pw: string) => Promise<any>) => cb('mock-password'));

vi.mock('../../context/SessionContext', () => ({
    useSession: () => ({
        isSessionActive: true,
        protectAction: mockProtectAction,
    }),
}));

vi.mock('../../utils/log', () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
    },
}));

describe('WalletSyncBadge Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        (settingsService.getSettings as any).mockResolvedValue({
            activeNetwork: 'mainnet',
        });
    });

    it('renders "Offline Mode" neutral badge with SST V4 tooltip when status is NoServers', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('NoServers');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText('Offline Mode')).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /L2 Status: Offline Mode/i });
        expect(badgeButton).toBeInTheDocument();
        expect(badgeButton.getAttribute('title')).toContain('No Human Money Node servers are currently active');
    });

    it('renders "L2 Unreachable" amber badge when status is Offline', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('Offline');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText('L2 Unreachable')).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /L2 Status: L2 Unreachable/i });
        expect(badgeButton).toBeInTheDocument();
        expect(badgeButton.getAttribute('title')).toContain('Payments possible offline');
    });

    it('renders "L2 Synchronized" emerald badge when status is Synchronized', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('Synchronized');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText(/L2 Synchronized/i)).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /L2 Status: L2 Synchronized/i });
        expect(badgeButton).toBeInTheDocument();
        expect(badgeButton.getAttribute('title')).toContain('State verified on Layer 2');
    });

    it('displays Testnet pill when active network is testnet', async () => {
        (settingsService.getSettings as any).mockResolvedValue({
            activeNetwork: 'testnet',
        });
        (settingsService.checkAnchorSync as any).mockResolvedValue('NoServers');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText(/Testnet/i)).toBeInTheDocument();
        });
    });
});
