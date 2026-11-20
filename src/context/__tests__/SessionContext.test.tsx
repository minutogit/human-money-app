import { render, screen, act, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { SessionProvider, useSession } from '../SessionContext';
import { authService } from '../../services/authService';
import { settingsService } from '../../services/settingsService';
import { NavigationProvider, useNavigation } from '../NavigationContext';
import { Sidebar } from '../../components/shell/Sidebar';
import App from '../../App';
import { updateNetworkStatus } from '../../sync/syncBridge';
import { rateLimitManager } from '../../sync/rateLimitManager';
import { AppConfigDto, AppState } from '../../types';

function makeAppConfig(
  sessionTimeoutSeconds = 300,
  privacyDefault: 'ask' | 'stealth' | 'public' = 'public',
): AppConfigDto {
  return {
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
      sessionTimeoutSeconds,
      privacyDefault,
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
}

// Mock Tauri APIs
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('0.2.1'),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === 'list_profiles') return Promise.resolve([]);
    if (cmd === 'has_profiles') return Promise.resolve(true);
    if (cmd === 'get_user_id') return Promise.resolve('did:key:z123');
    if (cmd === 'get_app_config') return Promise.resolve(makeAppConfig(300, 'public'));
    return Promise.resolve(undefined);
  }),
}));

// Mock authService and settingsService
vi.mock('../../services/authService', () => ({
  authService: {
    getSessionStatus: vi.fn(),
    unlockSession: vi.fn(),
    unlockSessionWithPin: vi.fn(),
    lockSession: vi.fn().mockResolvedValue(undefined),
    elevateSessionWithPassword: vi.fn(),
    refreshSessionActivity: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../services/settingsService', () => ({
  settingsService: {
    getConfig: vi.fn(),
  },
}));

vi.mock('../../sync/syncBridge', () => ({
  updateNetworkStatus: vi.fn().mockResolvedValue(undefined),
  wakeSyncCoordinator: vi.fn().mockResolvedValue(undefined),
  triggerAutoBackup: vi.fn(),
  dispatchBackupUpdated: vi.fn(),
  afterWalletMutation: vi.fn(),
  runSyncBackupPass: vi.fn().mockResolvedValue(null),
  runPostSyncPass: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../hooks/useWindowTitleSync', () => ({
  useWindowTitleSync: vi.fn(),
}));

const mockCheckIntegrity = vi.fn().mockResolvedValue(undefined);
const mockResetIntegrity = vi.fn();

vi.mock('../../hooks/useIntegrityCheck', () => ({
  useIntegrityCheck: () => ({
    integrityReport: null,
    checkIntegrity: mockCheckIntegrity,
    resetIntegrity: mockResetIntegrity,
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
  const {
    hasCachedSession,
    isAuthenticated,
    accessMode,
    baseAccessMode,
    profileName,
    notifyLogin,
    notifyLogout,
    protectAction,
    lockSession,
  } = ctx;
  return (
    <div>
      <div data-testid="has-cached-session">{String(hasCachedSession)}</div>
      <div data-testid="is-authenticated">{String(isAuthenticated)}</div>
      <div data-testid="access-mode">{accessMode}</div>
      <div data-testid="base-access-mode">{baseAccessMode}</div>
      <div data-testid="profile-name">{profileName}</div>
      <button onClick={() => notifyLogin('Alice', 'alice_folder')}>Login Alice</button>
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
  const { hasCachedSession, isAuthenticated, notifyLogin, setProfileName } = useSession();
  const { navigate, appState } = useNavigation();

  return (
    <div>
      <div data-testid="current-view">{appState.view}</div>
      <div data-testid="app-session-active">{String(hasCachedSession)}</div>
      <div data-testid="app-authenticated">{String(isAuthenticated)}</div>
      <button onClick={() => navigate({ view: initialView } as AppState)}>Set Internal View</button>
      <button onClick={() => { notifyLogin('Alice', 'alice_folder'); }}>Login Alice</button>
      <button onClick={() => { setProfileName('Alice'); }}>Set Profile Alice</button>
      <Sidebar />
    </div>
  );
}

function fullStatus(hasCachedSession: boolean) {
  return {
    isUnlocked: true,
    hasCachedSession,
    accessMode: 'full' as const,
    baseAccessMode: 'full' as const,
  };
}

function dailyPayStatus(hasCachedSession: boolean, elevated = false) {
  return {
    isUnlocked: true,
    hasCachedSession,
    accessMode: (elevated ? 'full' : 'dailyPay') as 'full' | 'dailyPay',
    baseAccessMode: 'dailyPay' as const,
  };
}

describe('SessionContext and Session Timeout UX (APP-017)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (settingsService.getConfig as Mock).mockReset().mockResolvedValue(makeAppConfig(300, 'public'));
    (authService.lockSession as Mock).mockReset().mockResolvedValue(undefined);
    (authService.unlockSession as Mock).mockReset().mockResolvedValue(fullStatus(true));
    (authService.unlockSessionWithPin as Mock).mockReset().mockResolvedValue(dailyPayStatus(true));
    (authService.elevateSessionWithPassword as Mock).mockReset().mockResolvedValue(fullStatus(true));
    (authService.getSessionStatus as Mock).mockReset().mockResolvedValue(fullStatus(true));
  });

  it('provides isAuthenticated and preserves profileName on session expiry', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(fullStatus(true));

    render(
      <SessionProvider>
        <SessionConsumer />
      </SessionProvider>
    );

    expect(screen.getByTestId('is-authenticated').textContent).toBe('false');
    expect(screen.getByTestId('has-cached-session').textContent).toBe('false');
    expect(screen.getByTestId('profile-name').textContent).toBe('');

    // Log in user
    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('is-authenticated').textContent).toBe('true');
      expect(screen.getByTestId('has-cached-session').textContent).toBe('true');
    });
    expect(screen.getByTestId('profile-name').textContent).toBe('Alice');
    expect(mockCheckIntegrity).toHaveBeenCalledWith({ allowSilentRepair: true });

    // Logout should properly clear everything
    await act(async () => {
      screen.getByText('Logout').click();
    });

    expect(screen.getByTestId('is-authenticated').textContent).toBe('false');
    expect(screen.getByTestId('has-cached-session').textContent).toBe('false');
    expect(screen.getByTestId('profile-name').textContent).toBe('');
  });

  it('keeps user authenticated when sessionTimeoutSeconds === 0 and disables silent repair', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(fullStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(0, 'public'));

    render(
      <SessionProvider>
        <SessionConsumer />
      </SessionProvider>
    );

    // User logs in
    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('is-authenticated').textContent).toBe('true');
      expect(screen.getByTestId('has-cached-session').textContent).toBe('false');
      expect(screen.getByTestId('profile-name').textContent).toBe('Alice');
    });
    expect(mockCheckIntegrity).toHaveBeenCalledWith({ allowSilentRepair: false });
  });

  it('renders Sidebar with session locked indicator when session is inactive but user is authenticated', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(fullStatus(true));

    render(<SidebarTestApp initialView="wallet" />);

    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    await act(async () => {
      screen.getByText('Set Internal View').click();
    });

    await waitFor(() => {
      expect(screen.getByText('Alice')).toBeInTheDocument();
      expect(screen.getByText(/Active Profile/i)).toBeInTheDocument();
    });
  });

  it('Sidebar profile header works cleanly when cached session is locked', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(fullStatus(false));

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

    const lockedPill = screen.getByTitle(/Session Locked/i);
    expect(lockedPill).toBeInTheDocument();
    expect(lockedPill.className).toContain('cursor-pointer');

    await act(async () => {
      fireEvent.click(lockedPill);
    });
    expect(container).toBeInTheDocument();
  });

  it('App does NOT redirect to needs_login when isAuthenticated is true but hasCachedSession is false', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(fullStatus(false));

    await act(async () => {
      render(<App />);
    });

    expect(document.body).toBeInTheDocument();
  });

  it('keeps modal open with error when unlockSession fails in session mode and resolves on valid retry', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(fullStatus(false));
    (authService.unlockSession as Mock)
      .mockRejectedValueOnce(new Error('Invalid password'))
      .mockResolvedValueOnce(fullStatus(true));

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('is-authenticated').textContent).toBe('true');
    });

    vi.mocked(updateNetworkStatus).mockClear();

    let actionPromise!: Promise<unknown>;
    await act(async () => {
      actionPromise = sessionCtx.protectAction(async () => 'unlocked-ok');
    });

    const input = await screen.findByPlaceholderText('Profile Password');
    fireEvent.change(input, { target: { value: 'wrong-pass' } });

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    expect(screen.getByText('Authentication Required')).toBeInTheDocument();
    expect(screen.getByText('Invalid password')).toBeInTheDocument();
    expect(authService.unlockSession).toHaveBeenCalledWith('wrong-pass');

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
    expect(updateNetworkStatus).toHaveBeenCalled();
  });

  it('queues concurrent protectAction callers without dropping earlier promises', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(fullStatus(false));
    (authService.unlockSession as Mock).mockResolvedValue(fullStatus(true));

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

  it('demotes elevated accessMode back to dailyPay on lockSession and heartbeat expiry', async () => {
    vi.useFakeTimers();
    try {
      (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(true, true));
      const lockedSpy = vi.fn();
      window.addEventListener('humoco:session-locked', lockedSpy);

      const { unmount } = render(
        <SessionProvider>
          <SessionConsumer />
        </SessionProvider>
      );

      await act(async () => {
        screen.getByText('Login Alice').click();
      });

      expect(screen.getByTestId('has-cached-session').textContent).toBe('true');
      expect(screen.getByTestId('access-mode').textContent).toBe('full');
      expect(screen.getByTestId('base-access-mode').textContent).toBe('dailyPay');

      lockedSpy.mockClear();

      // 1. Explicit lockSession demotes accessMode back to dailyPay and emits SESSION_LOCKED
      await act(async () => {
        screen.getByText('Lock Session').click();
      });

      expect(authService.lockSession).toHaveBeenCalledTimes(1);
      expect(screen.getByTestId('access-mode').textContent).toBe('dailyPay');
      expect(screen.getByTestId('has-cached-session').textContent).toBe('false');
      expect(lockedSpy).toHaveBeenCalledTimes(1);

      // Re-login in elevated dailyPay state and trigger heartbeat expiry
      await act(async () => {
        screen.getByText('Login Alice').click();
      });
      expect(screen.getByTestId('access-mode').textContent).toBe('full');

      lockedSpy.mockClear();

      (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(false, false));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(10_000);
      });

      expect(screen.getByTestId('has-cached-session').textContent).toBe('false');
      expect(screen.getByTestId('access-mode').textContent).toBe('dailyPay');
      expect(screen.getByTestId('is-authenticated').textContent).toBe('true');
      expect(lockedSpy).toHaveBeenCalledTimes(1);

      window.removeEventListener('humoco:session-locked', lockedSpy);
      unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it('notifyLogout resets integrity, rateLimitManager and emits session lock', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(fullStatus(true));
    const cooldownClearSpy = vi.spyOn(rateLimitManager, 'clearCooldown');
    const lockedSpy = vi.fn();
    window.addEventListener('humoco:session-locked', lockedSpy);

    render(
      <SessionProvider>
        <SessionConsumer />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    mockResetIntegrity.mockClear();
    cooldownClearSpy.mockClear();
    lockedSpy.mockClear();

    await act(async () => {
      screen.getByText('Logout').click();
    });

    expect(mockResetIntegrity).toHaveBeenCalledTimes(1);
    expect(cooldownClearSpy).toHaveBeenCalledTimes(1);
    expect(lockedSpy).toHaveBeenCalledTimes(1);

    window.removeEventListener('humoco:session-locked', lockedSpy);
    cooldownClearSpy.mockRestore();
  });

  it('re-unlocks expired dailyPay session via PIN on timeout > 0 and rejects on wrong PIN', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(300));
    (authService.unlockSessionWithPin as Mock)
      .mockRejectedValueOnce({ code: 'error.auth.invalidPin', message: 'Invalid PIN (4 attempts remaining).' })
      .mockResolvedValueOnce(dailyPayStatus(true));

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('base-access-mode').textContent).toBe('dailyPay');
    });

    let actionPromise!: Promise<unknown>;
    await act(async () => {
      actionPromise = sessionCtx.protectAction(async (pwd) => {
        expect(pwd).toBeNull();
        return 'pin-reunlocked-ok';
      });
    });

    const pinInput = await screen.findByPlaceholderText('6–8 digit Quick PIN');
    fireEvent.change(pinInput, { target: { value: '000000' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });
    expect(screen.getByText(/Incorrect PIN/i)).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText('6–8 digit Quick PIN'), {
      target: { value: '246810' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    await expect(actionPromise).resolves.toBe('pin-reunlocked-ok');
    expect(authService.unlockSessionWithPin).toHaveBeenLastCalledWith('246810', 'alice_folder');
    expect(authService.unlockSession).not.toHaveBeenCalled();
    expect(screen.getByTestId('has-cached-session').textContent).toBe('true');
  });

  it('switches PIN modal to master password automatically on 5-strike wipe and completes via elevateSession', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(300));
    (authService.unlockSessionWithPin as Mock).mockRejectedValueOnce({
      code: 'error.auth.pinWipedMaxAttempts',
      message: 'PIN wiped after 5 failed attempts',
    });
    (authService.elevateSessionWithPassword as Mock).mockResolvedValue(dailyPayStatus(true, true));

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('base-access-mode').textContent).toBe('dailyPay');
    });

    let actionPromise!: Promise<unknown>;
    await act(async () => {
      actionPromise = sessionCtx.protectAction(async () => 'after-wipe-master-ok');
    });

    const pinInput = await screen.findByPlaceholderText('6–8 digit Quick PIN');
    fireEvent.change(pinInput, { target: { value: '000000' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    // Switched in-place to password input with wipe message preserved
    const pwInput = await screen.findByPlaceholderText('Profile Password');
    expect(screen.getByText(/PIN removed after 5 failed attempts/i)).toBeInTheDocument();

    fireEvent.change(pwInput, { target: { value: 'master-pw' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    await expect(actionPromise).resolves.toBe('after-wipe-master-ok');
    expect(authService.elevateSessionWithPassword).toHaveBeenCalledWith('master-pw');
  });

  it('handles timeout=0 dailyPay action via single-use backend grant and passes grant instead of raw PIN', async () => {
    const mockGrant = 'hmc_grant_0123456789abcdef0123456789abcdef';
    (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(0));
    (authService.unlockSessionWithPin as Mock).mockResolvedValue({
      ...dailyPayStatus(false),
      oneShotGrant: mockGrant,
    });

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('base-access-mode').textContent).toBe('dailyPay');
    });

    let seenPwd: string | null = 'UNSET';
    let actionPromise!: Promise<unknown>;
    await act(async () => {
      actionPromise = sessionCtx.protectAction(async (pwd) => {
        seenPwd = pwd;
        return 'ephemeral-pin-ok';
      });
    });

    const pinInput = await screen.findByPlaceholderText('6–8 digit Quick PIN');
    fireEvent.change(pinInput, { target: { value: '246810' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    await expect(actionPromise).resolves.toBe('ephemeral-pin-ok');
    expect(seenPwd).toBe(mockGrant);
    expect(authService.unlockSessionWithPin).toHaveBeenCalledWith('246810', 'alice_folder');
    expect(authService.lockSession).not.toHaveBeenCalled();
    expect(screen.getByTestId('has-cached-session').textContent).toBe('false');
  });

  it('verifies and passes entered password in timeout=0 full mode without caching session', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(fullStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(0));
    (authService.unlockSession as Mock)
      .mockRejectedValueOnce(new Error('Invalid password'))
      .mockResolvedValueOnce(fullStatus(false));

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });

    let seenPwd: string | null = null;
    let actionCalls = 0;
    let actionPromise!: Promise<unknown>;
    await act(async () => {
      actionPromise = sessionCtx.protectAction(async (pwd) => {
        actionCalls += 1;
        seenPwd = pwd;
        return 't0-full-ok';
      });
    });

    fireEvent.change(await screen.findByPlaceholderText('Profile Password'), {
      target: { value: 'wrong-secret' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    expect(screen.getByText('Invalid password')).toBeInTheDocument();
    expect(actionCalls).toBe(0);
    expect(authService.unlockSession).toHaveBeenCalledWith('wrong-secret');

    fireEvent.change(await screen.findByPlaceholderText('Profile Password'), {
      target: { value: 'full-secret' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    await expect(actionPromise).resolves.toBe('t0-full-ok');
    expect(actionCalls).toBe(1);
    expect(seenPwd).toBe('full-secret');
    expect(authService.unlockSession).toHaveBeenLastCalledWith('full-secret');
    expect(screen.getByTestId('has-cached-session').textContent).toBe('false');
  });

  it('steps up from dailyPay PIN session on timeout=0 without leaving a persistent session cache', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(0));
    (authService.elevateSessionWithPassword as Mock).mockResolvedValue(dailyPayStatus(false));

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('is-authenticated').textContent).toBe('true');
    });

    let capturedPwd: string | null = null;
    let actionPromise!: Promise<unknown>;
    await act(async () => {
      actionPromise = sessionCtx.protectAction(async (pwd) => {
        capturedPwd = pwd;
        return 'elevated-ok';
      }, { requireFullAccess: true });
    });

    const input = await screen.findByPlaceholderText('Profile Password');
    fireEvent.change(input, { target: { value: 'master-pw' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    await expect(actionPromise).resolves.toBe('elevated-ok');
    expect(capturedPwd).toBe('master-pw');
    expect(authService.elevateSessionWithPassword).toHaveBeenCalledWith('master-pw');
    expect(authService.unlockSession).not.toHaveBeenCalled();
    expect(screen.getByTestId('has-cached-session').textContent).toBe('false');
  });

  it('re-enqueues action in stepUp modal if PIN unlock in session modal hits insufficientAccessMode', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(300));
    (authService.unlockSessionWithPin as Mock).mockResolvedValue(dailyPayStatus(true));
    (authService.elevateSessionWithPassword as Mock).mockResolvedValue(dailyPayStatus(true, true));

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('base-access-mode').textContent).toBe('dailyPay');
    });

    let calls = 0;
    let actionPromise!: Promise<unknown>;
    await act(async () => {
      actionPromise = sessionCtx.protectAction(async () => {
        calls += 1;
        if (calls === 1) {
          throw { code: 'error.auth.insufficientAccessMode', message: 'InsufficientAccessMode' };
        }
        return 'stepup-retried-ok';
      });
    });

    // First submit PIN in session modal
    fireEvent.change(await screen.findByPlaceholderText('6–8 digit Quick PIN'), {
      target: { value: '246810' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    // Backend action rejected with insufficientAccessMode -> stepUp password modal opens
    const stepUpInput = await screen.findByPlaceholderText('Profile Password');
    expect(screen.getByText('Confirm Full Access with Master Password')).toBeInTheDocument();

    fireEvent.change(stepUpInput, { target: { value: 'master-pw' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    await expect(actionPromise).resolves.toBe('stepup-retried-ok');
    expect(calls).toBe(2);
    expect(authService.elevateSessionWithPassword).toHaveBeenCalledWith('master-pw');
  });

  it('allows switching from PIN modal to master password via switch button', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(300));
    (authService.elevateSessionWithPassword as Mock).mockResolvedValue(dailyPayStatus(true, true));

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('base-access-mode').textContent).toBe('dailyPay');
    });

    let actionPromise!: Promise<unknown>;
    await act(async () => {
      actionPromise = sessionCtx.protectAction(async () => 'switched-master-ok');
    });

    fireEvent.click(await screen.findByTestId('auth-modal-switch-password'));
    fireEvent.change(await screen.findByPlaceholderText('Profile Password'), {
      target: { value: 'master-pw' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    await expect(actionPromise).resolves.toBe('switched-master-ok');
    expect(authService.elevateSessionWithPassword).toHaveBeenCalledWith('master-pw');
    expect(screen.getByTestId('has-cached-session').textContent).toBe('true');
  });

  it('does not coalesce queued stepUp actions when timeout=0', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(0));
    (authService.elevateSessionWithPassword as Mock).mockResolvedValue(dailyPayStatus(false, true));

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('base-access-mode').textContent).toBe('dailyPay');
    });

    let p1!: Promise<unknown>;
    let p2!: Promise<unknown>;
    await act(async () => {
      p1 = sessionCtx.requireFullAccess(async (pwd) => `first:${pwd}`);
      p2 = sessionCtx.requireFullAccess(async (pwd) => `second:${pwd}`);
    });

    fireEvent.change(await screen.findByPlaceholderText('Profile Password'), {
      target: { value: 'pw-1' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });
    await expect(p1).resolves.toBe('first:pw-1');

    // Second queued stepUp must prompt independently when timeout === 0
    fireEvent.change(await screen.findByPlaceholderText('Profile Password'), {
      target: { value: 'pw-2' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });
    await expect(p2).resolves.toBe('second:pw-2');
    expect(authService.elevateSessionWithPassword).toHaveBeenCalledTimes(2);
  });

  it('K2: discards in-flight auth callback if logout/cancel happens before elevate resolves', async () => {
    (authService.getSessionStatus as Mock).mockResolvedValue(dailyPayStatus(false));
    (settingsService.getConfig as Mock).mockResolvedValue(makeAppConfig(300));

    let resolveElevate!: (status: ReturnType<typeof dailyPayStatus>) => void;
    (authService.elevateSessionWithPassword as Mock).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveElevate = resolve;
        }),
    );

    let sessionCtx!: ReturnType<typeof useSession>;
    render(
      <SessionProvider>
        <SessionConsumer onContextReady={(ctx) => { sessionCtx = ctx; }} />
      </SessionProvider>
    );

    await act(async () => {
      screen.getByText('Login Alice').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('base-access-mode').textContent).toBe('dailyPay');
    });

    const staleCallback = vi.fn().mockResolvedValue('should-never-run');
    let pendingPromise!: Promise<unknown>;
    await act(async () => {
      pendingPromise = sessionCtx.requireFullAccess(staleCallback);
    });
    const handledRejection = pendingPromise.catch((err) => err);

    fireEvent.change(await screen.findByPlaceholderText('Profile Password'), {
      target: { value: 'master-pw' },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    });

    expect(authService.elevateSessionWithPassword).toHaveBeenCalledWith('master-pw');

    // Cancel / log out before the backend call resolves
    await act(async () => {
      sessionCtx.notifyLogout();
    });

    const rejection = await handledRejection;
    expect(rejection).toBeInstanceOf(Error);
    expect((rejection as Error).message).toMatch(/logged out|cancelled/i);

    // Late resolution of the superseded unlock attempt must NOT run the callback or elevate state
    await act(async () => {
      resolveElevate(dailyPayStatus(true, true));
      await Promise.resolve();
    });

    expect(staleCallback).not.toHaveBeenCalled();
    expect(screen.getByTestId('is-authenticated').textContent).toBe('false');
    expect(screen.getByTestId('has-cached-session').textContent).toBe('false');
  });
});
