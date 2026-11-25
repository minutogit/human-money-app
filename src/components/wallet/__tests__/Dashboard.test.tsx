import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { Dashboard } from '../Dashboard';
import { UiModeProvider } from '../../../context/UiModeContext';
import { profileService } from '../../../services/profileService';
import { voucherService } from '../../../services/voucherService';
import { settingsService } from '../../../services/settingsService';
import { transferService } from '../../../services/transferService';
import { contactService } from '../../../services/contactService';
import { marketService } from '../../../services/marketService';
import { recoveryService } from '../../../services/recoveryService';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { AggregatedBalance, VoucherSummary, AppConfigDto, IntegrityReport } from '../../../types';

vi.mock('../../../services/profileService');
vi.mock('../../../services/voucherService');
vi.mock('../../../services/settingsService');
vi.mock('../../../services/transferService');
vi.mock('../../../services/contactService');
vi.mock('../../../services/marketService');
vi.mock('../../../services/recoveryService');
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
    writeText: vi.fn(),
}));

const mockNavigate = vi.fn();
const mockGoBack = vi.fn();

vi.mock('../../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
        goBack: mockGoBack,
        appState: { view: 'logged_in' },
    }),
}));

let mockIntegrityReport: IntegrityReport | null = null;

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        isAuthenticated: true,
        hasCachedSession: true,
        profileName: 'Alice',
        integrityReport: mockIntegrityReport,
        protectAction: vi.fn((fn: (pwd: string | null) => unknown) => fn('pwd')),
        lockSession: vi.fn(),
    }),
    useOptionalSession: () => null,
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

const mockConfig: AppConfigDto = {
    uiPreferences: {
        uiMode: 'simple',
        lastUsedDirectory: null,
        balanceViewMode: 'spendable',
        marketShowUnknown: true,
        maxStoredCards: 2000,
        marketSendCardOnPayment: false,
        marketSyncEnabled: true,
        shareMinLevel: 1,
        marketPollWifiOnly: false,
        wotShareViaSync: true,
        postAcceptFromContacts: true,
        mediaAutoLoadMaxHops: 2,
    },
    securityPolicy: {
        sessionTimeoutSeconds: 600,
        privacyDefault: 'ask',
        devModeUnlocked: false,
        autoBackupEnabled: true,
        syncBackupEnabled: true,
        backupSyncFolder: null,
        backupProtectionMode: null,
        marketSelectedSyncs: [],
        publicSyncs: [],
    },
    lifecycle: {
        pendingRestoreL2Reconciliation: false,
        backupDirty: false,
        lastBackupAt: null,
        lastSyncBackupAt: null,
    },
};

describe('Dashboard Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
        mockIntegrityReport = null;
        vi.mocked(writeText).mockResolvedValue(undefined);
        vi.mocked(recoveryService.getRecoveryProtectionStatus).mockResolvedValue(null as never);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 0,
            pendingCount: 0,
            friendSyncs: 0,
            publicSyncs: 0,
            hasOwnSync: false,
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
        vi.mocked(settingsService.getConfig).mockResolvedValue(mockConfig);
        vi.mocked(settingsService.patchUiPreferences).mockResolvedValue(mockConfig.uiPreferences);
        vi.mocked(transferService.getHistory).mockResolvedValue([]);
        vi.mocked(transferService.getEventHistory).mockResolvedValue([]);
        vi.mocked(contactService.getContacts).mockResolvedValue([]);
    });

    it('renders spendable balance by default and toggles to net balance mode in expert mode', async () => {
        render(
            <UiModeProvider initialMode="expert">
                <Dashboard />
            </UiModeProvider>
        );

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

        // Preference change should persist via UI preferences instead of localStorage
        await waitFor(() => {
            expect(settingsService.patchUiPreferences).toHaveBeenCalledWith({ balanceViewMode: 'net' });
        });
    });

    it('hides net balance toggle and user ID pill in simple mode', async () => {
        render(
            <UiModeProvider initialMode="simple">
                <Dashboard />
            </UiModeProvider>
        );

        await waitFor(() => {
            expect(screen.getByText(/700[.,]00/)).toBeInTheDocument();
        });

        expect(screen.queryByText('dashboard.modeNet')).not.toBeInTheDocument();
        expect(screen.queryByTestId('tech-id')).not.toBeInTheDocument();
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
            expect(screen.getByText(/\+0[.,]00/)).toBeInTheDocument();
            expect(screen.getByText(/\+700[.,]00/)).toBeInTheDocument();
            expect(screen.getAllByText(/-300[.,]00/)).toHaveLength(2);
        });
        expect(mockNavigate).not.toHaveBeenCalled();
    });

    it('displays expiring soon warning banner and navigates to wallet with filter', async () => {
        render(<Dashboard />);

        await waitFor(() => {
            expect(screen.getByText('dashboard.expiringSoonTitle:1')).toBeInTheDocument();
        });

        const bannerText = screen.getByText('dashboard.expiringSoonTitle:1');
        const bannerCard = bannerText.closest('.cursor-pointer');
        expect(bannerCard).not.toBeNull();
        fireEvent.click(bannerCard as HTMLElement);
        expect(mockNavigate).toHaveBeenCalledWith({
            view: 'wallet',
            initialStatusFilter: 'active',
            initialFilter: 'expiringSoon',
        });
    });

    it('provides copyable TechId in expert mode', async () => {
        render(
            <UiModeProvider initialMode="expert">
                <Dashboard />
            </UiModeProvider>
        );
        await waitFor(() => {
            expect(screen.getByTestId('tech-id-value')).toHaveTextContent('did:key:own');
            expect(screen.getByTestId('tech-id-copy')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTestId('tech-id-copy'));
        await waitFor(() => {
            expect(writeText).toHaveBeenCalledWith('did:key:own');
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

    it('shows collapsible voucher overview with issuer and expiry, hiding full wallet button in simple mode and showing it in expert mode', async () => {
        const simpleRender = render(
            <UiModeProvider initialMode="simple">
                <Dashboard />
            </UiModeProvider>
        );

        const toggleBtn = await screen.findByTestId('dashboard-vouchers-toggle');
        expect(screen.queryByTestId('dashboard-vouchers-list')).not.toBeInTheDocument();

        fireEvent.click(toggleBtn);

        await waitFor(() => {
            expect(screen.getByTestId('dashboard-vouchers-list')).toBeInTheDocument();
        });

        const row = screen.getByTestId('dashboard-voucher-row');
        expect(row).toHaveTextContent(/700[.,]00 Minuto/);
        expect(row).toHaveTextContent('dashboard.voucherIssuedBy');
        expect(row).toHaveTextContent('dashboard.expiresInDays:15');
        expect(screen.queryByTestId('dashboard-open-wallet-btn')).not.toBeInTheDocument();

        fireEvent.click(row);
        expect(mockNavigate).toHaveBeenCalledWith({
            view: 'voucher_details',
            voucherId: 'inst-1',
            previousView: { view: 'logged_in' },
        });

        simpleRender.unmount();

        // Expert mode exposes button to open full voucher list with filters
        render(
            <UiModeProvider initialMode="expert">
                <Dashboard />
            </UiModeProvider>
        );

        fireEvent.click(await screen.findByTestId('dashboard-vouchers-toggle'));
        const openWalletBtn = await screen.findByTestId('dashboard-open-wallet-btn');
        fireEvent.click(openWalletBtn);
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'wallet' });
    });

    it('hides security warning card when integrityReport is null and maps invalidSignature report', async () => {
        mockIntegrityReport = null;
        const firstRender = render(<Dashboard />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });
        expect(screen.queryByTestId('security-warning-card')).not.toBeInTheDocument();
        firstRender.unmount();

        mockIntegrityReport = { type: 'invalidSignature' };
        render(<Dashboard />);

        await waitFor(() => {
            expect(screen.getByTestId('security-warning-card')).toBeInTheDocument();
        });
        expect(screen.getByText('Ungültige Profil-Signatur erkannt')).toBeInTheDocument();
    });

    it('wires FirstStepsChecklist completion props and hides checklist when all setup steps are complete', async () => {
        const firstRender = render(<Dashboard />);

        expect(await screen.findByTestId('first-steps-checklist')).toBeInTheDocument();
        firstRender.unmount();

        vi.mocked(contactService.getContacts).mockResolvedValue([
            { did: 'did:key:contact1', name: 'Bob', isFavorite: false } as never,
        ]);
        vi.mocked(marketService.getSyncOverviewStatus).mockResolvedValue({
            connectedCount: 1,
            pendingCount: 0,
            friendSyncs: 1,
            publicSyncs: 0,
            hasOwnSync: false,
            lastSyncAt: 1700000000,
        });
        vi.mocked(recoveryService.getRecoveryProtectionStatus).mockResolvedValue({
            hasVoucher: true,
            confirmedGuardiansCount: 0,
            requiredThreshold: 0,
            guardians: [],
            emergencySheetConfirmedAt: 1700000000,
            hasIdentityRecovery: true,
            syncBackupCount: 1,
            hasSyncBackup: true,
            hasLocalBackup: true,
            hasVoucherBackup: true,
            guardianWards: [],
            rehearsalDue: false,
        });
        vi.mocked(settingsService.getConfig).mockResolvedValue({
            ...mockConfig,
            lifecycle: {
                ...mockConfig.lifecycle,
                lastBackupAt: '2026-01-01T00:00:00Z',
            },
        });

        render(<Dashboard />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });
        expect(screen.queryByTestId('first-steps-checklist')).not.toBeInTheDocument();
    });
});
