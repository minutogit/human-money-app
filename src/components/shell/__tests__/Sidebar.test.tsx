import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { Sidebar } from '../Sidebar';
import { UI_MODE_STORAGE_KEY } from '../../../context/UiModeContext';
import { integrityService } from '../../../services/integrityService';
import { profileService } from '../../../services/profileService';

vi.mock('../../../services/integrityService');
vi.mock('../../../services/profileService');
vi.mock('@tauri-apps/plugin-log', () => ({
    error: vi.fn(),
}));

const mockNavigate = vi.fn();
const mockSetSidebarOpen = vi.fn();

vi.mock('../../../context/NavigationContext', () => ({
    useNavigation: () => ({
        navigate: mockNavigate,
        appVersion: '0.5.1',
        appState: { view: 'logged_in' },
        isSidebarOpen: true,
        setSidebarOpen: mockSetSidebarOpen,
    }),
}));

const mockNotifyLogout = vi.fn();
const mockLockSession = vi.fn();

vi.mock('../../../context/SessionContext', () => ({
    useSession: () => ({
        isSessionActive: true,
        isAuthenticated: true,
        profileName: 'Alice',
        notifyLogout: mockNotifyLogout,
        protectAction: vi.fn(),
        lockSession: mockLockSession,
    }),
}));

vi.mock('react-i18next', () => ({
    useTranslation: () => ({
        t: (key: string, defaultOrOptions?: string | Record<string, unknown>) => {
            if (typeof defaultOrOptions === 'string') return defaultOrOptions;
            return key;
        },
    }),
}));

describe('Sidebar Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
        vi.mocked(integrityService.getDoubleSpendConflicts).mockResolvedValue([]);
        vi.mocked(profileService.getUserId).mockResolvedValue('did:key:z6MkAlice');
    });

    it('renders 5+3 simple navigation including wallet, without duplicate appName, Mehr disclosure, or user DID by default', () => {
        render(<Sidebar openClaimsCount={0} />);

        // 5 main entries in Simple mode
        expect(screen.getByText('Übersicht')).toBeInTheDocument();
        expect(screen.getByText('dashboard.title')).toBeInTheDocument();
        expect(screen.getByText('wallet.title')).toBeInTheDocument();
        expect(screen.getByText('history.transactionHistory')).toBeInTheDocument();

        expect(screen.getByText('Menschen')).toBeInTheDocument();
        expect(screen.getByText('contacts.addressBook')).toBeInTheDocument();
        expect(screen.getByText('marketplace.title')).toBeInTheDocument();

        // Expert-only & duplicate elements hidden in Simple mode
        expect(screen.queryByText('common.appName')).not.toBeInTheDocument();
        expect(screen.queryByTestId('sidebar-more-toggle')).not.toBeInTheDocument();
        expect(screen.queryByTestId('sidebar-user-id')).not.toBeInTheDocument();

        // 3 fixed footer entries: Help, Settings, Logout
        const helpBtn = screen.getByTestId('sidebar-help-button');
        expect(helpBtn).toBeInTheDocument();
        expect(screen.getByTestId('sidebar-settings-button')).toBeInTheDocument();
        expect(screen.getByTestId('sidebar-logout-button')).toBeInTheDocument();

        fireEvent.click(helpBtn);
        expect(mockNavigate).toHaveBeenCalledWith({
            view: 'help_center',
            previousView: { view: 'logged_in' },
        });
        expect(mockSetSidebarOpen).toHaveBeenCalledWith(false);

        // Old group keys should NOT exist
        expect(screen.queryByText('common.sidebarActions')).not.toBeInTheDocument();
        expect(screen.queryByText('common.sidebarRecords')).not.toBeInTheDocument();
        expect(screen.queryByText('common.sidebarSecurity')).not.toBeInTheDocument();
    });

    it('renders expert navigation including wallet, user TechId, and collapsible "Mehr" without duplicate help item', async () => {
        localStorage.setItem(UI_MODE_STORAGE_KEY, 'expert');
        render(<Sidebar openClaimsCount={0} />);

        expect(screen.getByText('wallet.title')).toBeInTheDocument();
        expect(screen.queryByText('common.appName')).not.toBeInTheDocument();

        await waitFor(() => {
            expect(screen.getByTestId('sidebar-user-id')).toBeInTheDocument();
        });
        expect(screen.getByTestId('tech-id-value')).toHaveTextContent('did:key:z6MkAlice');
        expect(screen.getByTestId('tech-id-value')).toHaveClass('text-[9px]');

        // By default collapsed
        expect(screen.queryByTestId('sidebar-more-content')).not.toBeInTheDocument();

        // Click toggle
        const toggleBtn = screen.getByTestId('sidebar-more-toggle');
        fireEvent.click(toggleBtn);

        // Now open (Help lives permanently in footer, not duplicated here)
        expect(screen.getByTestId('sidebar-more-content')).toBeInTheDocument();
        expect(screen.getByTestId('sidebar-more-item-conflicts')).toBeInTheDocument();
        expect(screen.queryByTestId('sidebar-more-item-help')).not.toBeInTheDocument();
        expect(screen.getByTestId('sidebar-more-item-support')).toBeInTheDocument();
        expect(screen.getByTestId('sidebar-more-item-bug')).toBeInTheDocument();
        expect(localStorage.getItem('humoco_sidebar_more_open')).toBe('true');

        // Click toggle again to close
        fireEvent.click(toggleBtn);
        expect(screen.queryByTestId('sidebar-more-content')).not.toBeInTheDocument();
        expect(localStorage.getItem('humoco_sidebar_more_open')).toBe('false');
    });

    it('hides top-level Konflikte when openClaimsCount is 0 in both modes', () => {
        localStorage.setItem(UI_MODE_STORAGE_KEY, 'expert');
        render(<Sidebar openClaimsCount={0} />);

        expect(screen.queryByTestId('sidebar-conflict-top-entry')).not.toBeInTheDocument();
        expect(screen.queryByTestId('sidebar-more-indicator-dot')).not.toBeInTheDocument();
    });

    it('renders top-level Konflikte in simple mode even without "Mehr" disclosure when openClaimsCount > 0', () => {
        render(<Sidebar openClaimsCount={3} />);

        const topConflictEntry = screen.getByTestId('sidebar-conflict-top-entry');
        expect(topConflictEntry).toBeInTheDocument();
        expect(topConflictEntry).toHaveTextContent('3');
        expect(screen.queryByTestId('sidebar-more-toggle')).not.toBeInTheDocument();

        fireEvent.click(topConflictEntry);
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'conflict_list' });
        expect(mockSetSidebarOpen).toHaveBeenCalledWith(false);
    });

    it('renders top-level Konflikte with badge and red dot on "Mehr" in expert mode when openClaimsCount > 0', () => {
        localStorage.setItem(UI_MODE_STORAGE_KEY, 'expert');
        render(<Sidebar openClaimsCount={3} />);

        // Top-level Konflikte entry visible
        const topConflictEntry = screen.getByTestId('sidebar-conflict-top-entry');
        expect(topConflictEntry).toBeInTheDocument();
        expect(topConflictEntry).toHaveTextContent('3');

        // Red indicator dot visible on collapsed 'Mehr' toggle
        expect(screen.getByTestId('sidebar-more-indicator-dot')).toBeInTheDocument();

        // Clicking top-level entry navigates to conflict_list
        fireEvent.click(topConflictEntry);
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'conflict_list' });
        expect(mockSetSidebarOpen).toHaveBeenCalledWith(false);
    });

    it('navigates to settings and logout correctly', () => {
        render(<Sidebar openClaimsCount={0} />);

        const settingsBtn = screen.getByTestId('sidebar-settings-button');
        fireEvent.click(settingsBtn);
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'settings' });

        const logoutBtn = screen.getByTestId('sidebar-logout-button');
        vi.mocked(profileService.logout).mockResolvedValue(undefined as never);
        fireEvent.click(logoutBtn);
        expect(mockNotifyLogout).toHaveBeenCalled();
        expect(mockNavigate).toHaveBeenCalledWith({ view: 'needs_login' });
    });
});
