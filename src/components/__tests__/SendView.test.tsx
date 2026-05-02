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

describe('SendView Component (FreeTaler Standard)', () => {
  const mockVouchers: VoucherSummary[] = [
    {
      localInstanceId: 'voucher1',
      status: 'active',
      validUntil: '2025-12-31',
      creatorId: 'did:key:z123',
      description: 'FreeTaler voucher',
      currentAmount: '100.00',
      unit: 'Taler',
      rawStandardName: 'FreeTaler',
      voucherStandardUuid: 'a1b2c3d4-e5f6-4789-8012-3456789abcde',
      transactionCount: 0,
      signaturesCount: 0,
      hasCollateral: false,
      isTestVoucher: false,
      displayCurrency: 'Taler',
      displayStandardName: 'FreeTaler',
      divisible: true,
    },
  ];

  const mockStandards: VoucherStandardInfo[] = [
    {
      id: 'freetaler_v1',
      content: `name = "FreeTaler"`,
    },
  ];

  const mockContacts: Contact[] = [
    {
      did: 'did:key:z456',
      profile: {
        id: 'did:key:z456',
        firstName: 'Jane',
        lastName: 'Smith',
      },
      tags: ['friend'],
      addedAt: '2024-01-01',
    },
  ];

  const mockSettings: AppSettings = {
    bundleRetentionDays: 30,
    sessionTimeoutSeconds: 300,
    privacyDefault: 'public',
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
      if (cmd === 'get_active_asset_classes') {
        return Promise.resolve([
          {
            standardUuid: 'a1b2c3d4-e5f6-4789-8012-3456789abcde',
            isTestVoucher: false,
            displayStandardName: 'FreeTaler',
            displayCurrency: 'Taler',
          },
        ]);
      }
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          organization: 'Test Org',
          firstName: 'Test',
          lastName: 'User',
        });
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
      expect(screen.getByRole('textbox', { name: /User ID \(DID\)/i })).toBeInTheDocument();
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

  it('formats amount in voucher list according to standard precision', async () => {
    const customVoucher: VoucherSummary = {
      ...mockVouchers[0],
      localInstanceId: 'precision-test',
      currentAmount: '100.1234',
      voucherStandardUuid: 'uuid-2',
      displayStandardName: 'PrecisionStd',
    };
    const customStandard: VoucherStandardInfo = {
      id: 'precision_std',
      content: 'name = "PrecisionStd"\nuuid = "uuid-2"\namount_decimal_places = 2',
    };

    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_summaries') return Promise.resolve([customVoucher]);
      if (cmd === 'get_voucher_standards') return Promise.resolve([customStandard]);
      if (cmd === 'get_user_id') return Promise.resolve('did:key:z789');
      if (cmd === 'get_app_settings') return Promise.resolve(mockSettings);
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_active_asset_classes') return Promise.resolve([{
        standardUuid: 'uuid-2',
        isTestVoucher: false,
        displayStandardName: 'PrecisionStd',
        displayCurrency: 'Taler',
      }]);
      if (cmd === 'get_user_profile') return Promise.resolve({});
      return Promise.resolve(undefined);
    });

    render(
      <SendView
        onBack={mockOnBack}
        onTransferPrepared={mockOnTransferPrepared}
        profileName="Test Profile"
      />
    );

    await waitFor(() => expect(screen.getAllByText(/PrecisionStd/i).length).toBeGreaterThan(0));
    expect(await screen.findByText(/100.12/)).toBeInTheDocument();
    expect(screen.queryByText('100.1234')).not.toBeInTheDocument();
  });
});
