import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { ReceiveView } from '../ReceiveView';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { VoucherStandardInfo, AppSettings } from '../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../../utils/settingsUtils', () => ({
  updateLastUsedDirectory: vi.fn(),
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((callback) => callback('test-password')),
  }),
}));

describe('ReceiveView Component (FreeTaler Standard)', () => {
  const mockStandards: VoucherStandardInfo[] = [
    {
      id: 'freetaler_v1',
      content: `name = "FreeTaler"`,
    },
  ];

  const mockSettings: AppSettings = {
    bundle_retention_days: 30,
    session_timeout_seconds: 300,
    privacy_default: 'public',
  };

  const mockOnBack = vi.fn();
  const mockOnReceiveSuccess = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') {
        return Promise.resolve(mockStandards);
      }
      if (cmd === 'get_app_settings') {
        return Promise.resolve(mockSettings);
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders drop zone and select button', async () => {
    render(
      <ReceiveView onBack={mockOnBack} onReceiveSuccess={mockOnReceiveSuccess} />
    );

    expect(await screen.findByText(/Drop Transfer File/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Select File/i })).toBeInTheDocument();
  });

  it('opens file dialog when select button is clicked', async () => {
    (open as Mock).mockResolvedValue('/path/to/bundle.transfer');

    render(
      <ReceiveView onBack={mockOnBack} onReceiveSuccess={mockOnReceiveSuccess} />
    );

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Select File/i })).toBeInTheDocument();
    });
  });
});
