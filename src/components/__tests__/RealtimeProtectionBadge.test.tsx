/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { RealtimeProtectionBadge } from '../RealtimeProtectionBadge';
import { settingsService } from '../../services/settingsService';

const mockNavigateToSettingsSection = vi.fn();
vi.mock('../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigateToSettingsSection: mockNavigateToSettingsSection,
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

describe('RealtimeProtectionBadge Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        (settingsService.getSettings as any).mockResolvedValue({
            activeNetwork: 'mainnet',
        });
    });

    it('renders "Offline protection" neutral badge when status is NoServers and opens popover', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('NoServers');

        render(<RealtimeProtectionBadge />);

        await waitFor(() => {
            expect(screen.getByText('Offline protection')).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /Echtzeitschutz Status: Offline protection/i });
        expect(badgeButton).toBeInTheDocument();

        fireEvent.click(badgeButton);
        expect(screen.getByRole('dialog')).toBeInTheDocument();
        expect(screen.getByText(/Payments are securely protected by mathematical Shared-Signature Traps/i)).toBeInTheDocument();

        const netBtn = screen.getByRole('button', { name: /Open network settings/i });
        fireEvent.click(netBtn);
        expect(mockNavigateToSettingsSection).toHaveBeenCalledWith('network');
    });

    it('closes popover on Escape key', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('NoServers');

        render(<RealtimeProtectionBadge />);

        await waitFor(() => {
            expect(screen.getByText('Offline protection')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Echtzeitschutz Status: Offline protection/i }));
        expect(screen.getByRole('dialog')).toBeInTheDocument();

        fireEvent.keyDown(document, { key: 'Escape' });
        await waitFor(() => {
            expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
        });
    });

    it('renders "Real-time protection active" emerald badge when status is Synchronized', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('Synchronized');

        render(<RealtimeProtectionBadge />);

        await waitFor(() => {
            expect(screen.getByText(/Real-time protection active/i)).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /Echtzeitschutz Status: Real-time protection active/i });
        expect(badgeButton).toBeInTheDocument();
    });

    it('displays Testnet pill when active network is testnet', async () => {
        (settingsService.getSettings as any).mockResolvedValue({
            activeNetwork: 'testnet',
        });
        (settingsService.checkAnchorSync as any).mockResolvedValue('NoServers');

        render(<RealtimeProtectionBadge />);

        await waitFor(() => {
            expect(screen.getByText(/Testnet/i)).toBeInTheDocument();
        });
    });

    it('renders "Security warning" rose badge for SameDeviceRollback and opens popover on click', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('SameDeviceRollback');

        render(<RealtimeProtectionBadge />);

        await waitFor(() => {
            expect(screen.getByText('Security warning')).toBeInTheDocument();
        });

        const badgeButton = screen.getByRole('button', { name: /Echtzeitschutz Status: Security warning/i });
        fireEvent.click(badgeButton);

        await waitFor(() => {
            expect(screen.getByRole('dialog')).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /Verify & synchronize vouchers|Gutscheine prüfen/i })).toBeInTheDocument();
        });
    });

    it('triggers security lockdown on SupersededByHandover', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('SupersededByHandover');

        render(<RealtimeProtectionBadge />);

        await waitFor(() => {
            expect(screen.getByText('Security warning')).toBeInTheDocument();
            expect(mockTriggerSecurityLockdown).toHaveBeenCalledWith('handover');
        });
    });

    it('triggers security lockdown on ConcurrentCloneDetected', async () => {
        (settingsService.checkAnchorSync as any).mockResolvedValue('ConcurrentCloneDetected');

        render(<RealtimeProtectionBadge />);

        await waitFor(() => {
            expect(screen.getByText('Security warning')).toBeInTheDocument();
            expect(mockTriggerSecurityLockdown).toHaveBeenCalledWith('clone');
        });
    });
});
