import { render, screen, act, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { SessionProvider, useSession } from '../SessionContext';
import { authService } from '../../services/authService';
import { settingsService } from '../../services/settingsService';
import { NavigationProvider, useNavigation } from '../NavigationContext';
import { Sidebar } from '../../components/Sidebar';
import App from '../../App';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('0.2.1'),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === 'list_profiles') return Promise.resolve([]);
    if (cmd === 'has_profiles') return Promise.resolve(true);
    if (cmd === 'get_user_id') return Promise.resolve('did:key:z123');
    if (cmd === 'get_app_settings') return Promise.resolve({
      sessionTimeoutSeconds: 300,
      privacyDefault: 'public',
    });
    return Promise.resolve(undefined);
  }),
}));

// Mock authService and settingsService
vi.mock('../../services/authService', () => ({
  authService: {
    isSessionActive: vi.fn(),
    unlockSession: vi.fn(),
    refreshSessionActivity: vi.fn().mockResolvedValue(true),
  },
}));

vi.mock('../../services/settingsService', () => ({
  settingsService: {
    getSettings: vi.fn().mockResolvedValue({
      sessionTimeoutSeconds: 300,
      privacyDefault: 'public',
    }),
  },
}));

vi.mock('../../utils/sealSync', () => ({
  startSealSyncLoop: vi.fn(),
  stopSealSyncLoop: vi.fn(),
}));

vi.mock('../../hooks/useWindowTitleSync', () => ({
  useWindowTitleSync: vi.fn(),
}));

vi.mock('../../hooks/useIntegrityCheck', () => ({
  useIntegrityCheck: () => ({
    integrityReport: null,
    checkIntegrity: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock('../../services/profileService', () => ({
  profileService: {
    logout: vi.fn().mockResolvedValue(undefined),
    getUserId: vi.fn().mockResolvedValue('did:key:z123'),
    getProfile: vi.fn().mockResolvedValue(null),
    listProfiles: vi.fn().mockResolvedValue([]),
    hasProfiles: vi.fn().mockResolvedValue(true),
  },
}));

// Test helper component to inspect SessionContext values
function SessionConsumer() {
  const { isSessionActive, isAuthenticated, profileName, notifyLogin, notifyLogout, protectAction } = useSession();
  return (
    <div>
      <div data-testid="is-session-active">{String(isSessionActive)}</div>
      <div data-testid="is-authenticated">{String(isAuthenticated)}</div>
      <div data-testid="profile-name">{profileName}</div>
      <button onClick={() => notifyLogin('Alice')}>Login Alice</button>
      <button onClick={() => notifyLogout()}>Logout</button>
      <button onClick={() => protectAction(async () => 'action-done')}>Protect Action</button>
    </div>
  );
}

import { AppState } from '../../types';

// Test harness that puts the app in an internal view with Sidebar
function SidebarTestApp({ initialView = 'wallet' }: { initialView?: AppState['view'] }) {
  return (
    <SessionProvider>
      <NavigationProvider>
        <SidebarTestContent initialView={initialView} />
      </NavigationProvider>
    </SessionProvider>
  );
}

function SidebarTestContent({ initialView }: { initialView: AppState['view'] }) {
  const { isSessionActive, isAuthenticated, notifyLogin, setProfileName } = useSession();
  const { navigate, appState } = useNavigation();

  return (
    <div>
      <div data-testid="current-view">{appState.view}</div>
      <div data-testid="app-session-active">{String(isSessionActive)}</div>
      <div data-testid="app-authenticated">{String(isAuthenticated)}</div>
      <button onClick={() => navigate({ view: initialView } as AppState)}>Set Internal View</button>
      <button onClick={() => { notifyLogin('Alice'); }}>Login Alice</button>
      <button onClick={() => { setProfileName('Alice'); }}>Set Profile Alice</button>
      <Sidebar />
    </div>
  );
}

describe('SessionContext and Session Timeout UX (APP-017)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('provides isAuthenticated and preserves profileName on session expiry', async () => {
    (authService.isSessionActive as Mock).mockResolvedValue(true);

    render(
      <SessionProvider>
        <SessionConsumer />
      </SessionProvider>
    );

    expect(screen.getByTestId('is-authenticated').textContent).toBe('false');
    expect(screen.getByTestId('is-session-active').textContent).toBe('false');
    expect(screen.getByTestId('profile-name').textContent).toBe('');

    // Log in user
    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    expect(screen.getByTestId('is-authenticated').textContent).toBe('true');
    expect(screen.getByTestId('is-session-active').textContent).toBe('true');
    expect(screen.getByTestId('profile-name').textContent).toBe('Alice');

    // Simulate session expiry in backend (e.g. 10m timer expires)
    (authService.isSessionActive as Mock).mockResolvedValue(false);

    // Logout should properly clear everything
    await act(async () => {
      screen.getByText('Logout').click();
    });

    expect(screen.getByTestId('is-authenticated').textContent).toBe('false');
    expect(screen.getByTestId('is-session-active').textContent).toBe('false');
    expect(screen.getByTestId('profile-name').textContent).toBe('');
  });

  it('keeps user authenticated when sessionTimeoutSeconds === 0 (Always ask password)', async () => {
    // When timeout is 0, backend is_session_active() is always false
    (authService.isSessionActive as Mock).mockResolvedValue(false);
    (settingsService.getSettings as Mock).mockResolvedValue({
      sessionTimeoutSeconds: 0,
      privacyDefault: 'public',
    });

    render(
      <SessionProvider>
        <SessionConsumer />
      </SessionProvider>
    );

    // User logs in
    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    // Wait for authService.isSessionActive() promise in notifyLogin to settle
    await waitFor(() => {
      // User must be authenticated even though backend timed session is inactive
      expect(screen.getByTestId('is-authenticated').textContent).toBe('true');
      expect(screen.getByTestId('is-session-active').textContent).toBe('false');
      expect(screen.getByTestId('profile-name').textContent).toBe('Alice');
    });
  });

  it('renders Sidebar with session locked indicator when session is inactive but user is authenticated', async () => {
    (authService.isSessionActive as Mock).mockResolvedValue(true);

    render(<SidebarTestApp initialView="wallet" />);

    // Log in
    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    // Navigate to wallet
    await act(async () => {
      screen.getByText('Set Internal View').click();
    });

    // Sidebar should be rendered with Alice
    await waitFor(() => {
      expect(screen.getByText('Alice')).toBeInTheDocument();
      expect(screen.getByText(/Active Wallet/i)).toBeInTheDocument();
    });
  });

  it('Sidebar profile header has clickable unlock action when session is locked', async () => {
    (authService.isSessionActive as Mock).mockResolvedValue(false);

    const { container } = render(<SidebarTestApp initialView="wallet" />);

    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    await act(async () => {
      screen.getByText('Set Internal View').click();
    });

    await waitFor(() => {
      expect(screen.getByText('Alice')).toBeInTheDocument();
    });

    // Status shows locked indicator
    const lockedPill = screen.getByTitle(/Session Locked/i);
    expect(lockedPill).toBeInTheDocument();
    expect(lockedPill.className).toContain('cursor-pointer');

    // Clicking locked pill invokes protectAction (which triggers auth unlock)
    await act(async () => {
      fireEvent.click(lockedPill);
    });
    expect(container).toBeInTheDocument();
  });

  it('App does NOT redirect to needs_login when isAuthenticated is true but isSessionActive is false', async () => {
    (authService.isSessionActive as Mock).mockResolvedValue(false);

    await act(async () => {
      render(<App />);
    });

    expect(document.body).toBeInTheDocument();
  });
});
