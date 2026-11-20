import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from '../App';

const mockNavigate = vi.fn();
const mockSetPendingDroppedFile = vi.fn();
const mockSessionState = {
  isForkLocked: false,
  isRecoveryRequired: false,
  clearLocks: vi.fn(),
  isAuthenticated: false,
  protectAction: vi.fn(async (fn: (pwd: string | null) => Promise<unknown>) => fn(null)),
};
const mockNavState: {
  appState: { view: string };
  navigate: typeof mockNavigate;
  pendingDroppedFile: null;
  setPendingDroppedFile: typeof mockSetPendingDroppedFile;
} = {
  appState: { view: 'needs_login' },
  navigate: mockNavigate,
  pendingDroppedFile: null,
  setPendingDroppedFile: mockSetPendingDroppedFile,
};

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: vi.fn(),
}));

vi.mock('../context/SessionContext', () => ({
  SessionProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  useSession: () => mockSessionState,
}));

vi.mock('../context/UiModeContext', () => ({
  UiModeProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

vi.mock('../context/NavigationContext', () => ({
  NavigationProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  useNavigation: () => mockNavState,
}));

vi.mock('../components/shell/Sidebar', () => ({
  Sidebar: () => <div data-testid="sidebar" />,
}));
vi.mock('../components/shell/Header', () => ({
  Header: () => <div data-testid="header" />,
}));
vi.mock('../components/shell/AppRouter', () => ({
  AppRouter: () => <div data-testid="router" />,
}));
vi.mock('../components/shell/GlobalFileDropZone', () => ({
  GlobalFileDropZone: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));
vi.mock('../components/auth/GuardianReleaseModal', () => ({
  GuardianReleaseModal: () => null,
}));
vi.mock('../components/profile/GlobalProfileEditorModal', () => ({
  GlobalProfileEditorModal: () => null,
}));
vi.mock('../components/shell/EventToastStack', () => ({
  EventToastStack: () => null,
}));
vi.mock('../components/auth/ForkLockOverlay', () => ({
  ForkLockOverlay: () => null,
}));

describe('App FIFO launch queue with deferred ack (H9)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSessionState.isAuthenticated = false;
    mockNavState.appState = { view: 'needs_login' };
  });

  it('keeps auth-required items un-acked until login and processes/acks them sequentially in FIFO order', async () => {
    const queue = [
      'humoco://pay?to=did:key:zAlice&amount=5',
      'humoco://contact?did=did:key:zBob&name=Bob',
    ];
    const acked: string[] = [];

    (invoke as Mock).mockImplementation(async (cmd: string, args?: { item?: string }) => {
      if (cmd === 'peek_startup_launch_items') return [...queue];
      if (cmd === 'ack_startup_launch_item' && args?.item) {
        acked.push(args.item);
        return true;
      }
      return undefined;
    });

    const { rerender } = render(<App />);

    await waitFor(() => {
      expect(screen.getByTestId('pending-deeplink-banner')).toBeInTheDocument();
    });
    expect(acked).toEqual([]);
    expect(mockNavigate).not.toHaveBeenCalledWith(
      expect.objectContaining({ view: 'send_vouchers' }),
    );

    // User signs in -> both queued items dispatch and ack in FIFO order
    mockSessionState.isAuthenticated = true;
    mockNavState.appState = { view: 'logged_in' };
    rerender(<App />);

    await waitFor(() => {
      expect(acked).toEqual([
        'humoco://pay?to=did:key:zAlice&amount=5',
        'humoco://contact?did=did:key:zBob&name=Bob',
      ]);
    });
    expect(mockNavigate).toHaveBeenNthCalledWith(1, {
      view: 'send_vouchers',
      initialPaymentUri: 'humoco://pay?to=did:key:zAlice&amount=5',
    });
    expect(mockNavigate).toHaveBeenNthCalledWith(2, {
      view: 'address_book',
      initialContactPayload: 'humoco://contact?did=did:key:zBob&name=Bob',
    });
  });

  it('dispatches and acks pre-login kit links immediately while logged out', async () => {
    const kitUri = 'humoco://kit?v=1&ent=abcd&lang=1&pass=0&sync=W10';
    const acked: string[] = [];

    (invoke as Mock).mockImplementation(async (cmd: string, args?: { item?: string }) => {
      if (cmd === 'peek_startup_launch_items') return [kitUri];
      if (cmd === 'ack_startup_launch_item' && args?.item) {
        acked.push(args.item);
        return true;
      }
      return undefined;
    });

    render(<App />);

    await waitFor(() => {
      expect(acked).toEqual([kitUri]);
    });
    expect(mockNavigate).toHaveBeenCalledWith({
      view: 'restore_backup',
      initialMethod: 'words_or_sheet',
      initialPayload: kitUri,
    });
  });
});
