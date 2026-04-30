import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { SendView } from '../SendView';
import { invoke } from '@tauri-apps/api/core';
import { VoucherSummary, VoucherStandardInfo, Contact, AppSettings } from '../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((callback) => callback('test-password')),
  }),
}));

describe('SendView Component (Silber Standard)', () => {
  const mockVouchers: VoucherSummary[] = [
    {
      local_instance_id: 'voucher1',
      status: 'Active',
      valid_until: '2025-12-31',
      creator_id: 'did:key:z123',
      description: 'Silver voucher',
      current_amount: '100.00',
      unit: 'AG',
      voucher_standard_name: 'Silver Standard',
      voucher_standard_uuid: 'silver-uuid-123',
      transaction_count: 0,
      guarantor_signatures_count: 0,
      additional_signatures_count: 0,
      has_collateral: false,
      is_test_voucher: false,
      display_currency: 'AG',
      display_standard_name: 'Silver Standard',
      divisible: true,
    },
  ];

  const mockStandards: VoucherStandardInfo[] = [
    {
      id: 'silver_v1',
      content: `name = "Silver Standard"`,
    },
  ];

  const mockContacts: Contact[] = [
    {
      did: 'did:key:z456',
      profile: {
        id: 'did:key:z456',
        first_name: 'Jane',
        last_name: 'Smith',
      },
      tags: ['friend'],
      added_at: '2024-01-01',
    },
  ];

  const mockSettings: AppSettings = {
    bundle_retention_days: 30,
    session_timeout_seconds: 300,
    privacy_default: 'public',
  };

  const mockOnBack = vi.fn();
  const mockOnTransferPrepared = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_summaries') {
        return Promise.resolve(mockVouchers);
      }
      if (cmd === 'get_user_id') {
        return Promise.resolve('did:key:z789');
      }
      if (cmd === 'get_voucher_standards') {
        return Promise.resolve(mockStandards);
      }
      if (cmd === 'get_app_settings') {
        return Promise.resolve(mockSettings);
      }
      if (cmd === 'get_contacts') {
        return Promise.resolve(mockContacts);
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders form fields for sending', async () => {
    render(
      <SendView
        onBack={mockOnBack}
        onTransferPrepared={mockOnTransferPrepared}
        profileName="Test Profile"
      />
    );

    await waitFor(() => {
      expect(screen.getByLabelText(/Recipient User ID/i)).toBeInTheDocument();
      expect(screen.getByPlaceholderText(/did:key:z/i)).toBeInTheDocument();
    });
  });

  it('loads vouchers and contacts on mount', async () => {
    render(
      <SendView
        onBack={mockOnBack}
        onTransferPrepared={mockOnTransferPrepared}
        profileName="Test Profile"
      />
    );

    await waitFor(() => {
      expect(invoke).toHaveBeenCalled();
    });
  });
});
