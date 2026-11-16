import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { SendView } from '../SendView';
import { invoke } from '@tauri-apps/api/core';
import { VoucherSummary, VoucherStandardInfo, Contact, AppSettings } from '../../types';
import { UI_MODE_STORAGE_KEY } from '../../context/UiModeContext';

const { mockNavigate } = vi.hoisted(() => ({
  mockNavigate: vi.fn(),
}));

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((callback) => callback('test-password')),
    profileName: 'Test Profile',
  }),
  useOptionalSession: () => null,
}));

vi.mock('../../context/NavigationContext', () => ({
  useNavigation: () => ({
    navigate: mockNavigate,
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
    marketSyncEnabled: true,
  };



  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.setItem(UI_MODE_STORAGE_KEY, 'expert');
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
      expect(screen.getByRole('textbox', { name: /User ID/i })).toBeInTheDocument();
      expect(screen.getByRole('textbox', { name: /User ID/i })).toBeInTheDocument();
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
    const recipientInput = await screen.findByRole('textbox', { name: /User ID/i });
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
    await waitFor(() => expect(screen.getByRole('textbox', { name: /User ID/i })).toBeInTheDocument());
    const recipientInput = screen.getByRole('textbox', { name: /User ID/i });
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
    await waitFor(() => expect(screen.getByRole('textbox', { name: /User ID/i })).toBeInTheDocument());

    const recipientInput = screen.getByRole('textbox', { name: /User ID/i });
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
    await waitFor(() => expect(screen.getByRole('textbox', { name: /User ID/i })).toBeInTheDocument());

    const recipientInput = screen.getByRole('textbox', { name: /User ID/i });
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
    const recipientInput = await screen.findByRole('textbox', { name: /User ID/i });
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

  it('keeps #target-amount visible even when "All" standard filter is active and auto-selects when typing', async () => {
    render(<SendView />);

    await waitFor(() => {
      expect(screen.getAllByText('FreeTaler').length).toBeGreaterThan(0);
    });

    // Click "All" pill (sets selectedStandardId = null)
    const allPill = screen.getByRole('button', { name: /All|Alle/i });
    fireEvent.click(allPill);

    const amountInput = document.getElementById('target-amount') as HTMLInputElement | null;
    expect(amountInput).toBeInTheDocument();

    fireEvent.change(amountInput!, { target: { value: '25' } });
    await waitFor(() => {
      expect(amountInput).toHaveValue(25);
      expect(screen.getByText(/1 Vouchers Selected/i)).toBeInTheDocument();
    });
  });

  it('never mixes real and test vouchers of the same standard in selection', async () => {
    const mixedVouchers: VoucherSummary[] = [
      {
        ...mockVouchers[0],
        localInstanceId: 'real-voucher-1',
        currentAmount: '50.00',
        isTestVoucher: false,
        validUntil: '2026-12-31',
      },
      {
        ...mockVouchers[0],
        localInstanceId: 'test-voucher-1',
        currentAmount: '100.00',
        isTestVoucher: true,
        validUntil: '2026-06-30',
      },
    ];

    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_summaries') return Promise.resolve(mixedVouchers);
      if (cmd === 'get_user_id') return Promise.resolve('did:key:z789');
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_app_settings') return Promise.resolve(mockSettings);
      if (cmd === 'get_contacts') return Promise.resolve(mockContacts);
      if (cmd === 'get_active_asset_classes') {
        return Promise.resolve([
          {
            standardUuid: 'a1b2c3d4-e5f6-4789-8012-3456789abcde',
            isTestVoucher: true,
            displayStandardName: 'FreeTaler (Test)',
            displayCurrency: 'Taler',
          },
          {
            standardUuid: 'a1b2c3d4-e5f6-4789-8012-3456789abcde',
            isTestVoucher: false,
            displayStandardName: 'FreeTaler',
            displayCurrency: 'Taler',
          },
        ]);
      }
      if (cmd === 'get_user_profile') return Promise.resolve({ firstName: 'Test', lastName: 'User' });
      return Promise.resolve(undefined);
    });

    render(<SendView />);

    await waitFor(() => {
      expect(document.getElementById('target-amount')).toBeInTheDocument();
    });

    // Type an amount greater than the single 50 real voucher: must NOT pull in test-voucher-1
    const amountInput = document.getElementById('target-amount') as HTMLInputElement;
    fireEvent.change(amountInput, { target: { value: '120' } });

    await waitFor(() => {
      expect(screen.getByText(/1 Vouchers Selected/i)).toBeInTheDocument();
      // Only the 1 real voucher (50.00) is selected, not 2 (150.00)
      expect(screen.queryByText(/2 Vouchers Selected/i)).not.toBeInTheDocument();
      expect(screen.queryByText(/150[.,]00/)).not.toBeInTheDocument();
    });
  });

  it('APP-UX-09 P3 Simple Mode: hides sender card, raw recipient DID input, and collapses manual voucher selection', async () => {
    localStorage.setItem(UI_MODE_STORAGE_KEY, 'simple');

    const { container } = render(<SendView />);

    await waitFor(() => {
      expect(screen.getByTestId('quick-contact-did:key:z456')).toBeInTheDocument();
      expect(document.getElementById('target-amount')).toBeInTheDocument();
    });

    expect(screen.queryByText(/Your name for the recipient/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Send anonymously/i)).not.toBeInTheDocument();
    expect(document.getElementById('recipientId')).not.toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/did:key:z/i)).not.toBeInTheDocument();
    expect(screen.getByTestId('manual-voucher-selection-toggle')).toBeInTheDocument();
    expect(container.textContent).not.toContain('did:key:');
  });

  it('APP-UX-09 P3 Simple Mode: quick contact + amount sends with profile senderProfileName', async () => {
    localStorage.setItem(UI_MODE_STORAGE_KEY, 'simple');

    const baseImpl = (invoke as Mock).getMockImplementation();
    (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'create_transfer_bundle') {
        return Promise.resolve({
          bundleData: [1, 2, 3],
          bundleId: 'bundle-123',
          involvedSourcesDetails: [],
        });
      }
      if (cmd === 'save_transaction_record') {
        return Promise.resolve(undefined);
      }
      if (cmd === 'deposit_transfer_to_post') {
        return Promise.resolve({ msgId: 'm1', depositedTo: 1, errors: [] });
      }
      return baseImpl ? baseImpl(cmd, args) : Promise.resolve(undefined);
    });

    render(<SendView />);

    const janeBtn = await screen.findByTestId('quick-contact-did:key:z456');
    fireEvent.click(janeBtn);

    const amountInput = document.getElementById('target-amount') as HTMLInputElement;
    fireEvent.change(amountInput, { target: { value: '25' } });

    await screen.findByText(/1 Vouchers Selected/i, undefined, { timeout: 2000 });

    const submitBtn = screen.getByRole('button', { name: /Send Vouchers/i });
    expect(submitBtn).not.toBeDisabled();
    fireEvent.click(submitBtn);

    const confirmBtn = await screen.findByRole('button', { name: /Execute/i });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'create_transfer_bundle',
        expect.objectContaining({
          recipientId: 'did:key:z456',
          senderProfileName: 'Test User',
        })
      );
    });
  });

  it('warns instead of applying target amount when requested standard UUID is missing locally', async () => {
    render(<SendView />);
    await waitFor(() => expect(screen.getByRole('textbox', { name: /User ID/i })).toBeInTheDocument());

    const recipientInput = screen.getByRole('textbox', { name: /User ID/i });
    const payUri =
      'humoco://pay?to=did:key:z999&amount=42.50&std=11111111-1111-1111-1111-111111111111&stdName=SonnenTaler';
    fireEvent.change(recipientInput, { target: { value: payUri } });

    await waitFor(() => {
      expect(screen.getByText(/SonnenTaler/i)).toBeInTheDocument();
    });

    expect(recipientInput).toHaveValue('did:key:z999');
    const targetAmountInput = document.getElementById('target-amount') as HTMLInputElement;
    expect(targetAmountInput.value).toBe('');
  });

  it('still navigates to transfer_success with translated depositError when sync server deposit fails', async () => {
    const baseImpl = (invoke as Mock).getMockImplementation();
    (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'create_transfer_bundle') {
        return Promise.resolve({
          bundleData: [1, 2, 3],
          bundleId: 'bundle-123',
          involvedSourcesDetails: [],
          l2Dispatch: [],
        });
      }
      if (cmd === 'save_transaction_record') {
        return Promise.resolve(undefined);
      }
      if (cmd === 'deposit_transfer_to_post') {
        return Promise.reject({
          code: 'error.network.l2Offline',
          message: 'Sync server unreachable',
          details: {},
        });
      }
      return baseImpl ? baseImpl(cmd, args) : Promise.resolve(undefined);
    });

    render(<SendView />);

    await waitFor(() => expect(screen.getByRole('textbox', { name: /User ID/i })).toBeInTheDocument());
    fireEvent.change(screen.getByRole('textbox', { name: /User ID/i }), {
      target: { value: 'did:key:z456' },
    });

    const amountInput = document.getElementById('target-amount') as HTMLInputElement;
    fireEvent.change(amountInput, { target: { value: '25' } });
    await screen.findByText(/1 Vouchers Selected/i, undefined, { timeout: 2000 });

    fireEvent.click(screen.getByRole('button', { name: /Send Vouchers/i }));
    const confirmBtn = await screen.findByRole('button', { name: /Execute/i });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('save_transaction_record', expect.any(Object));
    });
    expect(screen.queryByText(/Transfer failed/i)).not.toBeInTheDocument();

    await waitFor(
      () => {
        expect(mockNavigate).toHaveBeenCalledWith(
          expect.objectContaining({
            view: 'transfer_success',
            bundleId: 'bundle-123',
            depositedTo: 0,
            depositError: expect.any(String),
          })
        );
      },
      { timeout: 2500 }
    );
    const navArg = mockNavigate.mock.calls[0][0];
    expect(navArg.depositError).not.toBe('[object Object]');
  });

  it('displays dedicated splitChangePendingL2 message instead of generic validation failure', async () => {
    const baseImpl = (invoke as Mock).getMockImplementation();
    (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'create_transfer_bundle') {
        return Promise.reject({
          code: 'error.validation.splitChangePendingL2',
          message: 'Validation Error: Security violation: Split change voucher is blocked by unresolved or conflicted parent intent',
          details: {},
        });
      }
      return baseImpl ? baseImpl(cmd, args) : Promise.resolve(undefined);
    });

    render(<SendView />);

    await waitFor(() => expect(screen.getByRole('textbox', { name: /User ID/i })).toBeInTheDocument());
    fireEvent.change(screen.getByRole('textbox', { name: /User ID/i }), {
      target: { value: 'did:key:z456' },
    });

    const amountInput = document.getElementById('target-amount') as HTMLInputElement;
    fireEvent.change(amountInput, { target: { value: '25' } });
    await screen.findByText(/1 Vouchers Selected/i, undefined, { timeout: 2000 });

    fireEvent.click(screen.getByRole('button', { name: /Send Vouchers/i }));
    const confirmBtn = await screen.findByRole('button', { name: /Execute/i });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(screen.getByText(/awaiting Layer 2 confirmation/i)).toBeInTheDocument();
    });
    expect(screen.queryByText(/Validation failed\./i)).not.toBeInTheDocument();
  });
});

