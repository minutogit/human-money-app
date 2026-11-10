import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { Dashboard } from '../Dashboard';
import { profileService } from '../../services/profileService';
import { voucherService } from '../../services/voucherService';
import { settingsService } from '../../services/settingsService';
import { transferService } from '../../services/transferService';
import { contactService } from '../../services/contactService';
import { marketService } from '../../services/marketService';
import { AggregatedBalance, VoucherSummary, AppSettings, IntegrityReport } from '../../types';

vi.mock('../../services/profileService');
vi.mock('../../services/voucherService');
vi.mock('../../services/settingsService');
vi.mock('../../services/transferService');
vi.mock('../../services/contactService');
vi.mock('../../services/marketService');
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
    writeText: vi.fn(),
}));

const mockNavigate = vi.fn();
const mockGoBack = vi.fn();

vi.mock('../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
        goBack: mockGoBack,
        appState: { view: 'logged_in' },
    }),
}));

let mockIntegrityReport: IntegrityReport | null = null;

vi.mock('../../context/SessionContext', () => ({
    useSession: () => ({
        isSessionActive: true,
        profileName: 'Alice',
        integrityReport: mockIntegrityReport,
        protectAction: vi.fn((fn: (pwd: string | null) => unknown) => fn('pwd')),
        lockSession: vi.fn(),
    }),
}));

vi.mock('react-i18next', () => ({
    useTranslation: () => ({
        t: (key: string, defaultOrOptions?: string | Record<string, unknown>, options?: Record<string, unknown>) => {
            const opts = (typeof defaultOrOptions === 'object' ? defaultOrOptions : options) || {};
            if (opts.amount !== undefined) return `${key}:${String(opts.amount)}`;
            if (opts.count !== undefined) return `${key}:${String(opts.count)}`;
            if (typeof defaultOrOptions === 'string') return defaultOrOptions;
            return key;
        },
        i18n: { language: 'en' },
    }),
}));

const mockBalances: AggregatedBalance[] = [
    {
        standardName: 'Minuto Standard',
        standardUuid: 'uuid-minuto',
        unit: 'MIN',
        totalAmount: '700',
        displayCurrency: 'Minuto',
        displayStandardName: 'Minuto Standard',
        isTestVoucher: false,
        receivedAmount: '0',
        ownAvailableAmount: '700',
        ownCirculatingAmount: '300',
        netBalance: '-300',
    }
];

const mockVouchers: VoucherSummary[] = [
    {
        localInstanceId: 'inst-1',
        creatorId: 'did:key:own',
        currentAmount: '700',
        unit: 'MIN',
        displayCurrency: 'Minuto',
        displayStandardName: 'Minuto Standard',
        rawStandardName: 'Minuto Standard',
        voucherStandardUuid: 'uuid-minuto',
        status: 'active',
        isTestVoucher: false,
        validUntil: new Date(Date.now() + 15 * 24 * 60 * 60 * 1000).toISOString(), // 15 days -> expiring soon!
        description: 'Test voucher',
        transactionCount: 1,
        signaturesCount: 2,
        hasCollateral: false,
        allowPartialTransfers: true,
    }
];

const mockSettings: AppSettings = {
    sessionTimeoutSeconds: 600,
    privacyDefault: 'ask',
    balanceViewMode: 'spendable',
};

describe('Dashboard Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
        mockIntegrityReport = null;
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 0,
            pendingCount: 0,
            friendSyncs: 0,
            publicSyncs: 0,
            hasOwnSync: false,
            ownSyncSoftWarning: false,
            lastSyncAt: null,
        });
        vi.mocked(profileService.getUserId).mockResolvedValue('did:key:own');
        vi.mocked(profileService.getProfile).mockResolvedValue({
            id: 'did:key:own',
            firstName: 'Alice',
            lastName: 'Tester',
        });
        vi.mocked(voucherService.getTotalBalanceByCurrency).mockResolvedValue(mockBalances);
        vi.mocked(voucherService.getSummaries).mockResolvedValue(mockVouchers);
        vi.mocked(settingsService.getSettings).mockResolvedValue(mockSettings);
        vi.mocked(settingsService.saveSettings).mockResolvedValue(undefined as never);
        vi.mocked(transferService.getHistory).mockResolvedValue([]);
        vi.mocked(transferService.getEventHistory).mockResolvedValue([]);
        vi.mocked(contactService.getContacts).mockResolvedValue([]);
    });

    it('renders spendable balance by default and toggles to net balance mode', async () => {
        render(<Dashboard />);

        await waitFor(() => {
            // Spendable mode shows totalAmount = 700.00 (or 700,00 depending on environment locale)
            expect(screen.getByText(/700[.,]00/)).toBeInTheDocument();
        });

        // Click Net Balance toggle button
        const netModeBtn = screen.getByText('dashboard.modeNet');
        fireEvent.click(netModeBtn);

        await waitFor(() => {
            // In net balance mode, it displays netBalance = -300.00
            expect(screen.getByText(/-300[.,]00/)).toBeInTheDocument();
            // And shows own available frame
            expect(screen.getByText(/dashboard\.ownFrameAvailable.*700[.,]00/i)).toBeInTheDocument();
        });

        // Local storage should remember choice
        expect(localStorage.getItem('hmc_balance_view_mode')).toBe('net');
    });

    it('shows balance breakdown popover when clicking the info button', async () => {
        render(<Dashboard />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });

        const infoBtn = screen.getByLabelText('dashboard.balanceDetailsTooltip');
        fireEvent.click(infoBtn);

        await waitFor(() => {
            expect(screen.getByText('dashboard.balanceBreakdownTitle')).toBeInTheDocument();
            expect(screen.getByText(/dashboard\.breakdownReceived/i)).toBeInTheDocument();
            expect(screen.getByText(/dashboard\.breakdownOwnAvailable/i)).toBeInTheDocument();
            expect(screen.getByText(/dashboard\.breakdownOwnCirculating/i)).toBeInTheDocument();
        });
    });

    it('displays expiring soon warning banner and navigates to wallet with filter', async () => {
        render(<Dashboard />);

        await waitFor(() => {
            expect(screen.getByText('dashboard.expiringSoonTitle:1')).toBeInTheDocument();
        });

        const bannerText = screen.getByText('dashboard.expiringSoonTitle:1');
        const bannerCard = bannerText.closest('.cursor-pointer');
        expect(bannerCard).not.toBeNull();
        if (bannerCard) {
            fireEvent.click(bannerCard);
            expect(mockNavigate).toHaveBeenCalledWith({
                view: 'wallet',
                initialStatusFilter: 'active',
                initialFilter: 'expiringSoon',
            });
        }
    });

    it('provides copy user ID button with title attribute', async () => {
        render(<Dashboard />);
        await waitFor(() => {
            const copyBtn = screen.getByTitle(/dashboard\.copyUserIdHint/i);
            expect(copyBtn).toBeInTheDocument();
        });
    });

    it('renders primary action bar at the top with send, receive, and create buttons', async () => {
        render(<Dashboard />);

        await waitFor(() => {
            expect(screen.getByTestId('dashboard-primary-actions')).toBeInTheDocument();
        });

        const sendBtn = screen.getByTestId('dashboard-action-send');
        const receiveBtn = screen.getByTestId('dashboard-action-receive');
        const createBtn = screen.getByTestId('dashboard-action-create');

        expect(sendBtn).toBeInTheDocument();
        expect(receiveBtn).toBeInTheDocument();
        expect(createBtn).toBeInTheDocument();

        fireEvent.click(sendBtn);
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'send_vouchers' });

        fireEvent.click(receiveBtn);
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'receive_bundle' });

        fireEvent.click(createBtn);
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'create_voucher', previousView: { view: 'logged_in' } });
    });

    it('displays concrete cause-based security warning with "Jetzt beheben" button and info hint', async () => {
        mockIntegrityReport = { type: 'manipulatedItems', items: ['item-1'] };
        render(<Dashboard />);

        await waitFor(() => {
            expect(screen.getByTestId('security-warning-card')).toBeInTheDocument();
        });

        expect(screen.getByText('Gutschein-Konflikt oder Datenabweichung erkannt')).toBeInTheDocument();
        expect(screen.getByTestId('fix-integrity-button')).toBeInTheDocument();

        // Clicking "Jetzt beheben" opens the integrity report modal
        fireEvent.click(screen.getByTestId('fix-integrity-button'));
        await waitFor(() => {
            expect(screen.getByText('integrity.manipulatedItemsTitle')).toBeInTheDocument();
        });
    });

    it('displays explanatory empty balance state with voucherBasics hint when balances are empty', async () => {
        vi.mocked(voucherService.getTotalBalanceByCurrency).mockResolvedValue([]);
        vi.mocked(voucherService.getSummaries).mockResolvedValue([]);

        render(<Dashboard />);

        await waitFor(() => {
            expect(screen.getByTestId('dashboard-empty-balances')).toBeInTheDocument();
        });

        expect(screen.getByText('Noch keine Gutscheine')).toBeInTheDocument();
        expect(screen.getByText(/Gutscheine sind Versprechen von Menschen/i)).toBeInTheDocument();

        // Check empty state buttons
        const createBtn = screen.getByTestId('empty-balance-create-btn');
        fireEvent.click(createBtn);
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'create_voucher', previousView: { view: 'logged_in' } });
    });
});
