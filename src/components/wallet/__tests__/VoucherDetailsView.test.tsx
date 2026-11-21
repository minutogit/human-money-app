import { render, screen, waitFor, fireEvent, within } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { VoucherDetailsView } from '../VoucherDetailsView';
import { UiModeProvider } from '../../../context/UiModeContext';
import { voucherService } from '../../../services/voucherService';
import { settingsService } from '../../../services/settingsService';
import { profileService } from '../../../services/profileService';
import { standardsService } from '../../../services/standardsService';
import { contactService } from '../../../services/contactService';
import type { VoucherDetails, VoucherL2StatusDto } from '../../../types';

const { mockNavigate, mockGoBack } = vi.hoisted(() => ({
    mockNavigate: vi.fn(),
    mockGoBack: vi.fn(),
}));

vi.mock('../../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
        goBack: mockGoBack,
        appState: { view: 'voucher_details', voucherId: 'local-v1' },
    }),
}));

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        protectAction: async (fn: (pwd: string | null) => unknown) => fn('pw'),
    }),
    useOptionalSession: () => null,
}));

vi.mock('../../../utils/log', () => ({
    logger: {
        debug: vi.fn(),
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
    },
}));

vi.mock('../../../services/voucherService', () => ({
    voucherService: {
        getDetails: vi.fn(),
        checkReputation: vi.fn(),
        getProofId: vi.fn(),
    },
}));

vi.mock('../../../services/settingsService', () => ({
    settingsService: {
        getConfig: vi.fn(),
        checkVoucherL2Status: vi.fn(),
    },
}));

vi.mock('../../../services/profileService', () => ({
    profileService: {
        getProfile: vi.fn(),
        getUserId: vi.fn(),
    },
}));

vi.mock('../../../services/standardsService', () => ({
    standardsService: {
        getStandards: vi.fn(),
        parseStandard: vi.fn(),
    },
}));

vi.mock('../../../services/contactService', () => ({
    contactService: {
        getContacts: vi.fn(),
    },
}));

const mockAnchoredDetails: VoucherDetails = {
    localInstanceId: 'local-v1',
    status: 'active',
    currentAmount: '50',
    displayCurrency: 'Taler',
    displayStandardName: 'FreeTaler',
    isTestVoucher: false,
    voucher: {
        voucherStandard: {
            name: 'FreeTaler',
            uuid: 'a1b2c3d4-e5f6-4789-8012-3456789abcde',
            standardDefinitionHash: 'std-hash',
        },
        voucherId: 'v-12345678-abcd',
        voucherNonce: 'nonce-1',
        creationDate: '2026-01-01T00:00:00Z',
        validUntil: '2028-01-01T00:00:00Z',
        nonRedeemableTestVoucher: false,
        nominalValue: {
            amount: '50',
            unit: 'Taler',
            abbreviation: 'Taler',
        },
        creator: {
            id: 'did:key:zIssuer',
            firstName: 'Alice',
            lastName: 'Issuer',
        },
        signatures: [],
        transactions: [],
        layer2Anchor: 'aa'.repeat(32),
    },
};

describe('VoucherDetailsView L2 unreachable holder guidance (S04-Q2)', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(voucherService.getDetails).mockResolvedValue(mockAnchoredDetails);
        vi.mocked(voucherService.checkReputation).mockResolvedValue('clean');
        vi.mocked(voucherService.getProofId).mockResolvedValue(null);
        vi.mocked(settingsService.getConfig).mockResolvedValue(null as never);
        vi.mocked(profileService.getProfile).mockResolvedValue(null as never);
        vi.mocked(profileService.getUserId).mockResolvedValue('did:key:zHolder');
        vi.mocked(standardsService.getStandards).mockResolvedValue([]);
        vi.mocked(contactService.getContacts).mockResolvedValue([]);
    });

    it('renders actionable 3-step guidance, recheck button, and issuer return CTA on offline L2 status', async () => {
        const offlineStatus: VoucherL2StatusDto = {
            status: 'offline',
            archived: false,
        };
        vi.mocked(settingsService.checkVoucherL2Status).mockResolvedValue(offlineStatus);

        render(
            <UiModeProvider initialMode="expert">
                <VoucherDetailsView />
            </UiModeProvider>,
        );

        const headerCheckBtn = await screen.findByRole('button', { name: /Check online status/i });
        fireEvent.click(headerCheckBtn);

        const banner = await screen.findByTestId('voucher-l2-unreachable-banner');
        expect(banner).toBeInTheDocument();
        expect(within(banner).getAllByRole('listitem')).toHaveLength(3);

        fireEvent.click(screen.getByTestId('voucher-l2-recheck-btn'));
        await waitFor(() => {
            expect(settingsService.checkVoucherL2Status).toHaveBeenCalledTimes(2);
        });

        fireEvent.click(screen.getByTestId('voucher-l2-return-issuer-btn'));
        expect(mockNavigate).toHaveBeenCalledWith({
            view: 'send_vouchers',
            initialRecipientId: 'did:key:zIssuer',
            initialVoucherId: 'local-v1',
            initialNote: expect.stringContaining('v-123456'),
        });
    });

    it.each(['inconclusive', 'no_servers'] as const)(
        'also renders holder guidance banner for %s L2 status',
        async (status) => {
            vi.mocked(settingsService.checkVoucherL2Status).mockResolvedValue({
                status,
                archived: false,
            });

            render(
                <UiModeProvider initialMode="expert">
                    <VoucherDetailsView />
                </UiModeProvider>,
            );

            const headerCheckBtn = await screen.findByRole('button', {
                name: /Check online status/i,
            });
            fireEvent.click(headerCheckBtn);

            expect(await screen.findByTestId('voucher-l2-unreachable-banner')).toBeInTheDocument();
        },
    );
});
