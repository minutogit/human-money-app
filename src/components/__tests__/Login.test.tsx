import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { Login } from '../Login';
import { invoke } from '@tauri-apps/api/core';
import { ProfileInfo } from '../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-log', () => ({
  info: vi.fn(),
  error: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('@tauri-apps/plugin-log', () => ({
  info: vi.fn(),
  error: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
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
  const mockOnSwitchToRecreate = vi.fn();
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
        onSwitchToRecreate={mockOnSwitchToRecreate}
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
        onSwitchToRecreate={mockOnSwitchToRecreate}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    await waitFor(() => {
      const loginButton = screen.getByRole('button', { name: /Login/i });
      expect(loginButton).toBeDisabled();
    });
  });

  it('shows navigation buttons for creating and recreating profiles', async () => {
    render(
      <Login
        onLoginSuccess={mockOnLoginSuccess}
        onSwitchToCreate={mockOnSwitchToCreate}
        onSwitchToRecreate={mockOnSwitchToRecreate}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/Don't have a wallet\? Create one/i)).toBeInTheDocument();
      expect(screen.getByText(/Recreate profile from seed/i)).toBeInTheDocument();
      expect(screen.getByText(/Forgot password\?/i)).toBeInTheDocument();
    });
  });

  it('calls onLoginSuccess on valid credentials', async () => {
    const mockOnLoginSuccess = vi.fn();
    render(
      <Login 
        onLoginSuccess={mockOnLoginSuccess} 
        onSwitchToCreate={() => {}} 
        onSwitchToRecreate={() => {}} 
        onSwitchToReset={() => {}} 
      />
    );

    // Wait for profiles to load
    const passwordInput = await screen.findByLabelText(/Password/i);
    const loginButton = screen.getByRole('button', { name: /Login/i });

    // Fill form
    fireEvent.change(passwordInput, { target: { value: 'password123' } });
    
    // We need to wait for the button to be enabled (profiles loaded)
    await waitFor(() => {
        expect(loginButton).not.toBeDisabled();
    });

    fireEvent.click(loginButton);

    // Wait and check if callback was called
    await waitFor(() => {
      expect(mockOnLoginSuccess).toHaveBeenCalled();
    }, { timeout: 3000 });
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
        onSwitchToRecreate={mockOnSwitchToRecreate}
        onSwitchToReset={mockOnSwitchToReset}
      />
    );

    // Wait for trash button and click it to open delete password prompt modal
    const trashButton = await screen.findByTitle(/Delete Profile/i);
    fireEvent.click(trashButton);

    // Modal is open, find the password input and the submit button
    const passwordInput = await screen.findByLabelText(/Wallet Password/i);
    const verifyButton = screen.getByRole('button', { name: /Verify/i });

    // Enter incorrect password and click verify
    fireEvent.change(passwordInput, { target: { value: 'wrong-password' } });
    fireEvent.click(verifyButton);

    // Verify error is shown inside the modal
    await waitFor(() => {
      expect(screen.getByText(/Authentication Error: Invalid password/i)).toBeInTheDocument();
    });
  });
});
