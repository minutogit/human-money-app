/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
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
const mockTriggerSecurityLockdown = vi.fn();

vi.mock('../../context/SessionContext', () => ({
    useSession: () => ({
        isSessionActive: true,
        protectAction: mockProtectAction,
        triggerSecurityLockdown: mockTriggerSecurityLockdown,
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

    it('renders "Offline Mode" neutral badge with online sync tooltip when status is NoServers', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('NoServers');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText('Offline Mode')).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /L2 Status: Offline Mode/i });
        expect(badgeButton).toBeInTheDocument();
        expect(badgeButton.getAttribute('title')).toContain('No online sync configured');
    });

    it('renders "Online Sync Unreachable" amber badge when status is Offline', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('Offline');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText('Online Sync Unreachable')).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /L2 Status: Online Sync Unreachable/i });
        expect(badgeButton).toBeInTheDocument();
        expect(badgeButton.getAttribute('title')).toContain('Online sync servers are currently unreachable');
    });

    it('renders "Online Sync Active" emerald badge when status is Synchronized', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('Synchronized');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText(/Online Sync Active/i)).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /L2 Status: Online Sync Active/i });
        expect(badgeButton).toBeInTheDocument();
        expect(badgeButton.getAttribute('title')).toContain('Your wallet is verified with online protection.');
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

    it('renders "Local Backup Behind" amber badge for SameDeviceRollback and opens modal on click', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('SameDeviceRollback');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText('Local Backup Behind')).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /L2 Status: Local Backup Behind/i });
        fireEvent.click(badgeButton);

        await waitFor(() => {
            expect(screen.getByText('Update Local Backup')).toBeInTheDocument();
            expect(screen.getByText(/This device loaded an older backup/i)).toBeInTheDocument();
        });
    });

    it('triggers security lockdown on SupersededByHandover', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('SupersededByHandover');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText('Transferred to Other Device')).toBeInTheDocument();
            expect(mockTriggerSecurityLockdown).toHaveBeenCalledWith('handover');
        });
    });

    it('triggers security lockdown on ConcurrentCloneDetected', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('ConcurrentCloneDetected');

        render(<WalletSyncBadge />);

        await waitFor(() => {
            expect(screen.getByText('Clone Conflict')).toBeInTheDocument();
            expect(mockTriggerSecurityLockdown).toHaveBeenCalledWith('clone');
        });
    });
});
