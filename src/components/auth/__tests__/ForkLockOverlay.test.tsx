import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { ForkLockOverlay } from '../ForkLockOverlay';
import { invoke } from '@tauri-apps/api/core';
import { UiModeProvider } from '../../../context/UiModeContext';

const mockNavigate = vi.fn();
vi.mock('../../../context/NavigationContext', () => ({
  useNavigation: () => ({
    navigate: mockNavigate,
  }),
}));

const mockClearLocks = vi.fn();
const mockNotifyLogout = vi.fn();
const mockNotifyLogin = vi.fn();

let mockLockdownKind: 'handover' | 'clone' | 'fork' | null = 'handover';
let mockLockdownDetails: { targetEpoch?: number; isSeedRecovery?: boolean } | null = { targetEpoch: 5 };

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    lockdownKind: mockLockdownKind,
    lockdownDetails: mockLockdownDetails,
    profileName: 'Alice',
    clearLocks: mockClearLocks,
    notifyLogout: mockNotifyLogout,
    notifyLogin: mockNotifyLogin,
  }),
  useOptionalSession: () => null,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('ForkLockOverlay Component', () => {
  const mockOnStartRecovery = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockLockdownKind = 'handover';
    mockLockdownDetails = { targetEpoch: 5 };

    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') {
        return Promise.resolve([
          { profileName: 'Alice', folderName: 'alice_folder' },
        ]);
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('inst_123');
      }
      if (cmd === 'handover_to_this_device') {
        return Promise.resolve(undefined);
      }
      if (cmd === 'logout') {
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders calm handover view without epoch noise in simple mode and opens multiDevice help', async () => {
    render(
      <UiModeProvider>
        <ForkLockOverlay onStartRecovery={mockOnStartRecovery} />
      </UiModeProvider>
    );

    await waitFor(() => {
      expect(screen.getByText(/(Profile|Wallet) Active on Another Device/i)).toBeInTheDocument();
      expect(screen.getByText(/Payments on this device are paused/i)).toBeInTheDocument();
      expect(screen.queryByTestId('forklock-epoch-detail')).not.toBeInTheDocument();
      expect(screen.getByTestId('forklock-help-button')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Switch Profile \/ Log Out/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Remove Local Profile from This Device/i })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('forklock-help-button'));
    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument();
      expect(screen.getByText(/Using Your Profile on Multiple Devices/i)).toBeInTheDocument();
    });
  });

  it('reveals epoch detail inside handover view in expert mode', async () => {
    render(
      <UiModeProvider initialMode="expert">
        <ForkLockOverlay onStartRecovery={mockOnStartRecovery} />
      </UiModeProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('forklock-epoch-detail')).toHaveTextContent(/Epoch 5/i);
    });
  });

  it('logs out and clears locks when clicking Switch Profile in handover view', async () => {
    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    const logoutBtn = screen.getByRole('button', { name: /Switch Profile \/ Log Out/i });
    fireEvent.click(logoutBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('logout');
      expect(mockNotifyLogout).toHaveBeenCalled();
      expect(mockClearLocks).toHaveBeenCalled();
      expect(mockNavigate).toHaveBeenCalledWith({ view: 'needs_login' });
    });
  });

  it('allows reclaiming wallet from handover view', async () => {
    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    // Toggle reclaim section
    const toggleBtn = screen.getByText(/Reactivate (Profile|Wallet) on This Device/i);
    fireEvent.click(toggleBtn);

    const passwordInput = await screen.findByPlaceholderText(/Password/i);
    fireEvent.change(passwordInput, { target: { value: 'secret' } });

    const reclaimSubmitBtn = screen.getByRole('button', { name: /Reclaim (Profile|Wallet) on This Device/i });
    fireEvent.click(reclaimSubmitBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('handover_to_this_device', {
        folderName: 'alice_folder',
        password: 'secret',
        localInstanceId: 'inst_123',
      });
      expect(mockClearLocks).toHaveBeenCalled();
      expect(mockNotifyLogin).toHaveBeenCalledWith('Alice', 'alice_folder');
    });
  });

  it('renders clone security alert when lockdownKind is clone', async () => {
    mockLockdownKind = 'clone';
    mockLockdownDetails = null;

    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    await waitFor(() => {
      expect(screen.getByText(/Multi-Device Conflict Detected/i)).toBeInTheDocument();
      expect(screen.getByTestId('forklock-help-button')).toBeInTheDocument();
    });
    const recoverBtn = screen.getByRole('button', { name: /Recover (Profile|Wallet) with (Seed Phrase|Recovery Words)/i });
    expect(recoverBtn).toBeInTheDocument();

    fireEvent.click(recoverBtn);
    expect(mockOnStartRecovery).toHaveBeenCalled();
  });

  it('renders fork lockdown view when lockdownKind is fork', async () => {
    mockLockdownKind = 'fork';
    mockLockdownDetails = null;

    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    await waitFor(() => {
      expect(screen.getByText(/Device Synchronization Conflict Detected/i)).toBeInTheDocument();
      expect(screen.getByText(/Your profile history diverged across devices/i)).toBeInTheDocument();
      expect(screen.getByTestId('forklock-help-button')).toBeInTheDocument();
    });
  });

  it('offers profile selector when multiple local profiles match ambiguously and reclaims chosen profile', async () => {
    mockLockdownKind = 'handover';
    mockLockdownDetails = { targetEpoch: 5 };
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') {
        return Promise.resolve([
          { profileName: 'Alice', folderName: 'alice_1' },
          { profileName: 'Alice', folderName: 'alice_2' },
        ]);
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('inst_123');
      }
      return Promise.resolve(undefined);
    });

    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    const selector = await screen.findByTestId('fork-lock-profile-select');
    fireEvent.click(screen.getByText(/Reactivate (Profile|Wallet) on This Device/i));

    const passwordInput = await screen.findByPlaceholderText(/Password/i);
    fireEvent.change(passwordInput, { target: { value: 'secret' } });
    fireEvent.click(screen.getByRole('button', { name: /Reclaim (Profile|Wallet) on This Device/i }));

    // Shows explicit error when no profile has been chosen yet and does not invoke handover
    await waitFor(() => {
      expect(screen.getByText(/No profile selected|Kein Profil ausgewählt/i)).toBeInTheDocument();
    });
    expect(invoke).not.toHaveBeenCalledWith('handover_to_this_device', expect.anything());

    fireEvent.change(selector, { target: { value: 'alice_2' } });
    fireEvent.click(screen.getByRole('button', { name: /Reclaim (Profile|Wallet) on This Device/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('handover_to_this_device', {
        folderName: 'alice_2',
        password: 'secret',
        localInstanceId: 'inst_123',
      });
    });
  });

  it('shows error and keeps locks intact when handover reclaim fails', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') {
        return Promise.resolve([{ profileName: 'Alice', folderName: 'alice_folder' }]);
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('inst_123');
      }
      if (cmd === 'handover_to_this_device') {
        return Promise.reject(new Error('Bad master password'));
      }
      return Promise.resolve(undefined);
    });

    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    fireEvent.click(await screen.findByText(/Reactivate (Profile|Wallet) on This Device/i));
    const passwordInput = await screen.findByPlaceholderText(/Password/i);
    fireEvent.change(passwordInput, { target: { value: 'wrongpass' } });
    fireEvent.click(screen.getByRole('button', { name: /Reclaim (Profile|Wallet) on This Device/i }));

    expect(await screen.findByText('Bad master password')).toBeInTheDocument();
    expect(mockClearLocks).not.toHaveBeenCalled();
    expect(mockNotifyLogin).not.toHaveBeenCalled();
  });

  it('deletes local profile with password verification and navigates to needs_profile while keeping locks on failure', async () => {
    let profileDeleted = false;
    let shouldFailDelete = true;
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') {
        return Promise.resolve(
          profileDeleted ? [] : [{ profileName: 'Alice', folderName: 'alice_folder' }]
        );
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('inst_123');
      }
      if (cmd === 'delete_profile') {
        if (shouldFailDelete) {
          return Promise.reject(new Error('Invalid password'));
        }
        profileDeleted = true;
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });

    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    fireEvent.click(
      await screen.findByRole('button', { name: /Remove Local Profile from This Device/i })
    );

    const confirmDeleteBtn = await screen.findByRole('button', { name: /Delete Profile/i });
    expect(confirmDeleteBtn).toBeDisabled();

    const passwordInput = screen.getByPlaceholderText(/Password/i);

    // Negative baseline: wrong password displays error and leaves locks/navigation untouched
    fireEvent.change(passwordInput, { target: { value: 'wrong' } });
    fireEvent.click(confirmDeleteBtn);

    expect(await screen.findByText('Invalid password')).toBeInTheDocument();
    expect(mockClearLocks).not.toHaveBeenCalled();
    expect(mockNotifyLogout).not.toHaveBeenCalled();
    expect(mockNavigate).not.toHaveBeenCalled();

    // Positive path: valid password deletes profile, clears locks, and opens onboarding when none remain
    shouldFailDelete = false;
    fireEvent.change(passwordInput, { target: { value: 'secret' } });
    fireEvent.click(confirmDeleteBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('delete_profile', {
        folderName: 'alice_folder',
        password: 'secret',
      });
      expect(mockClearLocks).toHaveBeenCalled();
      expect(mockNotifyLogout).toHaveBeenCalled();
      expect(mockNavigate).toHaveBeenCalledWith({ view: 'needs_profile' });
    });
  });
});
