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

  it('should strip whitespace from recipient ID on input', async () => {
    render(<SendView />);
    await waitFor(() => expect(screen.getByPlaceholderText(/did:key:z/i)).toBeInTheDocument());
    const recipientInput = screen.getByPlaceholderText(/did:key:z/i);
    fireEvent.change(recipientInput, { target: { value: 'did:key:z456 \t\n' } });
    await waitFor(() => {
      expect(recipientInput).toHaveValue('did:key:z456');
    });
  });

  it('parses standard TOML definitions into standardsDict', async () => {
    let parsedTomlContent = '';
    (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_voucher_summaries') return Promise.resolve(mockVouchers);
      if (cmd === 'get_user_id') return Promise.resolve('did:key:z789');
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_app_settings') return Promise.resolve(mockSettings);
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_active_asset_classes') return Promise.resolve([]);
      if (cmd === 'get_user_profile') return Promise.resolve(null);
      if (cmd === 'parse_standard_toml') {
        parsedTomlContent = String(args?.tomlContent || '');
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'a1b2c3d4-e5f6-4789-8012-3456789abcde', name: 'FreeTaler', abbreviation: 'Taler' },
            features: { privacyMode: 'stealth' },
          },
        });
      }
      return Promise.resolve(undefined);
    });

    render(<SendView />);

    await waitFor(() => {
      expect(parsedTomlContent).toContain('name = "FreeTaler"');
    });
  });

  it('applies payment request from recipient input preserving target amount', async () => {
    render(<SendView />);
    await waitFor(() => expect(screen.getByPlaceholderText(/did:key:z/i)).toBeInTheDocument());

    const recipientInput = screen.getByPlaceholderText(/did:key:z/i);
    const payUri = 'humoco://pay?to=did:key:z999&amount=42.50&std=FreeTaler&note=Honey';
    fireEvent.change(recipientInput, { target: { value: payUri } });

    await waitFor(() => {
      expect(recipientInput).toHaveValue('did:key:z999');
    });

    const targetAmountInput = screen.getByPlaceholderText('0.00');
    expect(targetAmountInput).toHaveValue(42.5);
    expect(screen.getByPlaceholderText(/note or reference/i)).toHaveValue('Honey');
  });

  it('sets target amount when initialVoucherId is provided', async () => {
    render(<SendView initialVoucherId="voucher1" />);
    await waitFor(() => {
      const targetAmountInput = screen.getByPlaceholderText('0.00');
      expect(targetAmountInput).toHaveValue(100);
    });
  });

  it('shows error feedback on invalid payment request in recipient input', async () => {
    render(<SendView />);
    await waitFor(() => expect(screen.getByPlaceholderText(/did:key:z/i)).toBeInTheDocument());

    const recipientInput = screen.getByPlaceholderText(/did:key:z/i);
    fireEvent.change(recipientInput, { target: { value: 'humoco://pay?invalid=true' } });

    await waitFor(() => {
      expect(screen.getByText(/No valid payment request/i)).toBeInTheDocument();
    });
  });

  it('shows sender name input and does not display Anonymous when get_user_profile returns empty profile', async () => {
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
        return Promise.resolve({});
      }
      return Promise.resolve(undefined);
    });

    render(<SendView />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/Your name for the recipient/i)).toBeInTheDocument();
    });

    const senderInput = screen.getByPlaceholderText(/Your name for the recipient/i);
    expect(senderInput).toHaveValue('');

    const anonymousCheckbox = screen.getByRole('checkbox', { name: /Send anonymously/i });
    expect(anonymousCheckbox).not.toBeChecked();

    expect(screen.queryByText('Anonymous')).not.toBeInTheDocument();
    expect(screen.queryByText('transfer.anonymous')).not.toBeInTheDocument();
  });

  it('hides sender name input when sendAnonymously is checked and passes senderProfileName: null to create_transfer_bundle', async () => {
    let createBundleArgs: Record<string, unknown> | undefined;

    (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
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
        createBundleArgs = args;
        return Promise.resolve({
          bundleData: [1, 2, 3],
          bundleId: 'bundle-123',
          involvedSourcesDetails: [],
        });
      }
      if (cmd === 'save_transaction_record') {
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });

    render(<SendView />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/Your name for the recipient/i)).toBeInTheDocument();
    });

    // Checkbox is initially unchecked, input is visible with default profile name
    const senderInput = screen.getByPlaceholderText(/Your name for the recipient/i);
    expect(senderInput).toHaveValue('Test User');

    const anonymousCheckbox = screen.getByRole('checkbox', { name: /Send anonymously/i });
    expect(anonymousCheckbox).not.toBeChecked();

    // Toggle anonymous checkbox to send anonymously
    fireEvent.click(anonymousCheckbox);
    expect(anonymousCheckbox).toBeChecked();

    // Sender name input should now be hidden and anonymous hint shown
    expect(screen.queryByPlaceholderText(/Your name for the recipient/i)).not.toBeInTheDocument();
    expect(screen.getByText(/Your name is hidden/i)).toBeInTheDocument();

    // Fill recipient ID
    const recipientInput = await screen.findByRole('textbox', { name: /User ID \(DID\)/i });
    fireEvent.change(recipientInput, { target: { value: 'did:key:z456' } });

    // Select voucher
    const freetalerVouchers = await screen.findAllByText('FreeTaler');
    fireEvent.click(freetalerVouchers[freetalerVouchers.length - 1]);

    // Click Send Vouchers
    const sendButton = await screen.findByRole('button', { name: /Send Vouchers/i });
    fireEvent.click(sendButton);

    // Confirm transfer
    const executeButton = await screen.findByRole('button', { name: /Execute/i });
    fireEvent.click(executeButton);

    // Assert create_transfer_bundle was invoked with senderProfileName: null
    await waitFor(() => {
      expect(createBundleArgs).toBeDefined();
      expect(createBundleArgs?.senderProfileName).toBeNull();
    });
  });
});

