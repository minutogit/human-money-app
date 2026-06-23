import { render, screen, waitFor, fireEvent } from '@testing-library/react';
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
    profileName: 'Test Profile',
  }),
}));

vi.mock('../../context/NavigationContext', () => ({
  useNavigation: () => ({
    navigate: vi.fn(),
    goBack: vi.fn(),
    appState: { view: 'send_vouchers' },
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
      allowPartialTransfers: true,
    },
  ];

  const mockStandards: VoucherStandardInfo[] = [
    {
      id: 'freetaler_v1',
      displayName: 'FreeTaler',
      content: `name = "FreeTaler"
issuer_name = "Human Money Project"
unit = "Taler"
abbreviation = "Taler"
default_validity_duration = "P1Y"
amount_decimal_places = 4`,
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
    render(<SendView />);

    await waitFor(() => {
      expect(screen.getByRole('textbox', { name: /User ID \(DID\)/i })).toBeInTheDocument();
      expect(screen.getByPlaceholderText(/did:key:z/i)).toBeInTheDocument();
    });
  });

  it('loads vouchers and contacts on mount', async () => {
    render(<SendView />);

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
      displayName: 'PrecisionStd',
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

    render(<SendView />);

    await waitFor(() => expect(screen.getAllByText(/PrecisionStd/i).length).toBeGreaterThan(0));
    expect(await screen.findByText(/100.12/)).toBeInTheDocument();
    expect(screen.queryByText('100.1234')).not.toBeInTheDocument();
  });

  it('translates BackendError correctly instead of showing [object Object]', async () => {
    const backendError = {
      code: 'error.validation.insufficientFunds',
      message: 'Insufficient funds: Needed: 100, Available: 50',
      details: {
        needed: '100',
        available: '50',
      },
    };

    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_summaries') return Promise.resolve(mockVouchers);
      if (cmd === 'get_user_id') return Promise.resolve('did:key:z789');
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_app_settings') return Promise.resolve(mockSettings);
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
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
      if (cmd === 'create_transfer_bundle') {
        return Promise.reject(backendError);
      }
      return Promise.resolve(undefined);
    });

    render(<SendView />);

    // Type recipient ID
    const recipientInput = await screen.findByRole('textbox', { name: /User ID \(DID\)/i });
    fireEvent.change(recipientInput, { target: { value: 'did:key:z456' } });

    // Click on the voucher card to select it
    const freetalerVouchers = await screen.findAllByText('FreeTaler');
    const freetalerVoucher = freetalerVouchers[freetalerVouchers.length - 1];
    fireEvent.click(freetalerVoucher);

    // Click "Send Vouchers" (transfer.sendVoucher)
    const sendButton = await screen.findByRole('button', { name: /Send Vouchers/i });
    fireEvent.click(sendButton);

    // Click "Execute" in the confirmation modal
    const executeButton = await screen.findByRole('button', { name: /Execute/i });
    fireEvent.click(executeButton);

    // Assert that the feedback message is displayed and contains the translated error
    // In en.json: "transfer.transferFailed": "Transfer failed: {{error}}"
    // and "error.validation.insufficientFunds": "Insufficient funds. Needed: {{needed}}, Available: {{available}}."
    // So the translation for the whole message should be:
    // "Transfer failed: Insufficient funds. Needed: 100, Available: 50"
    await waitFor(() => {
      const feedbackAlert = screen.getByText(/Transfer failed:/i);
      expect(feedbackAlert).toBeInTheDocument();
      expect(feedbackAlert.textContent).toContain('Insufficient funds. Needed: 100, Available: 50');
      expect(feedbackAlert.textContent).not.toContain('[object Object]');
    });
  });
});
