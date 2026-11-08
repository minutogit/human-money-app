/* eslint-disable @typescript-eslint/no-explicit-any */
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { ForkLockOverlay } from '../ForkLockOverlay';
import { invoke } from '@tauri-apps/api/core';

const mockNavigate = vi.fn();
vi.mock('../../context/NavigationContext', () => ({
  useNavigation: () => ({
    navigate: mockNavigate,
  }),
}));

const mockClearLocks = vi.fn();
const mockNotifyLogout = vi.fn();
const mockNotifyLogin = vi.fn();

let mockLockdownKind: 'handover' | 'clone' | 'fork' | null = 'handover';
let mockLockdownDetails: { targetEpoch?: number; isSeedRecovery?: boolean } | null = { targetEpoch: 5 };

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    lockdownKind: mockLockdownKind,
    lockdownDetails: mockLockdownDetails,
    profileName: 'Alice',
    clearLocks: mockClearLocks,
    notifyLogout: mockNotifyLogout,
    notifyLogin: mockNotifyLogin,
  }),
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

  it('renders calm handover view when lockdownKind is handover', async () => {
    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    await waitFor(() => {
      expect(screen.getByText(/Wallet Active on Another Device/i)).toBeInTheDocument();
      expect(screen.getByText(/Epoch 5/i)).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Switch Profile \/ Log Out/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Remove Local Profile from This Device/i })).toBeInTheDocument();
    });
  });

  it('logs out and clears locks when clicking Switch Profile in handover view', async () => {
    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    const logoutBtn = screen.getByRole('button', { name: /Switch Profile \/ Log Out/i });
    fireEvent.click(logoutBtn);

    await waitFor(() => {
      expect(mockNotifyLogout).toHaveBeenCalled();
      expect(mockClearLocks).toHaveBeenCalled();
      expect(mockNavigate).toHaveBeenCalledWith({ view: 'needs_login' });
    });
  });

  it('allows reclaiming wallet from handover view', async () => {
    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    // Toggle reclaim section
    const toggleBtn = screen.getByText(/Reactivate Wallet on This Device/i);
    fireEvent.click(toggleBtn);

    const passwordInput = await screen.findByPlaceholderText(/Password/i);
    fireEvent.change(passwordInput, { target: { value: 'secret' } });

    const reclaimSubmitBtn = screen.getByRole('button', { name: /Reclaim Wallet on This Device/i });
    fireEvent.click(reclaimSubmitBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('handover_to_this_device', {
        folderName: 'alice_folder',
        password: 'secret',
        localInstanceId: 'inst_123',
      });
      expect(mockClearLocks).toHaveBeenCalled();
      expect(mockNotifyLogin).toHaveBeenCalledWith('Alice');
    });
  });

  it('renders clone security alert when lockdownKind is clone', async () => {
    mockLockdownKind = 'clone';
    mockLockdownDetails = null;

    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    await waitFor(() => {
      expect(screen.getByText(/Security Alert: Concurrent Clone Detected/i)).toBeInTheDocument();
    });
    const recoverBtn = screen.getByRole('button', { name: /Recover Wallet with Seed Phrase/i });
    expect(recoverBtn).toBeInTheDocument();

    fireEvent.click(recoverBtn);
    expect(mockOnStartRecovery).toHaveBeenCalled();
  });

  it('renders fork lockdown view when lockdownKind is fork', async () => {
    mockLockdownKind = 'fork';
    mockLockdownDetails = null;

    render(<ForkLockOverlay onStartRecovery={mockOnStartRecovery} />);

    await waitFor(() => {
      expect(screen.getByText(/Security Lockdown/i)).toBeInTheDocument();
      expect(screen.getByText(/A fork has been detected in your transaction history/i)).toBeInTheDocument();
    });
  });
});
