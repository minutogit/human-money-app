import { render, screen, act, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import {
  NavigationProvider,
  useNavigation,
  useOptionalNavigation,
} from '../NavigationContext';
import { AppState } from '../../types';
import { profileService } from '../../services/profileService';
import { authService } from '../../services/authService';

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('0.3.0'),
}));

const mockNotifyLogin = vi.fn();
vi.mock('../SessionContext', () => ({
  useSession: () => ({
    notifyLogin: mockNotifyLogin,
  }),
}));

vi.mock('../../services/profileService', () => ({
  profileService: {
    getProfile: vi.fn(),
  },
}));

vi.mock('../../services/authService', () => ({
  authService: {
    listProfiles: vi.fn(),
  },
}));

function NavigationConsumer() {
  const {
    appState,
    navigate,
    goBack,
    isSidebarOpen,
    setSidebarOpen,
    appVersion,
    checkProfile,
  } = useNavigation();

  return (
    <div>
      <div data-testid="current-view">{appState.view}</div>
      <div data-testid="voucher-id">
        {'voucherId' in appState ? appState.voucherId : ''}
      </div>
      <div data-testid="sidebar-open">{String(isSidebarOpen)}</div>
      <div data-testid="app-version">{appVersion}</div>

      <button onClick={() => navigate({ view: 'wallet' })}>Nav Wallet</button>
      <button
        onClick={() =>
          navigate({ view: 'voucher_details', voucherId: 'voucher-123' })
        }
      >
        Nav Voucher Details
      </button>
      <button onClick={() => navigate({ view: 'settings' })}>Nav Settings</button>
      <button onClick={() => navigate({ view: 'activities' })}>Nav Activities</button>
      <button onClick={() => navigate({ view: 'logged_in' })}>Nav Logged In</button>
      <button onClick={() => navigate({ view: 'needs_login' })}>Nav Needs Login</button>
      <button
        onClick={() =>
          navigate({
            view: 'voucher_details',
            voucherId: 'voucher-456',
            previousView: { view: 'wallet' },
          })
        }
      >
        Nav Voucher Details With Prev
      </button>
      <button onClick={() => goBack()}>Go Back</button>
      <button onClick={() => setSidebarOpen(true)}>Open Sidebar</button>
      <button onClick={() => setSidebarOpen(false)}>Close Sidebar</button>
      <button onClick={() => void checkProfile()}>Run Check Profile</button>
    </div>
  );
}

describe('NavigationContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (profileService.getProfile as Mock).mockResolvedValue({
      id: 'did:key:zUser123',
      firstName: 'Alice',
    });
    (authService.listProfiles as Mock).mockResolvedValue([]);
  });

  it('initializes appVersion and automatically sets logged_in when profile session is active', async () => {
    render(
      <NavigationProvider>
        <NavigationConsumer />
      </NavigationProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');
      expect(screen.getByTestId('app-version')).toHaveTextContent('0.3.0');
    });

    expect(mockNotifyLogin).toHaveBeenCalledWith('Alice');
  });

  it('sets needs_login on mount when no active session profile exists', async () => {
    (profileService.getProfile as Mock).mockRejectedValue(new Error('No session'));

    render(
      <NavigationProvider>
        <NavigationConsumer />
      </NavigationProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('current-view')).toHaveTextContent('needs_login');
    });

    expect(authService.listProfiles).toHaveBeenCalledTimes(1);
  });

  it('manages multi-step navigation history stack and falls back to logged_in when empty', async () => {
    render(
      <NavigationProvider>
        <NavigationConsumer />
      </NavigationProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');
    });

    // Step 1: Navigate to 'wallet'
    await act(async () => {
      screen.getByText('Nav Wallet').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('wallet');

    // Step 2: Navigate to 'voucher_details'
    await act(async () => {
      screen.getByText('Nav Voucher Details').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('voucher_details');
    expect(screen.getByTestId('voucher-id')).toHaveTextContent('voucher-123');

    // Step 3: Navigate to 'settings'
    await act(async () => {
      screen.getByText('Nav Settings').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('settings');

    // Step 4: Go Back -> should return to 'voucher_details'
    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('voucher_details');
    expect(screen.getByTestId('voucher-id')).toHaveTextContent('voucher-123');

    // Step 5: Go Back -> should return to 'wallet'
    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('wallet');

    // Step 6: Go Back -> should return to 'logged_in'
    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');

    // Step 7: Go Back on empty stack -> should stay/fallback to 'logged_in'
    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');
  });

  it('resets the history stack cleanly when navigating to logged_in, needs_login, or loading', async () => {
    render(
      <NavigationProvider>
        <NavigationConsumer />
      </NavigationProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');
    });

    // Build some history
    await act(async () => {
      screen.getByText('Nav Wallet').click();
    });
    await act(async () => {
      screen.getByText('Nav Settings').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('settings');

    // Navigate to logged_in (resets history stack)
    await act(async () => {
      screen.getByText('Nav Logged In').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');

    // Going back should not return to settings or wallet; stack is empty, falls back to logged_in
    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');

    // Test needs_login reset as well
    await act(async () => {
      screen.getByText('Nav Wallet').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('wallet');

    await act(async () => {
      screen.getByText('Nav Needs Login').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('needs_login');

    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');
  });

  it('supports explicit previousView compatibility without leaving duplicate entries on the stack', async () => {
    render(
      <NavigationProvider>
        <NavigationConsumer />
      </NavigationProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');
    });

    // Navigate to 'wallet'
    await act(async () => {
      screen.getByText('Nav Wallet').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('wallet');

    // Navigate to voucher_details with previousView = { view: 'wallet' }
    await act(async () => {
      screen.getByText('Nav Voucher Details With Prev').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('voucher_details');
    expect(screen.getByTestId('voucher-id')).toHaveTextContent('voucher-456');

    // Go back once -> returns to 'wallet' and cleans duplicate wallet entry from stack
    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('wallet');

    // Next go back -> returns to 'logged_in' directly (not a second wallet entry!)
    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');
  });

  it('does not duplicate history entries when repeatedly navigating to the same view', async () => {
    render(
      <NavigationProvider>
        <NavigationConsumer />
      </NavigationProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');
    });

    // Navigate to wallet
    await act(async () => {
      screen.getByText('Nav Wallet').click();
    });
    // Navigate to wallet again
    await act(async () => {
      screen.getByText('Nav Wallet').click();
    });

    // Navigate to settings
    await act(async () => {
      screen.getByText('Nav Settings').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('settings');

    // First back goes to wallet
    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('wallet');

    // Second back goes directly to logged_in, not duplicate wallet
    await act(async () => {
      screen.getByText('Go Back').click();
    });
    expect(screen.getByTestId('current-view')).toHaveTextContent('logged_in');
  });

  it('caps history stack to maximum 20 entries preventing unbounded growth', async () => {
    let navContext!: ReturnType<typeof useNavigation>;
    function Inspector() {
      navContext = useNavigation();
      return <div data-testid="view">{navContext.appState.view}</div>;
    }

    render(
      <NavigationProvider>
        <Inspector />
      </NavigationProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('view')).toHaveTextContent('logged_in');
    });

    // Push 25 alternating views
    await act(async () => {
      for (let i = 0; i < 25; i++) {
        const viewName: AppState['view'] = i % 2 === 0 ? 'wallet' : 'settings';
        navContext.navigate({ view: viewName } as AppState);
      }
    });

    // We can go back up to 20 times without error, popping bounded entries
    for (let i = 0; i < 22; i++) {
      await act(async () => {
        navContext.goBack();
      });
    }

    // After exhausting history, it safely stays at logged_in
    expect(navContext.appState.view).toBe('logged_in');
  });

  it('controls sidebar open and closed state', async () => {
    render(
      <NavigationProvider>
        <NavigationConsumer />
      </NavigationProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('sidebar-open')).toHaveTextContent('false');
    });

    await act(async () => {
      screen.getByText('Open Sidebar').click();
    });
    expect(screen.getByTestId('sidebar-open')).toHaveTextContent('true');

    await act(async () => {
      screen.getByText('Close Sidebar').click();
    });
    expect(screen.getByTestId('sidebar-open')).toHaveTextContent('false');
  });

  it('throws an error when useNavigation is used outside of NavigationProvider', () => {
    function InvalidConsumer() {
      useNavigation();
      return null;
    }

    // Suppress console.error for expected throw
    const originalConsoleError = console.error;
    console.error = vi.fn();

    expect(() => render(<InvalidConsumer />)).toThrow(
      'useNavigation must be used within a NavigationProvider'
    );

    console.error = originalConsoleError;
  });

  it('returns null when useOptionalNavigation is used outside of NavigationProvider', () => {
    let capturedContext: unknown = 'init';
    function OptionalConsumer() {
      capturedContext = useOptionalNavigation();
      return null;
    }

    render(<OptionalConsumer />);
    expect(capturedContext).toBeNull();
  });
});
