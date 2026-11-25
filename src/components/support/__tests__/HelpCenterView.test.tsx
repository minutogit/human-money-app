import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { HelpCenterView } from '../HelpCenterView';
import { UiModeProvider } from '../../../context/UiModeContext';
import enJson from '../../../locales/en.json';

const en = enJson as Record<string, string>;

const mockNavigate = vi.fn();
const mockGoBack = vi.fn();
let mockIsAuthenticated = false;

vi.mock('../../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
        goBack: mockGoBack,
        appState: { view: 'help_center' },
    }),
}));

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        isAuthenticated: mockIsAuthenticated,
    }),
    useOptionalSession: () => ({
        isAuthenticated: mockIsAuthenticated,
        hasCachedSession: false,
    }),
}));

describe('HelpCenterView Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockIsAuthenticated = false;
    });

    it('renders header, search bar, and category filters', () => {
        render(<HelpCenterView />);

        expect(screen.getByPlaceholderText(en['helpCenter.searchPlaceholder'])).toBeInTheDocument();
        expect(screen.getByRole('button', { name: en['helpCenter.categoryAll'] })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: en['helpCenter.groupFirstSteps'] })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: en['helpCenter.groupDailyVouchers'] })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: en['helpCenter.groupContactsTrust'] })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: en['helpCenter.groupFaq'] })).toBeInTheDocument();
        expect(screen.queryByRole('button', { name: en['helpCenter.groupExpert'] })).not.toBeInTheDocument();
    });

    it('renders deep-linked initial topic', () => {
        render(<HelpCenterView initialTopic="wotNetwork" />);

        expect(screen.getAllByText(en['help.wotNetwork.title']).length).toBeGreaterThanOrEqual(1);
        expect(screen.getAllByText(en['help.wotNetwork.short']).length).toBeGreaterThanOrEqual(1);
    });

    it('filters topics by search query', () => {
        render(<HelpCenterView />);

        const searchInput = screen.getByPlaceholderText(en['helpCenter.searchPlaceholder']);
        fireEvent.change(searchInput, { target: { value: 'Seed Phrase' } });

        expect(screen.getAllByText(en['help.mnemonic.title']).length).toBeGreaterThanOrEqual(1);

        // Clear button clears search
        const clearBtn = screen.getByLabelText(en['helpCenter.clearSearch']);
        fireEvent.click(clearBtn);
        expect(searchInput).toHaveValue('');
    });

    it('filters topics by category pill click', () => {
        render(<HelpCenterView />);

        const vouchersGroupBtn = screen.getByRole('button', { name: en['helpCenter.groupDailyVouchers'] });
        fireEvent.click(vouchersGroupBtn);

        expect(screen.getAllByText(en['help.voucherMode.title']).length).toBeGreaterThanOrEqual(1);
    });

    it('switches topic when related topic chip is clicked', () => {
        render(<HelpCenterView initialTopic="conceptParadigm" />);

        const voucherBasicsTitle = en['help.voucherBasics.title'];
        const chip = screen.getByRole('button', { name: voucherBasicsTitle });
        expect(chip).toBeInTheDocument();

        fireEvent.click(chip);

        expect(screen.getAllByText(en['help.voucherBasics.short']).length).toBeGreaterThanOrEqual(1);
    });

    it('calls onBack when back button is clicked in auth mode', () => {
        const onBack = vi.fn();
        render(<HelpCenterView onBack={onBack} />);

        const backBtn = screen.getByText(en['helpCenter.backToLogin']);
        fireEvent.click(backBtn);
        expect(onBack).toHaveBeenCalled();
    });

    it('renders in PageLayout when user is authenticated', () => {
        mockIsAuthenticated = true;
        render(<HelpCenterView />);

        expect(screen.getAllByText(en['helpCenter.title']).length).toBeGreaterThanOrEqual(1);
        expect(screen.getAllByText(en['helpCenter.subtitle']).length).toBeGreaterThanOrEqual(1);
    });

    it('shows expert topics via disclosure in Simple mode and as category tab in Expert mode', () => {
        const { unmount } = render(
            <UiModeProvider initialMode="simple">
                <HelpCenterView />
            </UiModeProvider>
        );

        expect(screen.queryByRole('button', { name: en['helpCenter.groupExpert'] })).not.toBeInTheDocument();
        expect(screen.queryByTestId('help-center-expert-topics')).not.toBeInTheDocument();
        fireEvent.click(screen.getByTestId('help-center-expert-topics-toggle'));
        expect(screen.getByTestId('help-center-expert-topics')).toBeInTheDocument();
        unmount();

        render(
            <UiModeProvider initialMode="expert">
                <HelpCenterView />
            </UiModeProvider>
        );
        const expertPill = screen.getByRole('button', { name: en['helpCenter.groupExpert'] });
        expect(expertPill).toBeInTheDocument();
        fireEvent.click(expertPill);
        expect(screen.queryByTestId('help-center-expert-topics-toggle')).not.toBeInTheDocument();
    });

    it('displays empty-results message for unmatched search query and restores topics on clear', () => {
        render(<HelpCenterView />);

        const searchInput = screen.getByPlaceholderText(en['helpCenter.searchPlaceholder']);
        fireEvent.change(searchInput, { target: { value: 'zzzz-unmatched-query-999' } });

        expect(screen.getByText(en['helpCenter.noResults'])).toBeInTheDocument();

        fireEvent.click(screen.getByLabelText(en['helpCenter.clearSearch']));
        expect(screen.queryByText(en['helpCenter.noResults'])).not.toBeInTheDocument();
        expect(screen.getByTestId('help-group-heading-firstSteps')).toBeInTheDocument();
    });
});
