import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { Login } from '../Login';
import { invoke } from '@tauri-apps/api/core';
import { ProfileInfo } from '../../../types';

const mockNavigate = vi.fn();
vi.mock('../../../context/NavigationContext', () => ({
  useNavigation: () => ({
    navigate: mockNavigate,
    goBack: vi.fn(),
    appState: { view: 'needs_login' },
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-log', () => ({
  info: vi.fn(),
  error: vi.fn(),
}));

vi.mock('../../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

describe('Login Component', () => {
  const mockProfiles: ProfileInfo[] = [
    { profileName: 'Test Profile 1', folderName: 'profile1' },
    { profileName: 'Test Profile 2', folderName: 'profile2' },
  ];

  const mockOnLoginSuccess = vi.fn();
  const mockOnSwitchToCreate = vi.fn();
  const mockOnSwitchToRestoreBackup = vi.fn();
  const mockOnSwitchToReset = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') {
        return Promise.resolve(mockProfiles);
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('test-instance-id');
      }
      if (cmd === 'login') {
        return Promise.resolve(true);
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders profile dropdown and password field', async () => {
    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRestoreBackup={mockOnSwitchToRestoreBackup}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    await waitFor(() => {
      expect(screen.getByLabelText(/Select Profile/i)).toBeInTheDocument();
      expect(screen.getByLabelText(/Password/i)).toBeInTheDocument();
    });
  });

  it('disables login button when password is empty', async () => {
    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRestoreBackup={mockOnSwitchToRestoreBackup}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    await waitFor(() => {
      const loginButton = screen.getByRole('button', { name: /Login/i });
      expect(loginButton).toBeDisabled();
    });
  });

  it('shows navigation buttons for creating, opening/restoring, and forgot password', async () => {
    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRestoreBackup={mockOnSwitchToRestoreBackup}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/\[\+\] CREATE NEW (PROFILE|WALLET)/i)).toBeInTheDocument();
      expect(screen.getByText(/OPEN \/ RESTORE EXISTING PROFILE/i)).toBeInTheDocument();
      expect(screen.getByText(/Forgot password\?/i)).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText(/OPEN \/ RESTORE EXISTING PROFILE/i));
    expect(mockOnSwitchToRestoreBackup).toHaveBeenCalled();

    fireEvent.click(screen.getByText(/Forgot password\?/i));
    expect(mockOnSwitchToReset).toHaveBeenCalled();
  });

  it('calls onLoginSuccess on valid credentials', async () => {
    const onSuccess = vi.fn();
    render(
      <Login
        onLoginSuccess={onSuccess}
        onSwitchToCreate={() => {}}
        onSwitchToRestoreBackup={() => {}}
        onSwitchToReset={() => {}}
      />
    );

    const passwordInput = await screen.findByLabelText(/Password/i);
    const loginButton = screen.getByRole('button', { name: /Login/i });

    fireEvent.change(passwordInput, { target: { value: 'password123' } });

    await waitFor(() => {
      expect(loginButton).not.toBeDisabled();
    });

    fireEvent.click(loginButton);

    await waitFor(
      () => {
        expect(invoke).toHaveBeenCalledWith('login', {
          folderName: 'profile1',
          password: 'password123',
          cleanupOnLogin: true,
          localInstanceId: 'test-instance-id',
        });
        expect(onSuccess).toHaveBeenCalledWith('Test Profile 1', 'profile1');
      },
      { timeout: 3000 }
    );
  });

  it('shows error message inside password prompt modal when verifyProfilePassword fails', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') {
        return Promise.resolve(mockProfiles);
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('test-instance-id');
      }
      if (cmd === 'verify_profile_password') {
        return Promise.reject(new Error('Invalid password'));
      }
      return Promise.resolve(undefined);
    });

    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRestoreBackup={mockOnSwitchToRestoreBackup}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    const trashButton = await screen.findByTitle(/Delete Profile/i);
    fireEvent.click(trashButton);

    const passwordInput = await screen.findByLabelText(/(Profile|Wallet) Password/i);
    const verifyButton = screen.getByRole('button', { name: /Verify/i });

    fireEvent.change(passwordInput, { target: { value: 'wrong-password' } });
    fireEvent.click(verifyButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('verify_profile_password', {
        folderName: 'profile1',
        password: 'wrong-password',
      });
      expect(screen.getByText(/Authentication Error: Invalid password/i)).toBeInTheDocument();
      expect(invoke).not.toHaveBeenCalledWith('delete_profile', expect.anything());
    });
  });

  it('shows calm handover UI when login fails with supersededByHandover', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') {
        return Promise.resolve(mockProfiles);
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('test-instance-id');
      }
      if (cmd === 'login') {
        return Promise.reject({
          code: 'error.security.supersededByHandover',
          message: 'Wallet superseded by handover',
          details: { targetEpoch: '3' },
        });
      }
      return Promise.resolve(undefined);
    });

    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRestoreBackup={mockOnSwitchToRestoreBackup}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    const passwordInput = await screen.findByLabelText(/Password/i);
    const loginButton = screen.getByRole('button', { name: /Login/i });

    fireEvent.change(passwordInput, { target: { value: 'password123' } });
    await waitFor(() => expect(loginButton).not.toBeDisabled());
    fireEvent.click(loginButton);

    await waitFor(() => {
      expect(screen.getByText(/(Profile|Wallet) Active on Another Device/i)).toBeInTheDocument();
      expect(screen.getByText(/Payments on this device are paused/i)).toBeInTheDocument();
      expect(screen.queryByText(/Epoch 3/i)).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Switch Profile/i })).toBeInTheDocument();
      expect(mockOnLoginSuccess).not.toHaveBeenCalled();
    });
  });

  it('shows clone security alert UI when login fails with concurrentCloneDetected and navigates via onSwitchToRestoreBackup', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') {
        return Promise.resolve(mockProfiles);
      }
      if (cmd === 'get_local_instance_id') {
        return Promise.resolve('test-instance-id');
      }
      if (cmd === 'login') {
        return Promise.reject({
          code: 'error.security.concurrentCloneDetected',
          message: 'Concurrent clone detected',
        });
      }
      return Promise.resolve(undefined);
    });

    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRestoreBackup={mockOnSwitchToRestoreBackup}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    const passwordInput = await screen.findByLabelText(/Password/i);
    const loginButton = screen.getByRole('button', { name: /Login/i });

    fireEvent.change(passwordInput, { target: { value: 'password123' } });
    await waitFor(() => expect(loginButton).not.toBeDisabled());
    fireEvent.click(loginButton);

    await waitFor(() => {
      expect(screen.getByText(/Multi-Device Conflict Detected/i)).toBeInTheDocument();
      expect(mockOnLoginSuccess).not.toHaveBeenCalled();
    });

    fireEvent.click(
      screen.getByRole('button', {
        name: /Recover (Profile|Wallet) with (Seed Phrase|Recovery Words)/i,
      })
    );
    expect(mockOnSwitchToRestoreBackup).toHaveBeenCalled();
  });

  it('retains password on deviceMismatch so handoverToThisDevice submits non-empty password', async () => {
    (invoke as Mock).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === 'list_profiles') return Promise.resolve(mockProfiles);
      if (cmd === 'get_local_instance_id') return Promise.resolve('test-instance-id');
      if (cmd === 'login') {
        return Promise.reject({
          code: 'error.auth.deviceMismatch',
          message: 'Device Mismatch',
        });
      }
      if (cmd === 'handover_to_this_device') return Promise.resolve(args);
      if (cmd === 'get_user_id') return Promise.resolve('did:key:zAlice');
      return Promise.resolve(undefined);
    });

    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRestoreBackup={mockOnSwitchToRestoreBackup}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    const passwordInput = await screen.findByLabelText(/Password/i);
    const loginButton = screen.getByRole('button', { name: /Login/i });

    fireEvent.change(passwordInput, { target: { value: 'SecretPass-1' } });
    await waitFor(() => expect(loginButton).not.toBeDisabled());
    fireEvent.click(loginButton);

    const handoverBtn = await screen.findByRole('button', { name: /Perform Device Handover|Geräteübergabe durchführen/i });
    expect(handoverBtn).not.toBeDisabled();
    fireEvent.click(handoverBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'handover_to_this_device',
        expect.objectContaining({
          password: 'SecretPass-1',
        })
      );
    });
  });

  it('clears password input and shows verification error when login rejects with wrong password', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') return Promise.resolve(mockProfiles);
      if (cmd === 'get_local_instance_id') return Promise.resolve('test-instance-id');
      if (cmd === 'login') return Promise.reject(new Error('Wrong password'));
      return Promise.resolve(undefined);
    });

    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRestoreBackup={mockOnSwitchToRestoreBackup}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    const passwordInput = await screen.findByLabelText(/Password/i);
    const loginButton = screen.getByRole('button', { name: /Login/i });

    fireEvent.change(passwordInput, { target: { value: 'WrongSecret-1' } });
    await waitFor(() => expect(loginButton).not.toBeDisabled());
    fireEvent.click(loginButton);

    await waitFor(() => {
      expect(screen.getByText('Verification Failure: Wrong password')).toBeInTheDocument();
      expect(passwordInput).toHaveValue('');
      expect(mockOnLoginSuccess).not.toHaveBeenCalled();
    });
  });

  it('handles PIN login failure counter and falls back to master password on pinWipedMaxAttempts', async () => {
    let pinAttempts = 0;
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'list_profiles') return Promise.resolve(mockProfiles);
      if (cmd === 'get_local_instance_id') return Promise.resolve('test-instance-id');
      if (cmd === 'get_profile_pin_status') {
        return Promise.resolve({
          enabled: true,
          scope: 'dailyPay',
          failedAttempts: 0,
          maxAttempts: 5,
          remainingAttempts: 5,
        });
      }
      if (cmd === 'login_with_pin') {
        pinAttempts += 1;
        if (pinAttempts === 1) {
          return Promise.reject({
            code: 'error.auth.invalidPin',
            message: 'Invalid PIN',
            details: { remaining: '4' },
          });
        }
        return Promise.reject({
          code: 'error.auth.pinWipedMaxAttempts',
          message: 'PIN slot wiped',
        });
      }
      return Promise.resolve(undefined);
    });

    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRestoreBackup={mockOnSwitchToRestoreBackup}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    const pinInput = await screen.findByTestId('login-pin-input');
    const submitBtn = screen.getByRole('button', { name: /Login/i });

    // 1st failed attempt: decrements counter, clears PIN, stays in PIN mode
    fireEvent.change(pinInput, { target: { value: '111111' } });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('login_with_pin', {
        folderName: 'profile1',
        pin: '111111',
        cleanupOnLogin: true,
        localInstanceId: 'test-instance-id',
      });
      expect(screen.getByText('Incorrect PIN. 4 attempt(s) remaining.')).toBeInTheDocument();
      expect(pinInput).toHaveValue('');
      expect(screen.getByTestId('login-pin-input')).toBeInTheDocument();
    });

    // Final attempt wipes PIN slot and switches auth mode back to master password
    fireEvent.change(screen.getByTestId('login-pin-input'), { target: { value: '222222' } });
    fireEvent.click(screen.getByRole('button', { name: /Login/i }));

    await waitFor(() => {
      expect(
        screen.getByText(
          'PIN removed after 5 failed attempts for your security. Please sign in with your master password.'
        )
      ).toBeInTheDocument();
      expect(screen.queryByTestId('login-pin-input')).not.toBeInTheDocument();
      expect(screen.getByLabelText(/Password/i)).toBeInTheDocument();
      expect(mockOnLoginSuccess).not.toHaveBeenCalled();
    });
  });
});
