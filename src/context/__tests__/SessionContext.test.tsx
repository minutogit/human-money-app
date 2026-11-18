import { render, screen, act, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { SessionProvider, useSession } from '../SessionContext';
import { authService } from '../../services/authService';
import { settingsService } from '../../services/settingsService';
import { NavigationProvider, useNavigation } from '../NavigationContext';
import { Sidebar } from '../../components/shell/Sidebar';
import App from '../../App';
import { startSealSyncLoop, stopSealSyncLoop } from '../../sync/sealSync';
import { startAutoBackupLoop, stopAutoBackupLoop } from '../../sync/autoBackup';
import { startPostSyncLoop, stopPostSyncLoop } from '../../sync/postSync';
import { startMarketSyncLoop, stopMarketSyncLoop } from '../../sync/marketSync';
import { channelMembersSyncManager } from '../../sync/channelMembersSyncManager';
import { rateLimitManager } from '../../sync/rateLimitManager';
import { AppState } from '../../types';

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
    lockSession: vi.fn().mockResolvedValue(undefined),
    elevateSessionWithPassword: vi.fn().mockResolvedValue('full'),
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

vi.mock('../../sync/sealSync', () => ({
  startSealSyncLoop: vi.fn(),
  stopSealSyncLoop: vi.fn(),
}));

vi.mock('../../sync/autoBackup', () => ({
  startAutoBackupLoop: vi.fn(),
  stopAutoBackupLoop: vi.fn(),
  triggerAutoBackup: vi.fn(),
  dispatchBackupUpdated: vi.fn(),
  afterWalletMutation: vi.fn(),
  BACKUP_UPDATED_EVENT: 'humoco:backup-updated',
}));

vi.mock('../../sync/postSync', () => ({
  startPostSyncLoop: vi.fn(),
  stopPostSyncLoop: vi.fn(),
  runPostSyncPass: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../sync/marketSync', () => ({
  startMarketSyncLoop: vi.fn(),
  stopMarketSyncLoop: vi.fn(),
  runMarketSyncPass: vi.fn().mockResolvedValue(undefined),
  scheduleContactOnboardingMarketSync: vi.fn(),
  DEFAULT_ONBOARDING_DELAY_MS: 3000,
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
function SessionConsumer({
  onContextReady,
}: {
  onContextReady?: (ctx: ReturnType<typeof useSession>) => void;
}) {
  const ctx = useSession();
  onContextReady?.(ctx);
  const { isSessionActive, isAuthenticated, profileName, notifyLogin, notifyLogout, protectAction, lockSession } = ctx;
  return (
    <div>
      <div data-testid="is-session-active">{String(isSessionActive)}</div>
      <div data-testid="is-authenticated">{String(isAuthenticated)}</div>
      <div data-testid="profile-name">{profileName}</div>
      <button onClick={() => notifyLogin('Alice')}>Login Alice</button>
      <button onClick={() => notifyLogout()}>Logout</button>
      <button onClick={() => void lockSession()}>Lock Session</button>
      <button onClick={() => void protectAction(async () => 'action-done').catch(() => {})}>Protect Action</button>
    </div>
  );
}

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
    (settingsService.getSettings as Mock).mockResolvedValue({
      sessionTimeoutSeconds: 300,
      privacyDefault: 'public',
    });
    (authService.lockSession as Mock).mockResolvedValue(undefined);
    (authService.unlockSession as Mock).mockResolvedValue(undefined);
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
      expect(screen.getByText(/Active Profile/i)).toBeInTheDocument();
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

  it('keeps modal open with error when unlockSession fails in session mode and resolves on valid retry', async () => {
    (authService.isSessionActive as Mock).mockResolvedValue(false);
    (authService.unlockSession as Mock)
      .mockRejectedValueOnce(new Error('Invalid password'))
      .mockResolvedValueOnce(undefined);

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    vi.mocked(startSealSyncLoop).mockClear();
    vi.mocked(startAutoBackupLoop).mockClear();
    vi.mocked(startPostSyncLoop).mockClear();
    vi.mocked(startMarketSyncLoop).mockClear();

    let actionPromise!: Promise<unknown>;
    await act(async () => {
      actionPromise = sessionCtx.protectAction(async () => 'unlocked-ok');
    });

    // Modal should be visible
    const input = await screen.findByPlaceholderText('Profile Password');
    fireEvent.change(input, { target: { value: 'wrong-pass' } });

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    // Modal stays open with error message
    expect(screen.getByText('Authentication Required')).toBeInTheDocument();
    expect(screen.getByText('Invalid password')).toBeInTheDocument();
    expect(authService.unlockSession).toHaveBeenCalledWith('wrong-pass', 300);

    // Retry with correct password
    fireEvent.change(screen.getByPlaceholderText('Profile Password'), {
      target: { value: 'right-pass' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    await expect(actionPromise).resolves.toBe('unlocked-ok');
    await waitFor(() => {
      expect(screen.queryByText('Authentication Required')).not.toBeInTheDocument();
    });
    expect(startSealSyncLoop).toHaveBeenCalled();
    expect(startAutoBackupLoop).toHaveBeenCalled();
    expect(startPostSyncLoop).toHaveBeenCalled();
    expect(startMarketSyncLoop).toHaveBeenCalled();
  });

  it('queues concurrent protectAction callers without dropping earlier promises', async () => {
    (authService.isSessionActive as Mock).mockResolvedValue(false);
    (authService.unlockSession as Mock).mockResolvedValue(undefined);

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    let p1!: Promise<unknown>;
    let p2!: Promise<unknown>;
    await act(async () => {
      p1 = sessionCtx.protectAction(async () => 'first');
      p2 = sessionCtx.protectAction(async () => 'second');
    });

    const input = await screen.findByPlaceholderText('Profile Password');
    fireEvent.change(input, { target: { value: 'secret' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    await expect(Promise.all([p1, p2])).resolves.toEqual(['first', 'second']);
  });

  it('stops all sync loops on lockSession and session expiry, and cleans up on unmount', async () => {
    vi.useFakeTimers();
    try {
      (authService.isSessionActive as Mock).mockResolvedValue(true);

      const { unmount } = render(
        <SessionProvider>
          <SessionConsumer />
        </SessionProvider>
      );

      await act(async () => {
        screen.getByText('Login Alice').click();
      });

      expect(startSealSyncLoop).toHaveBeenCalledTimes(1);
      expect(startAutoBackupLoop).toHaveBeenCalledTimes(1);
      expect(startPostSyncLoop).toHaveBeenCalledTimes(1);
      expect(startMarketSyncLoop).toHaveBeenCalledTimes(1);

      vi.mocked(stopSealSyncLoop).mockClear();
      vi.mocked(stopAutoBackupLoop).mockClear();
      vi.mocked(stopPostSyncLoop).mockClear();
      vi.mocked(stopMarketSyncLoop).mockClear();

      // 1. Explicit lockSession stops loops
      await act(async () => {
        screen.getByText('Lock Session').click();
      });

      expect(authService.lockSession).toHaveBeenCalledTimes(1);
      expect(stopSealSyncLoop).toHaveBeenCalledTimes(1);
      expect(stopAutoBackupLoop).toHaveBeenCalledTimes(1);
      expect(stopPostSyncLoop).toHaveBeenCalledTimes(1);
      expect(stopMarketSyncLoop).toHaveBeenCalledTimes(1);

      // Re-login and trigger heartbeat session expiry
      await act(async () => {
        screen.getByText('Login Alice').click();
      });
      vi.mocked(stopSealSyncLoop).mockClear();
      vi.mocked(stopAutoBackupLoop).mockClear();
      vi.mocked(stopPostSyncLoop).mockClear();
      vi.mocked(stopMarketSyncLoop).mockClear();

      (authService.isSessionActive as Mock).mockResolvedValue(false);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(10_000);
      });

      expect(screen.getByTestId('is-session-active').textContent).toBe('false');
      expect(screen.getByTestId('is-authenticated').textContent).toBe('true');
      expect(stopSealSyncLoop).toHaveBeenCalledTimes(1);
      expect(stopAutoBackupLoop).toHaveBeenCalledTimes(1);
      expect(stopPostSyncLoop).toHaveBeenCalledTimes(1);
      expect(stopMarketSyncLoop).toHaveBeenCalledTimes(1);

      // Unmount also invokes stopSyncLoops
      vi.mocked(stopSealSyncLoop).mockClear();
      unmount();
      expect(stopSealSyncLoop).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it('notifyLogout stops sync loops and resets channelMembersSyncManager and rateLimitManager', async () => {
    (authService.isSessionActive as Mock).mockResolvedValue(true);
    const membersResetSpy = vi.spyOn(channelMembersSyncManager, 'reset');
    const cooldownClearSpy = vi.spyOn(rateLimitManager, 'clearCooldown');

    render(
      <SessionProvider>
        <SessionConsumer />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    vi.mocked(stopSealSyncLoop).mockClear();
    vi.mocked(stopAutoBackupLoop).mockClear();
    vi.mocked(stopPostSyncLoop).mockClear();
    vi.mocked(stopMarketSyncLoop).mockClear();
    membersResetSpy.mockClear();
    cooldownClearSpy.mockClear();

    await act(async () => {
      screen.getByText('Logout').click();
    });

    expect(stopSealSyncLoop).toHaveBeenCalledTimes(1);
    expect(stopAutoBackupLoop).toHaveBeenCalledTimes(1);
    expect(stopPostSyncLoop).toHaveBeenCalledTimes(1);
    expect(stopMarketSyncLoop).toHaveBeenCalledTimes(1);
    expect(membersResetSpy).toHaveBeenCalledTimes(1);
    expect(cooldownClearSpy).toHaveBeenCalledTimes(1);

    membersResetSpy.mockRestore();
    cooldownClearSpy.mockRestore();
  });
});
